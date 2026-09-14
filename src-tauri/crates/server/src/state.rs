//! In-process backend state: the single SQLite connection behind a `Mutex` plus
//! the managed PDF/vault roots. Every router arm reaches the DB via `with_conn`.

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

use linxiv_core::{config, storage};

pub struct AppState {
    conn: Mutex<Connection>,
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
        let conn = storage::open(&config::db_path())?;
        storage::init_db(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            pdf_dir: config::pdf_dir(),
            vault_root: config::vault_dir(),
        })
    }

    /// Build from already-resolved parts — the DI seam for tests. Not `cfg(test)`
    /// because downstream crates' tests (linxiv-app) build through it too.
    pub fn from_parts(conn: Connection, pdf_dir: PathBuf, vault_root: PathBuf) -> Self {
        Self {
            conn: Mutex::new(conn),
            pdf_dir,
            vault_root,
        }
    }

    /// Locks the shared connection for the duration of `f`. `f` is sync, so the
    /// guard can never span an `.await`. A poisoned mutex is recovered, not
    /// propagated — a `Connection` has no broken invariant to protect, and
    /// refusing the lock forever would take every DB route down for the process.
    ///
    /// TODO: maybe parallel reads, serial writes if peers contend.
    pub fn with_conn<T>(&self, f: impl FnOnce(&mut Connection) -> T) -> T {
        let mut guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard)
    }
}
