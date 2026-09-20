//! ORCID tools cluster: the on-demand backfill pass over DOI-linked authors.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router, ErrorData};
use schemars::JsonSchema;
use serde::Deserialize;

use linxiv_core::service::orcid_backfill as svc;
use linxiv_core::service::orcid_backfill::OrcidBackfillResponse;

use crate::util::{core_err, guard_err, invalid, json_ok};
use crate::Server;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OrcidBackfillParams {
    /// Random ORCID-less authors to check this pass, 1 to 100 (default 20).
    #[serde(default)]
    pub limit: Option<i64>,
}

#[tool_router(router = tools_orcid, vis = "pub(crate)")]
impl Server {
    // Route parity: `POST /api/orcid/backfill`.
    #[tool(
        description = "Fill missing ORCIDs onto DOI-linked authors via CrossRef then OpenAlex. One paced pass; only one may run at a time."
    )]
    pub async fn orcid_backfill(
        &self,
        Parameters(p): Parameters<OrcidBackfillParams>,
    ) -> Result<String, ErrorData> {
        let limit = p.limit.unwrap_or(svc::DEFAULT_LIMIT);
        svc::validate_limit(limit).map_err(guard_err)?;
        let _guard = svc::try_begin_backfill().ok_or_else(|| invalid(svc::BUSY_MSG))?;
        let candidates = self
            .with_conn(|conn| svc::orcid_backfill_candidates(conn, limit))
            .map_err(core_err)?;
        let (fetched, errored) = svc::fetch_orcid_records(&candidates).await;
        let updated = self
            .with_conn(|conn| svc::apply_results(conn, &candidates, &fetched))
            .map_err(core_err)?;
        json_ok(&OrcidBackfillResponse {
            checked: candidates.len(),
            updated,
            errored,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use linxiv_core::storage;
    use std::sync::Arc;

    fn server() -> Server {
        let conn = storage::open_in_memory().unwrap();
        storage::init_db(&conn).unwrap();
        Server {
            db: Arc::new(linxiv_core::service::db_admin::Db::held(conn)),
            pdf_dir: std::env::temp_dir(),
            tool_router: Server::tools_orcid(),
        }
    }

    /// Bad limit and busy slot both refuse before the candidate query; an
    /// empty library returns the zero envelope without touching the network.
    #[tokio::test]
    async fn backfill_guards_then_zero_envelope_on_empty_library() {
        let srv = server();
        let err = srv
            .orcid_backfill(Parameters(OrcidBackfillParams { limit: Some(0) }))
            .await
            .unwrap_err();
        assert_eq!(err.message.as_ref(), "limit must be between 1 and 100");

        let busy = svc::try_begin_backfill().expect("slot free");
        let err = srv
            .orcid_backfill(Parameters(OrcidBackfillParams { limit: None }))
            .await
            .unwrap_err();
        assert_eq!(err.message.as_ref(), svc::BUSY_MSG);
        drop(busy);

        let out = srv
            .orcid_backfill(Parameters(OrcidBackfillParams { limit: None }))
            .await
            .unwrap();
        assert_eq!(out, r#"{"checked":0,"updated":[],"errored":0}"#);
    }
}
