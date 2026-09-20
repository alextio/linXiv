//! Citation refs cluster: verify `linxiv://paper/{source_fk}?v={version}`
//! handles a client pasted into notes against the live library.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router, ErrorData};
use schemars::JsonSchema;
use serde::Deserialize;

use linxiv_core::service::refs as svc_refs;

use crate::util::{core_err, guard_err, json_ok};
use crate::Server;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResolveRefsParams {
    /// Citation handles to verify, each `linxiv://paper/{source_fk}?v={version}`.
    pub refs: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PaperRefParams {
    /// Paper source id (e.g. "arxiv:2204.12985").
    pub paper_id: String,
    /// Stored version to pin; the latest stored version when omitted.
    #[serde(default)]
    pub version: Option<i64>,
}

#[tool_router(router = tools_refs, vis = "pub(crate)")]
impl Server {
    #[tool(
        description = "Citation handle linxiv://paper/{source_fk}?v={version} for a saved paper, pinned to a stored version (latest by default). Paste it into notes; verify later with resolve_refs."
    )]
    pub async fn paper_ref(
        &self,
        Parameters(p): Parameters<PaperRefParams>,
    ) -> Result<String, ErrorData> {
        self.with_conn(|conn| {
            json_ok(&svc_refs::paper_ref(conn, &p.paper_id, p.version).map_err(guard_err)?)
        })
    }

    #[tool(
        description = "Verify citation refs (linxiv://paper/{source_fk}?v={version}). Each comes back current, stale (newer version stored), unknown (no such paper or version), or malformed."
    )]
    pub async fn resolve_refs(
        &self,
        Parameters(p): Parameters<ResolveRefsParams>,
    ) -> Result<String, ErrorData> {
        let out = self
            .with_conn(|conn| svc_refs::resolve_refs(conn, &p.refs))
            .map_err(core_err)?;
        json_ok(&out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use linxiv_core::models::PaperMetadata;
    use linxiv_core::service::paper as svc_paper;
    use linxiv_core::storage;
    use serde_json::Value;
    use std::sync::Arc;

    fn meta(source_id: &str) -> PaperMetadata {
        serde_json::from_value(serde_json::json!({
            "source_id": source_id, "version": 1, "title": "T", "authors": ["A"],
            "published": "2024-01-01", "summary": "S", "source": "arxiv"
        }))
        .unwrap()
    }

    fn server() -> Server {
        let conn = storage::open_in_memory().unwrap();
        storage::init_db(&conn).unwrap();
        Server {
            db: Arc::new(linxiv_core::service::db_admin::Db::held(conn)),
            pdf_dir: std::env::temp_dir(),
            tool_router: Server::tools_refs(),
        }
    }

    /// The tool emits `RefResolution` rows in input order, keyed `ref`.
    #[tokio::test]
    async fn resolve_refs_reports_each_input_in_order() {
        let srv = server();
        let fk = srv
            .with_conn(|conn| {
                svc_paper::save_paper_metadata(conn, &meta("arxiv:A"), None)?;
                svc_paper::resolve_source_fk(conn, "arxiv:A")
            })
            .unwrap();
        let handle: Value = serde_json::from_str(
            &srv.paper_ref(Parameters(PaperRefParams {
                paper_id: "arxiv:A".into(),
                version: None,
            }))
            .await
            .unwrap(),
        )
        .unwrap();
        assert_eq!(handle["ref"], format!("linxiv://paper/{fk}?v=1"));
        let out = srv
            .resolve_refs(Parameters(ResolveRefsParams {
                refs: vec![handle["ref"].as_str().unwrap().into(), "junk".into()],
            }))
            .await
            .unwrap();
        let out: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(out[0]["status"], "current");
        assert_eq!(out[0]["source_id"], "arxiv:A");
        assert_eq!(out[1]["status"], "malformed");
        assert_eq!(out[1]["ref"], "junk");
    }
}
