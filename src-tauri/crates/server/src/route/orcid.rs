//! `/api/orcid` routes: on-demand backfill pass, orchestrated by
//! `service::orcid_backfill` (guard → candidates → network → apply).

use serde::Deserialize;
use serde_json::Value;

use linxiv_core::service::orcid_backfill as svc;
use linxiv_core::service::orcid_backfill::OrcidBackfillResponse;

use crate::route::{to_value, ApiError, ReqCtx};
use crate::state::AppState;

/// Returns `Some(result)` if this group owns `(method, path)`, else `None`.
pub(crate) async fn handle(state: &AppState, ctx: &ReqCtx<'_>) -> Option<Result<Value, ApiError>> {
    match (ctx.method, ctx.segs) {
        ("POST", ["api", "orcid", "backfill"]) => Some(backfill(state, ctx).await),
        _ => None,
    }
}

fn default_limit() -> i64 {
    svc::DEFAULT_LIMIT
}

#[derive(Deserialize, ts_rs::TS)]
pub struct OrcidBackfillBody {
    #[serde(default = "default_limit")]
    #[ts(as = "Option<i64>", optional)]
    pub limit: i64,
}

/// `POST /api/orcid/backfill` — one pass over `limit` random ORCID-less,
/// DOI-linked authors, via CrossRef then OpenAlex per DOI (paced, one write).
async fn backfill(state: &AppState, ctx: &ReqCtx<'_>) -> Result<Value, ApiError> {
    let limit = match ctx.body {
        Some(_) => ctx.parse_body::<OrcidBackfillBody>()?.limit,
        None => default_limit(),
    };
    svc::validate_limit(limit)?;
    let Some(_guard) = svc::try_begin_backfill() else {
        return Err(ApiError::new(409, svc::BUSY_MSG));
    };

    let candidates = state.with_conn(|conn| svc::orcid_backfill_candidates(conn, limit))?;
    let (fetched, errored) = svc::fetch_orcid_records(&candidates).await;
    let updated = state.with_conn(|conn| svc::apply_results(conn, &candidates, &fetched))?;
    to_value(&OrcidBackfillResponse {
        checked: candidates.len(),
        updated,
        errored,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::testutil::{req, state};
    use serde_json::json;
    use tokio::sync::Mutex;

    // Serialize access to core's single-flight flag across all tests to prevent
    // races (flag must reset on Drop even if a test panics).
    //
    // tokio's Mutex, not std's: this guard is held across `.await`, and std's
    // poisons on panic — so the first failing test would make every sibling
    // panic in `lock()`, hiding which one actually broke.
    static TEST_MUTEX: Mutex<()> = Mutex::const_new(());

    #[tokio::test]
    async fn backfill_on_empty_library_returns_zero_without_network() {
        let _guard = TEST_MUTEX.lock().await;
        let v = req(&state(), "POST", "/api/orcid/backfill", None)
            .await
            .unwrap();
        assert_eq!(v, json!({ "checked": 0, "updated": [], "errored": 0 }));
    }

    #[tokio::test]
    async fn backfill_rejects_out_of_range_limit() {
        let _guard = TEST_MUTEX.lock().await;
        for limit in [0, 101, -3] {
            let err = req(
                &state(),
                "POST",
                "/api/orcid/backfill",
                Some(json!({ "limit": limit })),
            )
            .await
            .unwrap_err();
            assert_eq!(err.status, 422, "limit={limit}");
        }
    }

    #[tokio::test]
    async fn backfill_in_progress_returns_409() {
        let _guard = TEST_MUTEX.lock().await;
        // Holding core's guard releases the flag on drop, including on
        // assertion panic, so a failure here can't poison sibling tests.
        let _busy = svc::try_begin_backfill().expect("slot free");

        let err = req(&state(), "POST", "/api/orcid/backfill", None)
            .await
            .unwrap_err();

        assert_eq!(err.status, 409);
    }
}
