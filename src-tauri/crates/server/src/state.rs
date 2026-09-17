//! In-process backend state: the database handle plus the managed PDF/vault
//! roots. Every router arm reaches the DB via `with_conn`.

use std::path::PathBuf;

use rusqlite::Connection;

use linxiv_core::config;
use linxiv_core::service::db_admin::Db;

pub struct AppState {
    db: Db,
    /// Managed PDF directory (`config::pdf_dir()`), used by the PDF/export arms.
    pub pdf_dir: PathBuf,
    /// LaTeX vault root (`config::vault_dir()`), used by the editor arms.
    pub vault_root: PathBuf,
}

impl AppState {
    /// Resolve + create the data dir, open the DB, run schema init. Absent a
    /// LINXIV_DATA_DIR override it matches Tauri's `app_data_dir()` (D24).
    pub fn new() -> anyhow::Result<Self> {
        config::init_data_dir()?;
        // Pin the persistent device actor so every CRDT change is attributable.
        if let Err(e) = linxiv_share::init_device_actor(&config::data_dir()) {
            eprintln!("device actor init failed (history will be per-run): {e}");
        }
        // Lazy: init (and its migrations) run once here, then the connection is
        // released, so an idle app or node does not block `restore`.
        Ok(Self {
            db: Db::lazy()?,
            pdf_dir: config::pdf_dir(),
            vault_root: config::vault_dir(),
        })
    }

    /// Build from already-resolved parts — the DI seam for tests. Not `cfg(test)`
    /// because downstream crates' tests (linxiv-app) build through it too.
    pub fn from_parts(conn: Connection, pdf_dir: PathBuf, vault_root: PathBuf) -> Self {
        Self {
            db: Db::held(conn),
            pdf_dir,
            vault_root,
        }
    }

    /// Runs `f` against the database, serialized process-wide. `f` is sync, so
    /// the lock can never span an `.await`. See `db_admin::Db` for why the
    /// connection itself is per-call in production.
    ///
    /// TODO: maybe parallel reads, serial writes if peers contend.
    pub fn with_conn<T>(&self, f: impl FnOnce(&mut Connection) -> T) -> T {
        self.db.with(f)
    }
}
