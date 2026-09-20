//! `/api/versions` routes — arXiv new-version monitoring. `check` polls the
//! stalest N saved papers in ONE arXiv request; `new`/`ack` surface and clear the flags.

use serde::Deserialize;
use serde_json::Value;

use linxiv_core::service::version_monitor::{
    self as svc, NewVersionsResponse, VersionCheckResponse,
};

use crate::route::{to_value, ApiError, ReqCtx};
use crate::state::AppState;

/// Returns `Some(result)` if this group owns `(method, path)`, else `None`.
pub(crate) async fn handle(state: &AppState, ctx: &ReqCtx<'_>) -> Option<Result<Value, ApiError>> {
    match (ctx.method, ctx.segs) {
        ("POST", ["api", "versions", "check"]) => Some(check(state, ctx).await),
        ("GET", ["api", "versions", "new"]) => Some(list_new(state)),
        ("POST", ["api", "versions", "ack"]) => Some(ack(state, ctx)),
        _ => None,
    }
}

fn default_limit() -> i64 {
    svc::DEFAULT_LIMIT
}

#[derive(Deserialize, ts_rs::TS)]
pub struct VersionsCheckBody {
    #[serde(default = "default_limit")]
    #[ts(as = "Option<i64>", optional)]
    pub limit: i64,
}

/// `POST /api/versions/check` — one poll pass over the stalest `limit` papers.
/// Candidates are read, the single batched arXiv call awaits with no DB lock
/// held, then results are applied. `502` on an arXiv failure (nothing recorded,
/// so the same papers stay at the front of the staleness queue).
async fn check(state: &AppState, ctx: &ReqCtx<'_>) -> Result<Value, ApiError> {
    // Body is optional: no body → defaults.
    let limit = match ctx.body {
        Some(_) => ctx.parse_body::<VersionsCheckBody>()?.limit,
        None => default_limit(),
    };
    let limit = svc::validate_limit(limit)?;
    let Some(_guard) = svc::try_begin_check() else {
        return Err(ApiError::new(409, svc::CHECK_BUSY_MSG));
    };

    let candidates = state.with_conn(|conn| svc::stale_candidates(conn, limit))?;
    let fetched = svc::fetch_latest(&candidates).await?;
    let found = state.with_conn(|conn| svc::apply_results(conn, &candidates, &fetched))?;
    to_value(&VersionCheckResponse {
        checked: candidates.len(),
        new_versions: found,
    })
}

/// `GET /api/versions/new` — papers with an un-acknowledged new version.
fn list_new(state: &AppState) -> Result<Value, ApiError> {
    let list = state.with_conn(|conn| svc::list_new_versions(conn))?;
    to_value(&NewVersionsResponse { new_versions: list })
}

#[derive(Deserialize, ts_rs::TS)]
pub struct VersionsAckBody {
    pub source_fk: i64,
}

/// `POST /api/versions/ack` — clear the flag for one paper. 404 when unset.
fn ack(state: &AppState, ctx: &ReqCtx<'_>) -> Result<Value, ApiError> {
    let b: VersionsAckBody = ctx.parse_body()?;
    let receipt = state.with_conn(|conn| svc::ack_flagged(conn, b.source_fk))?;
    to_value(&receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::testutil::{req, state};
    use serde_json::json;
    use tokio::sync::Mutex;

    // Serialize access to core's single-flight slot across all tests to prevent race
    // conditions (flag must reset on Drop even if test panics).
    //
    // tokio's Mutex, not std's: this guard is held across `.await`, and std's
    // poisons on panic — so the first failing test would make every sibling
    // panic in `lock()`, hiding which one actually broke.
    static TEST_MUTEX: Mutex<()> = Mutex::const_new(());

    #[tokio::test]
    async fn check_on_empty_library_returns_zero_without_network() {
        let _guard = TEST_MUTEX.lock().await;
        let v = req(&state(), "POST", "/api/versions/check", None)
            .await
            .unwrap();
        assert_eq!(v, json!({ "checked": 0, "new_versions": [] }));
    }

    #[tokio::test]
    async fn check_rejects_out_of_range_limit() {
        let _guard = TEST_MUTEX.lock().await;
        for limit in [0, 101, -3] {
            let err = req(
                &state(),
                "POST",
                "/api/versions/check",
                Some(json!({ "limit": limit })),
            )
            .await
            .unwrap_err();
            assert_eq!(err.status, 422, "limit={limit}");
        }
    }

    #[tokio::test]
    async fn check_in_progress_returns_409() {
        let _guard = TEST_MUTEX.lock().await;
        // Holding the core guard sets the flag and resets it on drop, including
        // on assertion panic, so a failed assert cannot leak a 409 into siblings.
        let _busy = svc::try_begin_check().expect("no other pass running");

        let err = req(&state(), "POST", "/api/versions/check", None)
            .await
            .unwrap_err();

        assert_eq!(err.status, 409);
    }

    #[tokio::test]
    async fn new_list_empty_and_ack_unset_is_404() {
        let st = state();
        let v = req(&st, "GET", "/api/versions/new", None).await.unwrap();
        assert_eq!(v, json!({ "new_versions": [] }));
        let err = req(
            &st,
            "POST",
            "/api/versions/ack",
            Some(json!({ "source_fk": 1 })),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, 404);
    }
}
