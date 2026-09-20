//! orcid_backfill — fills ORCIDs onto existing authors via paper DOIs.
//! Adapters: `validate_limit` → `try_begin_backfill` → candidates (own conn)
//! → `fetch_orcid_records().await` → `apply_results` (own conn).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rusqlite::Connection;

use crate::error::{CoreError, Result};
use crate::models::PaperMetadata;
use crate::service::source::orcid_records_for_doi;
use crate::storage::queries::author::fill_orcid_if_null;
pub use crate::storage::queries::author::{orcid_backfill_candidates, OrcidCandidate};

pub const DEFAULT_LIMIT: i64 = 20;
/// Worded once so every surface reports the single-flight refusal identically.
pub const BUSY_MSG: &str = "an ORCID backfill is already in progress";

/// Pause between distinct DOIs so a pass never bursts CrossRef/OpenAlex,
/// neither of which is rate-limited by `sources::http` the way arXiv is.
const INTER_DOI_DELAY: Duration = Duration::from_millis(200);

/// `limit` must be 1..=100; `Validation` (422) otherwise.
pub fn validate_limit(limit: i64) -> Result<()> {
    if (1..=100).contains(&limit) {
        Ok(())
    } else {
        Err(CoreError::Validation(
            "limit must be between 1 and 100".into(),
        ))
    }
}

// ponytail: process-global single flight; a per-Db flag if one process ever
// serves several libraries.
static BACKFILL_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Held for the duration of one pass; releases the single-flight flag on drop.
pub struct BackfillGuard(());
impl Drop for BackfillGuard {
    fn drop(&mut self) {
        BACKFILL_IN_PROGRESS.store(false, Ordering::SeqCst);
    }
}

/// Claim the single-flight slot; `None` while another pass holds it.
pub fn try_begin_backfill() -> Option<BackfillGuard> {
    (!BACKFILL_IN_PROGRESS.swap(true, Ordering::SeqCst)).then_some(BackfillGuard(()))
}

/// Fetch CrossRef then OpenAlex records per distinct DOI, paced by
/// `INTER_DOI_DELAY`. Returns records keyed by DOI plus the errored-DOI count.
pub async fn fetch_orcid_records(
    candidates: &[OrcidCandidate],
) -> (HashMap<String, Vec<PaperMetadata>>, i64) {
    let mut dois: Vec<&str> = candidates.iter().map(|c| c.doi.as_str()).collect();
    dois.sort_unstable();
    dois.dedup();

    let mut fetched = HashMap::new();
    let mut errored = 0i64;
    let mut dois = dois.into_iter().peekable();
    while let Some(doi) = dois.next() {
        let (records, doi_errored) = orcid_records_for_doi(doi).await;
        if doi_errored {
            errored += 1;
        }
        fetched.insert(doi.to_string(), records);
        if dois.peek().is_some() {
            tokio::time::sleep(INTER_DOI_DELAY).await;
        }
    }
    (fetched, errored)
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
pub struct OrcidBackfillResponse {
    pub checked: usize,
    pub updated: Vec<OrcidCandidate>,
    /// DOIs where a source request failed (not just "no ORCID found").
    pub errored: i64,
}

/// Case-insensitive (ASCII, matching `AUTHOR_FULL_NAME COLLATE NOCASE`) name
/// lookup against a fetched record's index-aligned `author_orcids`.
fn match_orcid<'a>(meta: &'a PaperMetadata, name: &str) -> Option<&'a str> {
    let orcids = meta.author_orcids.as_ref()?;
    let i = meta
        .authors
        .iter()
        .position(|a| a.eq_ignore_ascii_case(name))?;
    orcids.get(i)?.as_deref()
}

/// Fill the first orcid name-matched per candidate's DOI, in one transaction.
/// No match/records for a DOI is silently skipped. Returns those updated.
pub fn apply_results(
    conn: &mut Connection,
    candidates: &[OrcidCandidate],
    fetched: &HashMap<String, Vec<PaperMetadata>>,
) -> Result<Vec<OrcidCandidate>> {
    crate::storage::db::transaction(conn, |tx| {
        let mut updated = Vec::new();
        for cand in candidates {
            let Some(records) = fetched.get(&cand.doi) else {
                continue;
            };
            let Some(orcid) = records.iter().find_map(|m| match_orcid(m, &cand.full_name)) else {
                continue;
            };
            if fill_orcid_if_null(tx, cand.author_id, orcid)? {
                updated.push(cand.clone());
            }
        }
        Ok(updated)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn meta(authors: &[&str], orcids: Option<Vec<Option<&str>>>) -> PaperMetadata {
        PaperMetadata {
            source_id: "doi:10.1/x".into(),
            version: 1,
            title: "T".into(),
            authors: authors.iter().map(|s| s.to_string()).collect(),
            published: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            updated: None,
            summary: "s".into(),
            category: None,
            categories: None,
            doi: Some("10.1/x".into()),
            journal_ref: None,
            comment: None,
            url: None,
            tags: None,
            source: Some("crossref".into()),
            author_orcids: orcids.map(|v| v.into_iter().map(|o| o.map(String::from)).collect()),
        }
    }

    #[test]
    fn validate_limit_accepts_only_1_to_100() {
        assert!(validate_limit(1).is_ok() && validate_limit(100).is_ok());
        for bad in [0, 101, -3] {
            assert!(matches!(validate_limit(bad), Err(CoreError::Validation(_))));
        }
    }

    #[test]
    fn guard_is_single_flight_and_releases_on_drop() {
        let first = try_begin_backfill().expect("slot free");
        assert!(try_begin_backfill().is_none());
        drop(first);
        assert!(try_begin_backfill().is_some());
    }

    #[tokio::test]
    async fn fetch_with_no_candidates_is_empty_and_touches_no_network() {
        let (fetched, errored) = fetch_orcid_records(&[]).await;
        assert!(fetched.is_empty());
        assert_eq!(errored, 0);
    }

    #[test]
    fn match_orcid_is_case_insensitive_and_index_aligned() {
        let m = meta(
            &["Alice Cole", "Bob Stone"],
            Some(vec![Some("0000-1"), None]),
        );
        assert_eq!(match_orcid(&m, "alice cole"), Some("0000-1"));
        assert_eq!(match_orcid(&m, "Bob Stone"), None);
        assert_eq!(match_orcid(&m, "Nobody"), None);
    }

    #[test]
    fn match_orcid_none_when_record_carries_no_orcids() {
        let m = meta(&["Alice Cole"], None);
        assert_eq!(match_orcid(&m, "Alice Cole"), None);
    }

    #[test]
    fn apply_results_fills_matched_skips_unmatched_and_missing_doi() {
        use crate::storage::queries::author::create_author;
        use crate::storage::{db::open_in_memory, init_db};

        let mut conn = open_in_memory().unwrap();
        init_db(&conn).unwrap();
        let alice = create_author(&conn, "Alice Cole", None, None, None).unwrap();
        let bob = create_author(&conn, "Bob Stone", None, None, None).unwrap();
        let ghost = create_author(&conn, "No Doi", None, None, None).unwrap();

        let candidates = vec![
            OrcidCandidate {
                author_id: alice,
                full_name: "Alice Cole".into(),
                doi: "10.1/x".into(),
            },
            OrcidCandidate {
                author_id: bob,
                full_name: "Bob Stone".into(),
                doi: "10.1/x".into(),
            },
            OrcidCandidate {
                author_id: ghost,
                full_name: "No Doi".into(),
                doi: "10.1/unfetched".into(),
            },
        ];
        let mut fetched = HashMap::new();
        fetched.insert(
            "10.1/x".to_string(),
            vec![meta(
                &["Alice Cole", "Someone Else"],
                Some(vec![Some("0000-1"), Some("0000-2")]),
            )],
        );

        let updated = apply_results(&mut conn, &candidates, &fetched).unwrap();
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].author_id, alice);

        assert_eq!(
            crate::storage::queries::author::get_author(&conn, alice)
                .unwrap()
                .unwrap()
                .orcid
                .as_deref(),
            Some("0000-1")
        );
        assert!(crate::storage::queries::author::get_author(&conn, bob)
            .unwrap()
            .unwrap()
            .orcid
            .is_none());
    }
}
