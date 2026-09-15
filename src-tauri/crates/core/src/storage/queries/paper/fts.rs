//! Full-text storage and FTS index maintenance, plus the TeX-source backfill
//! candidate queries.

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::error::Result;
use crate::models::{ARXIV_ID_PREFIX, ARXIV_PDF_MARKER};
use crate::storage::db::transaction;

// papers_fts is keyed by rowid == PAPER_ID, one row per version with text;
// source_id/version are UNINDEXED (display/join only).
// init_db always creates papers_fts, so a DELETE/INSERT against it cannot miss.

/// Rows the backfill works on: latest-version active papers with no TeX source
/// that `service::paper::source_fetch_url` would accept (an `arxiv:` id with a
/// `/pdf/` link) — both patterns built from the constants that fn matches on.
/// GLOB, not LIKE: LIKE is ASCII-case-insensitive.
fn backfill_where() -> String {
    format!(
        "FROM latest_papers WHERE COALESCE(downloaded_source, 0) = 0 \
         AND source_id GLOB '{ARXIV_ID_PREFIX}*' AND url GLOB '*{ARXIV_PDF_MARKER}*'"
    )
}

/// SOURCE_IDs of those rows, oldest-published first. Ids ONLY: `list_papers`
/// would build a `PaperDetails` per row, materialising the whole library just to
/// filter it out.
pub fn full_text_backfill_candidates(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT source_id {} ORDER BY published ASC, source_id ASC",
        backfill_where()
    ))?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// How many papers `full_text_backfill_candidates` would return, without
/// materialising an id per row — the backlog readout polls this.
pub fn full_text_backfill_count(conn: &Connection) -> Result<i64> {
    Ok(
        conn.query_row(&format!("SELECT COUNT(*) {}", backfill_where()), [], |r| {
            r.get(0)
        })?,
    )
}

/// Re-derive a lineage's FTS rows from `paper_index_text` (dropped when the
/// view yields nothing) — the same derivation the PAPER_META triggers run, so
/// the paths can't disagree. Only for writes no trigger sees: a SOURCE_ID
/// rename, an undelete, a merge; FULL_TEXT writers are covered by trigger
/// already. The DELETE routes through PAPER (served by
/// `idx_paper_source_fk_version`) so renamed/transplanted rows are still found
/// by rowid whatever source_id string they carry.
pub(super) fn refresh_fts(tx: &Transaction, source_id: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM papers_fts \
         WHERE rowid IN (SELECT PAPER_ID FROM PAPER \
                         WHERE SOURCE_FK = (SELECT SOURCE_FK FROM PAPER_ROOTS WHERE SOURCE_ID = ?))",
        [source_id],
    )?;
    tx.execute(
        "INSERT INTO papers_fts(rowid, source_id, version, full_text) \
         SELECT paper_id, source_id, version, full_text FROM paper_index_text WHERE source_id = ?",
        [source_id],
    )?;
    Ok(())
}

/// Store extracted TeX and mark DOWNLOADED_SOURCE; no-op if the version doesn't
/// exist. FTS follows by trigger. Empty text marks the version fetched and
/// unindexed; other versions' rows are independent and keep answering.
pub fn set_full_text(
    conn: &mut Connection,
    source_id: &str,
    version: i64,
    full_text: Option<&str>,
) -> Result<()> {
    transaction(conn, |tx| {
        let pid: Option<i64> = tx
            .query_row(
                "SELECT PAPER_ID FROM PAPER WHERE SOURCE_ID = ? AND VERSION = ?",
                params![source_id, version],
                |r| r.get(0),
            )
            .optional()?;
        let Some(pid) = pid else { return Ok(()) };
        tx.execute(
            "UPDATE PAPER_META SET FULL_TEXT = ?, DOWNLOADED_SOURCE = 1 WHERE PAPER_ID = ?",
            params![full_text, pid],
        )?;
        Ok(())
    })
}

/// Whether this exact active version already stores a non-empty TeX body — the
/// commit-time guard that keeps an empty re-fetch from erasing an indexed one.
/// One boolean column: the body can run to megabytes and no caller wants it.
pub fn has_full_text(conn: &Connection, source_id: &str, version: i64) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT full_text IS NOT NULL AND full_text != '' FROM papers \
             WHERE source_id = ? AND version = ?",
            params![source_id, version],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(false))
}

/// One active version's stored TeX body, `None` when the version isn't stored
/// (or the root is trashed — inherited from `papers`). The full-text-diff
/// endpoint is the caller; every display read stays `PAPER_COLUMNS_NO_TEXT`.
pub fn get_full_text(conn: &Connection, source_id: &str, version: i64) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT full_text FROM papers WHERE source_id = ? AND version = ?",
            params![source_id, version],
            |r| r.get(0),
        )
        .optional()?
        .flatten())
}

#[cfg(test)]
mod tests {
    use super::super::testutil::{count, meta};
    use super::super::*;
    use crate::storage::{db::open_in_memory, init_db};
    use rusqlite::{params, Connection};

    #[test]
    fn set_full_text_updates_meta_and_fts() {
        let mut conn = open_in_memory().unwrap();
        init_db(&conn).unwrap();
        save_paper_metadata(&mut conn, &meta("arxiv:ft", 1), None).unwrap();

        assert!(!has_full_text(&conn, "arxiv:ft", 1).unwrap());
        set_full_text(&mut conn, "arxiv:ft", 1, Some("the full tex body")).unwrap();
        let p = get_paper(&conn, "arxiv:ft", Some(1)).unwrap().unwrap();
        // `get_paper` blanks the body; the stored column answers through
        // `has_full_text`.
        assert_eq!(p.full_text, None);
        assert!(has_full_text(&conn, "arxiv:ft", 1).unwrap());
        assert!(p.downloaded_source);
        // FTS searchable under the SOURCE_ID string.
        let hit: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM papers_fts WHERE papers_fts MATCH 'tex' AND source_id = ?",
                ["arxiv:ft"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hit, 1);

        // Refresh is DELETE+INSERT, so no duplicate rows accumulate.
        set_full_text(&mut conn, "arxiv:ft", 1, Some("rewritten")).unwrap();
        assert_eq!(
            count(
                &conn,
                "SELECT COUNT(*) FROM papers_fts WHERE source_id = ?",
                "arxiv:ft"
            ),
            1
        );
    }

    /// THE INVARIANT: papers_fts is derived from FULL_TEXT, so a writer that
    /// stores text WITHOUT going through `set_full_text` still cannot desync
    /// search. Every write below is a raw statement of exactly the shape the
    /// index used to depend on nobody writing; drop the INSERT/UPDATE trigger or
    /// the STATUS gate from `paper_index_text.sql` and this goes red.
    #[test]
    fn raw_full_text_writes_cannot_desync_the_index() {
        let mut conn = open_in_memory().unwrap();
        init_db(&conn).unwrap();
        save_paper_metadata(&mut conn, &meta("arxiv:raw", 1), None).unwrap();
        let matches = |conn: &Connection, body: &str| {
            count(
                conn,
                "SELECT COUNT(*) FROM papers_fts WHERE papers_fts MATCH ? AND source_id = 'arxiv:raw'",
                body,
            )
        };
        let set_text = |conn: &Connection, version: i64, text: Option<&str>| {
            conn.execute(
                "UPDATE PAPER_META SET FULL_TEXT = ? WHERE PAPER_ID IN \
                 (SELECT PAPER_ID FROM PAPER WHERE SOURCE_ID = 'arxiv:raw' AND VERSION = ?)",
                params![text, version],
            )
            .unwrap();
        };

        // UPDATE: the body lands in the index without anyone asking it to.
        set_text(&conn, 1, Some("smuggled tex"));
        assert_eq!(matches(&conn, "smuggled"), 1);

        // INSERT: a v2 written with text gets its own row — one per version,
        // the older body stays searchable beside it.
        save_paper_metadata(&mut conn, &meta("arxiv:raw", 2), None).unwrap();
        let v2: i64 = conn
            .query_row(
                "SELECT PAPER_ID FROM PAPER WHERE SOURCE_ID = 'arxiv:raw' AND VERSION = 2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute("DELETE FROM PAPER_META WHERE PAPER_ID = ?", [v2])
            .unwrap();
        conn.execute(
            "INSERT INTO PAPER_META (PAPER_ID, PUBLISHED, FULL_TEXT) \
             VALUES (?, '2024-01-01', 'second tex')",
            [v2],
        )
        .unwrap();
        assert_eq!(matches(&conn, "second"), 1);
        assert_eq!(matches(&conn, "smuggled"), 1);
        assert_eq!(
            count(
                &conn,
                "SELECT COUNT(*) FROM papers_fts WHERE source_id = ?",
                "arxiv:raw"
            ),
            2
        );

        // Clearing the newest body only drops its own row; v1 keeps answering.
        set_text(&conn, 2, None);
        assert_eq!(matches(&conn, "second"), 0);
        assert_eq!(matches(&conn, "smuggled"), 1);

        // ...and clearing every body takes the paper out of search entirely.
        set_text(&conn, 1, Some(""));
        assert_eq!(
            count(
                &conn,
                "SELECT COUNT(*) FROM papers_fts WHERE source_id = ?",
                "arxiv:raw"
            ),
            0
        );

        // A soft-deleted paper keeps its FULL_TEXT, so re-deriving must NOT put it
        // back into search — the STATUS gate is inherited from `papers`.
        soft_delete_paper(&mut conn, "arxiv:raw").unwrap();
        set_text(&conn, 1, Some("resurrected tex"));
        assert_eq!(matches(&conn, "resurrected"), 0);
    }

    /// Dropping a version's meta row must drop exactly its own FTS row. Delete
    /// `papers_fts_meta_ad` and this goes red: search keeps answering with a body
    /// whose row no longer exists.
    #[test]
    fn deleting_a_meta_row_drops_only_its_own_fts_row() {
        let mut conn = open_in_memory().unwrap();
        init_db(&conn).unwrap();
        let matches = |conn: &Connection, body: &str| {
            count(
                conn,
                "SELECT COUNT(*) FROM papers_fts WHERE papers_fts MATCH ? AND source_id = 'arxiv:del'",
                body,
            )
        };
        let set_text = |conn: &Connection, version: i64, text: &str| {
            conn.execute(
                "UPDATE PAPER_META SET FULL_TEXT = ? WHERE PAPER_ID IN \
                 (SELECT PAPER_ID FROM PAPER WHERE SOURCE_ID = 'arxiv:del' AND VERSION = ?)",
                params![text, version],
            )
            .unwrap();
        };

        save_paper_metadata(&mut conn, &meta("arxiv:del", 1), None).unwrap();
        set_text(&conn, 1, "older body");
        save_paper_metadata(&mut conn, &meta("arxiv:del", 2), None).unwrap();
        set_text(&conn, 2, "newer body");
        assert_eq!(matches(&conn, "newer"), 1);
        assert_eq!(matches(&conn, "older"), 1, "every version's body indexed");

        conn.execute(
            "DELETE FROM PAPER_META WHERE PAPER_ID IN \
             (SELECT PAPER_ID FROM PAPER WHERE SOURCE_ID = 'arxiv:del' AND VERSION = 2)",
            [],
        )
        .unwrap();

        assert_eq!(matches(&conn, "older"), 1, "sibling version's row dropped");
        assert_eq!(matches(&conn, "newer"), 0, "deleted body still searchable");
    }

    /// Re-adding a trashed paper whose version is already stored: `INSERT OR
    /// IGNORE` no-ops, so `write_paper_version_in_tx` returns before any
    /// PAPER_META write and no trigger fires. The un-delete in
    /// `ensure_paper_root_row` has to re-derive, or the paper comes back active,
    /// with a body, and permanently absent from search.
    #[test]
    fn readding_a_trashed_paper_returns_it_to_search() {
        let mut conn = open_in_memory().unwrap();
        init_db(&conn).unwrap();
        let indexed = |conn: &Connection| {
            count(
                conn,
                "SELECT COUNT(*) FROM papers_fts WHERE source_id = ?",
                "arxiv:trash",
            )
        };

        save_paper_metadata(&mut conn, &meta("arxiv:trash", 1), None).unwrap();
        conn.execute(
            "UPDATE PAPER_META SET FULL_TEXT = 'kept body' WHERE PAPER_ID IN \
             (SELECT PAPER_ID FROM PAPER WHERE SOURCE_ID = 'arxiv:trash')",
            [],
        )
        .unwrap();
        assert_eq!(indexed(&conn), 1);

        soft_delete_paper(&mut conn, "arxiv:trash").unwrap();
        assert_eq!(indexed(&conn), 0, "trashed paper must leave the index");

        // Re-fetching the SAME version — the no-op path, not a new version.
        save_paper_metadata(&mut conn, &meta("arxiv:trash", 1), None).unwrap();
        assert_eq!(
            indexed(&conn),
            1,
            "re-added paper is active with a stored body but absent from search"
        );
    }
}
