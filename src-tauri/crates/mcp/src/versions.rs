//! Version-monitor tools cluster: poll arXiv for new versions of saved papers,
//! then list and acknowledge the discoveries.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router, ErrorData};
use schemars::JsonSchema;
use serde::Deserialize;

use linxiv_core::error::CoreError;
use linxiv_core::service::paper as svc_paper;
use linxiv_core::service::version_monitor::{
    self as svc, NewVersionsResponse, VersionCheckResponse,
};

use crate::util::{core_err, guard_err, invalid, json_ok};
use crate::Server;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CheckVersionsParams {
    /// Stalest papers to poll in this pass, 1 to 100 (default 20).
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListNewVersionsParams {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AckVersionParams {
    /// Paper source id whose new-version flag to clear (e.g. "arxiv:2204.12985").
    pub paper_id: String,
}

#[tool_router(router = tools_versions, vis = "pub(crate)")]
impl Server {
    // Route parity: `POST /api/versions/check`.
    #[tool(
        description = "Poll arXiv once for new versions of the stalest saved papers. Newer \
                       versions are saved and flagged; one pass runs at a time."
    )]
    pub async fn check_versions(
        &self,
        Parameters(p): Parameters<CheckVersionsParams>,
    ) -> Result<String, ErrorData> {
        let limit =
            svc::validate_limit(p.limit.unwrap_or(svc::DEFAULT_LIMIT)).map_err(guard_err)?;
        let Some(_guard) = svc::try_begin_check() else {
            return Err(invalid(svc::CHECK_BUSY_MSG));
        };
        let candidates = self
            .with_conn(|conn| svc::stale_candidates(conn, limit))
            .map_err(core_err)?;
        // Network hop outside the DB lock.
        let fetched = svc::fetch_latest(&candidates).await.map_err(guard_err)?;
        let found = self
            .with_conn(|conn| svc::apply_results(conn, &candidates, &fetched))
            .map_err(core_err)?;
        json_ok(&VersionCheckResponse {
            checked: candidates.len(),
            new_versions: found,
        })
    }

    // Route parity: `GET /api/versions/new`.
    #[tool(description = "List saved papers with an unacknowledged new arXiv version.")]
    pub async fn list_new_versions(
        &self,
        Parameters(_p): Parameters<ListNewVersionsParams>,
    ) -> Result<String, ErrorData> {
        self.with_conn(|conn| {
            json_ok(&NewVersionsResponse {
                new_versions: svc::list_new_versions(conn).map_err(core_err)?,
            })
        })
    }

    // Route parity: `POST /api/versions/ack`.
    #[tool(description = "Clear the new-version flag on one paper.")]
    pub async fn ack_version(
        &self,
        Parameters(p): Parameters<AckVersionParams>,
    ) -> Result<String, ErrorData> {
        self.with_conn(|conn| {
            let source_fk =
                svc_paper::resolve_source_fk(conn, &p.paper_id).map_err(|e| match e {
                    e @ CoreError::PaperNotFound(_) => {
                        invalid(format!("{e}. Run fetch_paper first."))
                    }
                    other => core_err(other),
                })?;
            json_ok(&svc::ack_flagged(conn, source_fk).map_err(guard_err)?)
        })
    }
}
