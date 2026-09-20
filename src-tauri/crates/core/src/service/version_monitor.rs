//! version_monitor — polling for new arXiv versions, separate from the
//! opportunistic capture that happens on refetch. Each pass checks the stalest N
//! saved arXiv papers (tracked per-root in VERSION_CHECK) and records any version
//! newer than the max already stored via the EXISTING write path
//! (`write_paper_version_in_tx`, what `save_paper_metadata` wraps).
//!
//! Each adapter (route, CLI, MCP) runs one pass as `try_begin_check` →
//! `stale_candidates` → one batched `fetch_latest` → `apply_results`.
//! Everything but that one network hop is sync + unit-testable offline.

use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::Connection;

use crate::config;
use crate::error::{CoreError, Result};
use crate::models::{OkReceipt, PaperMetadata};
use crate::storage::db::transaction;
use crate::storage::queries::paper as store;
pub use crate::storage::queries::version_check::{
    ack, list_new_versions, record_check, stale_candidates, Candidate, NewVersion,
    MAX_VERSION_CHECK_BATCH,
};

/// Papers polled per pass when the caller gives no limit.
pub const DEFAULT_LIMIT: i64 = 20;

/// Busy message shared by every surface when a pass is already running.
pub const CHECK_BUSY_MSG: &str = "version check already in progress";

/// A pass polls 1..=MAX_VERSION_CHECK_BATCH papers; anything else is a 422.
pub fn validate_limit(limit: i64) -> Result<i64> {
    if !(1..=MAX_VERSION_CHECK_BATCH).contains(&limit) {
        return Err(CoreError::Validation(format!(
            "limit must be between 1 and {MAX_VERSION_CHECK_BATCH}"
        )));
    }
    Ok(limit)
}

static CHECK_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Held for the duration of one pass; the single-flight flag clears on Drop.
pub struct CheckGuard(());

impl Drop for CheckGuard {
    fn drop(&mut self) {
        CHECK_IN_PROGRESS.store(false, Ordering::SeqCst);
    }
}

/// Claim the single-flight slot; `None` while another pass is running.
// ponytail: process-wide flag, per-DB slot if one process ever serves several DBs.
pub fn try_begin_check() -> Option<CheckGuard> {
    (!CHECK_IN_PROGRESS.swap(true, Ordering::SeqCst)).then_some(CheckGuard(()))
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
pub struct VersionCheckResponse {
    pub checked: usize,
    pub new_versions: Vec<NewVersion>,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
pub struct NewVersionsResponse {
    pub new_versions: Vec<NewVersion>,
}

/// Latest arXiv metadata for the candidates in ONE rate-limited request (none
/// when there are no candidates). Batched and arXiv-only, so it stays outside the
/// per-Provider dispatch (ADR-0010). Hold no DB lock across it.
pub async fn fetch_latest(candidates: &[Candidate]) -> Result<Vec<PaperMetadata>> {
    if candidates.is_empty() {
        return Ok(vec![]);
    }
    let ids: Vec<String> = candidates.iter().map(|c| c.source_id.clone()).collect();
    crate::sources::arxiv::fetch_by_ids(&ids, &config::data_dir()).await
}

/// Clear one paper's new-version flag; `NotFound` (404) when nothing was flagged.
pub fn ack_flagged(conn: &Connection, source_fk: i64) -> Result<OkReceipt> {
    if !ack(conn, source_fk)? {
        return Err(CoreError::NotFound(
            "no new version flagged for this paper".into(),
        ));
    }
    Ok(OkReceipt { ok: true })
}

/// Process one candidate: for active, resolvable roots save any newer version and
/// record the check (new version or None). Deleted/inactive roots skip both, as
/// does an error — leaving the candidate at the front of the staleness queue.
fn process_candidate(
    conn: &mut Connection,
    cand: &Candidate,
    fetched: &[PaperMetadata],
) -> Result<Option<NewVersion>> {
    let Some(root) = store::get_paper_root(conn, &cand.source_id)? else {
        return Ok(None);
    };
    if root.status != "active" {
        return Ok(None);
    }
    let newer = fetched
        .iter()
        .find(|m| m.source_id == cand.source_id)
        .filter(|m| m.version > cand.known_version);
    if let Some(m) = newer {
        // Save + flag as one transaction: either both land or neither does, so a
        // crash can't raise known_version with the discovery unflagged.
        transaction(conn, |tx| {
            store::write_paper_version_in_tx(tx, m, None)?;
            record_check(tx, root.source_fk, Some(m.version))
        })?;
        let result = NewVersion {
            source_fk: root.source_fk,
            source_id: cand.source_id.clone(),
            title: m.title.clone(),
            version: m.version,
        };
        return Ok(Some(result));
    }
    record_check(conn, root.source_fk, None)?;
    Ok(None)
}

/// Apply one pass's fetched metadata: capture each candidate's newer-than-known
/// version and record the check; per-candidate errors are logged and swallowed.
pub fn apply_results(
    conn: &mut Connection,
    candidates: &[Candidate],
    fetched: &[PaperMetadata],
) -> Result<Vec<NewVersion>> {
    let mut found = Vec::new();
    for cand in candidates {
        match process_candidate(conn, cand, fetched) {
            Ok(Some(new_version)) => found.push(new_version),
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("error checking candidate {}: {}", cand.source_id, e);
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{db, meta};

    fn save(conn: &mut Connection, source_id: &str, version: i64) {
        store::save_paper_metadata(conn, &meta(source_id, version), None).unwrap();
    }

    fn fk(conn: &Connection, source_id: &str) -> i64 {
        store::get_paper_root(conn, source_id)
            .unwrap()
            .unwrap()
            .source_fk
    }

    #[test]
    fn stale_candidates_selects_arxiv_never_checked_then_oldest() {
        let mut conn = db();
        save(&mut conn, "arxiv:a", 1);
        save(&mut conn, "arxiv:b", 2);
        save(&mut conn, "arxiv:c", 1);
        save(&mut conn, "local:x", 1); // non-arXiv: excluded
        save(&mut conn, "arxiv:gone", 1);
        store::soft_delete_paper(&mut conn, "arxiv:gone").unwrap(); // deleted: excluded

        // b checked long ago, c checked just now, a never checked.
        record_check(&conn, fk(&conn, "arxiv:b"), None).unwrap();
        conn.execute(
            "UPDATE VERSION_CHECK SET LAST_CHECKED_AT = '2000-01-01 00:00:00' WHERE SOURCE_FK = ?1",
            [fk(&conn, "arxiv:b")],
        )
        .unwrap();
        record_check(&conn, fk(&conn, "arxiv:c"), None).unwrap();

        let ids: Vec<String> = stale_candidates(&conn, 10)
            .unwrap()
            .into_iter()
            .map(|c| c.source_id)
            .collect();
        assert_eq!(ids, vec!["arxiv:a", "arxiv:b", "arxiv:c"]);

        // known_version is the max stored version; limit is respected.
        let top = stale_candidates(&conn, 1).unwrap();
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].source_id, "arxiv:a");
        assert_eq!(top[0].known_version, 1);
    }

    #[test]
    fn apply_results_captures_only_newer_and_rotates_all() {
        let mut conn = db();
        save(&mut conn, "arxiv:up", 1);
        save(&mut conn, "arxiv:same", 3);
        save(&mut conn, "arxiv:silent", 1); // arXiv returns nothing for it

        let cands = stale_candidates(&conn, 10).unwrap();
        let fetched = vec![meta("arxiv:up", 2), meta("arxiv:same", 3)];
        let found = apply_results(&mut conn, &cands, &fetched).unwrap();

        // Only the strictly-newer version was captured + flagged.
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source_id, "arxiv:up");
        assert_eq!(found[0].version, 2);
        assert_eq!(store::get_all_versions(&conn, "arxiv:up").unwrap().len(), 2);
        assert_eq!(
            store::get_all_versions(&conn, "arxiv:same").unwrap().len(),
            1
        );

        // Every candidate (even the unanswered one) got LAST_CHECKED_AT, so a
        // fresh never-checked paper now outranks all three.
        save(&mut conn, "arxiv:fresh", 1);
        let next = stale_candidates(&conn, 1).unwrap();
        assert_eq!(next[0].source_id, "arxiv:fresh");

        // The discovery is listed until acked; a later no-news pass keeps it.
        let listed = list_new_versions(&conn).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].source_id, "arxiv:up");
        assert_eq!(listed[0].version, 2);
        record_check(&conn, fk(&conn, "arxiv:up"), None).unwrap();
        assert_eq!(list_new_versions(&conn).unwrap().len(), 1);

        assert!(ack(&conn, fk(&conn, "arxiv:up")).unwrap());
        assert!(list_new_versions(&conn).unwrap().is_empty());
        assert!(!ack(&conn, fk(&conn, "arxiv:up")).unwrap());
    }

    #[test]
    fn apply_results_skips_deleted_candidates_no_fk_crash() {
        let mut conn = db();
        save(&mut conn, "arxiv:live", 1);
        save(&mut conn, "arxiv:deleted", 1);

        let cands = stale_candidates(&conn, 10).unwrap();
        assert_eq!(cands.len(), 2);

        // Delete one root's row (CASCADE deletes VERSION_CHECK and PAPER dependents).
        conn.execute(
            "DELETE FROM PAPER_ROOTS WHERE SOURCE_ID = ?1",
            ["arxiv:deleted"],
        )
        .unwrap();

        // apply_results with metadata for both: should skip the deleted one, not crash.
        let fetched = vec![meta("arxiv:live", 2), meta("arxiv:deleted", 2)];
        let found = apply_results(&mut conn, &cands, &fetched).unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source_id, "arxiv:live");
        assert_eq!(found[0].version, 2);
    }

    #[test]
    fn limit_validation_and_single_flight_guard() {
        assert!(validate_limit(0).is_err());
        assert!(validate_limit(MAX_VERSION_CHECK_BATCH + 1).is_err());
        assert_eq!(validate_limit(DEFAULT_LIMIT).unwrap(), DEFAULT_LIMIT);

        let first = try_begin_check().expect("slot free");
        assert!(try_begin_check().is_none(), "second claim while held");
        drop(first);
        assert!(try_begin_check().is_some(), "slot released on drop");
    }

    /// Real transaction, not just careful ordering: if the second write in the
    /// pair (record_check) fails, the first (the version save) is rolled back
    /// too, instead of being left committed with the flag never set.
    #[test]
    fn save_and_record_check_are_one_atomic_transaction() {
        let mut conn = db();
        save(&mut conn, "arxiv:x", 1);
        let good_fk = fk(&conn, "arxiv:x");
        let bad_fk = good_fk + 999; // no matching PAPER_ROOTS row -> FK violation

        let m = meta("arxiv:x", 2);
        let result = crate::storage::db::transaction(&mut conn, |tx| {
            store::write_paper_version_in_tx(tx, &m, None)?;
            record_check(tx, bad_fk, Some(m.version))
        });
        assert!(result.is_err());

        // The version write ran first but must not have survived the rollback.
        assert_eq!(store::get_all_versions(&conn, "arxiv:x").unwrap().len(), 1);
    }
}
