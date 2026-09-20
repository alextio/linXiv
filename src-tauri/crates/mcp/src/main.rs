//! linXiv MCP server — the library tools over stdio JSON-RPC. The tools are
//! split across six cluster modules, each a `#[tool_router]` impl merged here.

mod annotations;
mod feed;
mod io_authors_misc;
mod notes_pdf_trash;
mod orcid;
mod papers;
mod projects_tags;
mod refs;
mod util;
mod versions;

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::transport::stdio;
use rmcp::{tool_handler, ServerHandler, ServiceExt};
use rusqlite::Connection;
use tracing_subscriber::EnvFilter;

use linxiv_core::config;
use linxiv_core::service::db_admin::Db;

/// Shared MCP server state: the database handle, the managed PDF root, and the
/// merged tool router.
#[derive(Clone)]
pub struct Server {
    db: Arc<Db>,
    /// Managed PDF directory (`config::pdf_dir()`). Used by the PDF tools.
    pdf_dir: PathBuf,
    tool_router: ToolRouter<Self>,
}

impl Server {
    /// Open the data dir, run startup init, then assemble the merged router.
    /// The connection is NOT kept: an editor session that is connected but idle
    /// must not hold the database open, or `restore` can never run.
    pub fn new() -> anyhow::Result<Self> {
        config::init_data_dir()?;
        Ok(Self {
            db: Arc::new(Db::lazy()?),
            pdf_dir: config::pdf_dir(),
            tool_router: Self::tools_papers()
                + Self::tools_projects_tags()
                + Self::tools_notes_pdf_trash()
                + Self::tools_annotations()
                + Self::tools_io_authors_misc()
                + Self::tools_versions()
                + Self::tools_orcid()
                + Self::tools_feed()
                + Self::tools_refs(),
        })
    }

    /// Runs `f` against the database, serialized across this server's tool calls.
    pub fn with_conn<T>(&self, f: impl FnOnce(&mut Connection) -> T) -> T {
        self.db.with(f)
    }

    /// The handle for work that must run off the async runtime: `with_conn`
    /// blocks a tokio worker, fine for fast statements but not whole-DB file I/O.
    pub fn db(&self) -> Arc<Db> {
        Arc::clone(&self.db)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "linXiv: search, fetch, organize, and annotate academic papers. Cite papers as \
                 linxiv://paper/{source_fk}?v={version} using the source_fk and version fields \
                 returned by the paper tools, and verify citations with resolve_refs.",
        )
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // stdout is the JSON-RPC channel; all logs must go to stderr.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let server = Server::new()?;
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
