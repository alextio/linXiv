//! Merge-import: fold another linXiv DB (e.g. a backup) into the live one.
//!
//! Unlike `restore`, nothing is replaced — rows missing from the live DB are
//! inserted, matched by natural key: PAPER_ROOTS by SOURCE_ID, PAPER by
//! (SOURCE_ID, VERSION), TAG by label (nocase), AUTHOR by full name (nocase),
//! PROJECT by name (nocase, reading list matched by flag), NOTE/ANNOTATION by
//! UUID. Surrogate ids are remapped via temp map tables; FTS stays consistent
//! through the existing triggers. Transient tables (RSS, search history,
//! version checks, quarantine) are not imported.

use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// Rows inserted per entity by one `import_merge` run (`*_links` sums the
/// respective link tables); `pdf_refs_cleared` counts imported papers whose
/// PDF file was absent on this machine and had their PDF reference reset.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
pub struct ImportReport {
    pub roots: u64,
    pub papers: u64,
    pub paper_meta: u64,
    pub authors: u64,
    pub tags: u64,
    pub projects: u64,
    pub paper_links: u64,
    pub project_links: u64,
    pub notes: u64,
    pub annotations: u64,
    pub pdf_refs_cleared: u64,
}

/// Merge `src` (a linXiv DB file) into the live DB behind `conn`, insert-only.
/// The whole merge is one IMMEDIATE transaction — on any error nothing lands.
pub fn import_merge(conn: &mut Connection, src: &Path) -> Result<ImportReport> {
    super::validate_backup_source(src)?;
    let src_str = src
        .to_str()
        .ok_or_else(|| CoreError::Validation("source path is not valid UTF-8".into()))?;
    conn.execute("ATTACH DATABASE ?1 AS src", [src_str])?;
    let result = merge_attached(conn);
    let _ = conn.execute_batch("DETACH DATABASE src");
    result
}

fn merge_attached(conn: &mut Connection) -> Result<ImportReport> {
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut r = ImportReport {
        roots: 0,
        papers: 0,
        paper_meta: 0,
        authors: 0,
        tags: 0,
        projects: 0,
        paper_links: 0,
        project_links: 0,
        notes: 0,
        annotations: 0,
        pdf_refs_cleared: 0,
    };

    // Lineage roots, then the id map every other table remaps through.
    tx.execute_batch(
        "INSERT INTO PAPER_ROOTS (SOURCE_ID, STATUS, DELETED_AT, CREATED_AT, UPDATED_AT)
         SELECT s.SOURCE_ID, s.STATUS, s.DELETED_AT, s.CREATED_AT, s.UPDATED_AT
         FROM src.PAPER_ROOTS s
         WHERE NOT EXISTS (SELECT 1 FROM PAPER_ROOTS d WHERE d.SOURCE_ID = s.SOURCE_ID);",
    )?;
    r.roots = tx.changes();
    tx.execute_batch(
        "CREATE TEMP TABLE root_map AS
         SELECT s.SOURCE_FK AS src_fk, d.SOURCE_FK AS dst_fk
         FROM src.PAPER_ROOTS s JOIN main.PAPER_ROOTS d ON d.SOURCE_ID = s.SOURCE_ID;",
    )?;

    tx.execute_batch(
        "INSERT INTO PAPER (SOURCE_ID, VERSION, TITLE, CATEGORY, HAS_PDF,
                            CREATED_AT, UPDATED_AT, SOURCE_FK)
         SELECT s.SOURCE_ID, s.VERSION, s.TITLE, s.CATEGORY, s.HAS_PDF,
                s.CREATED_AT, s.UPDATED_AT, m.dst_fk
         FROM src.PAPER s JOIN temp.root_map m ON m.src_fk = s.SOURCE_FK
         WHERE NOT EXISTS (SELECT 1 FROM PAPER d
                           WHERE d.SOURCE_ID = s.SOURCE_ID AND d.VERSION = s.VERSION);",
    )?;
    r.papers = tx.changes();
    tx.execute_batch(
        "CREATE TEMP TABLE paper_map AS
         SELECT s.PAPER_ID AS src_id, d.PAPER_ID AS dst_id
         FROM src.PAPER s
         JOIN main.PAPER d ON d.SOURCE_ID = s.SOURCE_ID AND d.VERSION = s.VERSION;",
    )?;

    tx.execute_batch(
        "INSERT INTO PAPER_META (PAPER_ID, URL, PUBLISHED, UPDATED, CATEGORIES, DOI,
                                 JOURNAL_REF, COMMENT, SUMMARY, PROVIDER, PDF_PATH,
                                 FULL_TEXT, DOWNLOADED_SOURCE, AUTHORS, TAGS,
                                 CREATED_AT, UPDATED_AT)
         SELECT m.dst_id, s.URL, s.PUBLISHED, s.UPDATED, s.CATEGORIES, s.DOI,
                s.JOURNAL_REF, s.COMMENT, s.SUMMARY, s.PROVIDER, s.PDF_PATH,
                s.FULL_TEXT, s.DOWNLOADED_SOURCE, s.AUTHORS, s.TAGS,
                s.CREATED_AT, s.UPDATED_AT
         FROM src.PAPER_META s JOIN temp.paper_map m ON m.src_id = s.PAPER_ID
         WHERE NOT EXISTS (SELECT 1 FROM PAPER_META d WHERE d.PAPER_ID = m.dst_id);",
    )?;
    r.paper_meta = tx.changes();

    // ponytail: authors matched by full name (nocase); NULL-named src authors
    // and their links are skipped — dedupe by ORCID if that ever matters.
    tx.execute_batch(
        "INSERT INTO AUTHOR (AUTHOR_FULL_NAME, AUTHOR_FIRST, AUTHOR_LAST, AUTHOR_ORCID,
                             CREATED_AT, UPDATED_AT)
         SELECT s.AUTHOR_FULL_NAME, s.AUTHOR_FIRST, s.AUTHOR_LAST, s.AUTHOR_ORCID,
                s.CREATED_AT, s.UPDATED_AT
         FROM src.AUTHOR s
         WHERE s.AUTHOR_FULL_NAME IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM AUTHOR d
                           WHERE d.AUTHOR_FULL_NAME = s.AUTHOR_FULL_NAME COLLATE NOCASE);",
    )?;
    r.authors = tx.changes();
    tx.execute_batch(
        "CREATE TEMP TABLE author_map AS
         SELECT s.AUTHOR_FK AS src_id,
                (SELECT MIN(d.AUTHOR_FK) FROM main.AUTHOR d
                  WHERE d.AUTHOR_FULL_NAME = s.AUTHOR_FULL_NAME COLLATE NOCASE) AS dst_id
         FROM src.AUTHOR s WHERE s.AUTHOR_FULL_NAME IS NOT NULL;",
    )?;

    tx.execute_batch(
        "INSERT INTO TAG (TAG, CREATED_AT, UPDATED_AT)
         SELECT s.TAG, s.CREATED_AT, s.UPDATED_AT
         FROM src.TAG s
         WHERE s.TAG IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM TAG d WHERE d.TAG = s.TAG COLLATE NOCASE);",
    )?;
    r.tags = tx.changes();
    tx.execute_batch(
        "CREATE TEMP TABLE tag_map AS
         SELECT s.TAG_FK AS src_id, d.TAG_FK AS dst_id
         FROM src.TAG s JOIN main.TAG d ON d.TAG = s.TAG COLLATE NOCASE;",
    )?;

    tx.execute_batch(
        "INSERT OR IGNORE INTO PAPER_TO_AUTHOR (PAPER_ID, AUTHOR_FK, AUTHOR_INDEX,
                                                CREATED_AT, UPDATED_AT)
         SELECT pm.dst_id, am.dst_id, s.AUTHOR_INDEX, s.CREATED_AT, s.UPDATED_AT
         FROM src.PAPER_TO_AUTHOR s
         JOIN temp.paper_map pm ON pm.src_id = s.PAPER_ID
         JOIN temp.author_map am ON am.src_id = s.AUTHOR_FK;",
    )?;
    r.paper_links += tx.changes();
    // PAPER_TO_TAG has no unique index — guard with NOT EXISTS.
    tx.execute_batch(
        "INSERT INTO PAPER_TO_TAG (PAPER_ID, SOURCE_ID, VERSION, TAG_FK,
                                   CREATED_AT, UPDATED_AT)
         SELECT pm.dst_id, s.SOURCE_ID, s.VERSION, tm.dst_id, s.CREATED_AT, s.UPDATED_AT
         FROM src.PAPER_TO_TAG s
         JOIN temp.paper_map pm ON pm.src_id = s.PAPER_ID
         JOIN temp.tag_map tm ON tm.src_id = s.TAG_FK
         WHERE NOT EXISTS (SELECT 1 FROM PAPER_TO_TAG d
                           WHERE d.PAPER_ID = pm.dst_id AND d.TAG_FK = tm.dst_id);",
    )?;
    r.paper_links += tx.changes();

    // Projects: at most one live reading list — the source's folds into it.
    // Deleted source projects stay behind; their notes import unscoped.
    tx.execute_batch(
        "INSERT INTO PROJECT (NAME, DESCRIPTION, COLOR, STATUS, CREATED_AT, UPDATED_AT,
                              ARCHIVED_AT, IS_READING_LIST, SHARE_ID)
         SELECT s.NAME, s.DESCRIPTION, s.COLOR, s.STATUS, s.CREATED_AT, s.UPDATED_AT,
                s.ARCHIVED_AT, s.IS_READING_LIST,
                CASE WHEN EXISTS (SELECT 1 FROM PROJECT d2 WHERE d2.SHARE_ID = s.SHARE_ID)
                     THEN NULL ELSE s.SHARE_ID END
         FROM src.PROJECT s
         WHERE s.STATUS != 'deleted'
           AND NOT (s.IS_READING_LIST = 1 AND EXISTS
                    (SELECT 1 FROM PROJECT d
                      WHERE d.IS_READING_LIST = 1 AND d.STATUS != 'deleted'))
           AND NOT EXISTS (SELECT 1 FROM PROJECT d
                           WHERE d.NAME = s.NAME COLLATE NOCASE
                             AND d.STATUS != 'deleted'
                             AND d.IS_READING_LIST = s.IS_READING_LIST);",
    )?;
    r.projects = tx.changes();
    tx.execute_batch(
        "CREATE TEMP TABLE project_map AS
         SELECT s.PROJECT_FK AS src_id,
                COALESCE(
                  CASE WHEN s.IS_READING_LIST = 1 THEN
                    (SELECT MIN(d.PROJECT_FK) FROM main.PROJECT d
                      WHERE d.IS_READING_LIST = 1 AND d.STATUS != 'deleted') END,
                  (SELECT MIN(d.PROJECT_FK) FROM main.PROJECT d
                    WHERE d.NAME = s.NAME COLLATE NOCASE
                      AND d.STATUS != 'deleted'
                      AND d.IS_READING_LIST = s.IS_READING_LIST)
                ) AS dst_id
         FROM src.PROJECT s WHERE s.STATUS != 'deleted';
         DELETE FROM temp.project_map WHERE dst_id IS NULL;",
    )?;

    tx.execute_batch(
        "INSERT OR IGNORE INTO PROJECT_TO_PAPER (PROJECT_FK, SOURCE_FK,
                                                 CREATED_AT, UPDATED_AT)
         SELECT pj.dst_id, rm.dst_fk, s.CREATED_AT, s.UPDATED_AT
         FROM src.PROJECT_TO_PAPER s
         JOIN temp.project_map pj ON pj.src_id = s.PROJECT_FK
         JOIN temp.root_map rm ON rm.src_fk = s.SOURCE_FK;",
    )?;
    r.project_links += tx.changes();
    tx.execute_batch(
        "INSERT OR IGNORE INTO PROJECT_TO_TAG (PROJECT_FK, TAG_FK, CREATED_AT, UPDATED_AT)
         SELECT pj.dst_id, tm.dst_id, s.CREATED_AT, s.UPDATED_AT
         FROM src.PROJECT_TO_TAG s
         JOIN temp.project_map pj ON pj.src_id = s.PROJECT_FK
         JOIN temp.tag_map tm ON tm.src_id = s.TAG_FK;",
    )?;
    r.project_links += tx.changes();
    tx.execute_batch(
        "INSERT OR IGNORE INTO PAPER_TO_READING (PROJECT_FK, SOURCE_FK, STATUS, UPDATED_AT)
         SELECT pj.dst_id, rm.dst_fk, s.STATUS, s.UPDATED_AT
         FROM src.PAPER_TO_READING s
         JOIN temp.project_map pj ON pj.src_id = s.PROJECT_FK
         JOIN temp.root_map rm ON rm.src_fk = s.SOURCE_FK
         WHERE EXISTS (SELECT 1 FROM PROJECT_TO_PAPER d
                       WHERE d.PROJECT_FK = pj.dst_id AND d.SOURCE_FK = rm.dst_fk);",
    )?;
    r.project_links += tx.changes();

    // Notes and annotations dedupe on their UUIDs; the FTS triggers index them.
    tx.execute_batch(
        "INSERT INTO NOTE (SOURCE_FK, PAPER_ID_FK, PROJECT_FK, TITLE, NOTE, NOTE_UUID,
                           CREATED_AT, UPDATED_AT)
         SELECT rm.dst_fk, pm.dst_id, pj.dst_id, s.TITLE, s.NOTE, s.NOTE_UUID,
                s.CREATED_AT, s.UPDATED_AT
         FROM src.NOTE s
         JOIN temp.root_map rm ON rm.src_fk = s.SOURCE_FK
         LEFT JOIN temp.paper_map pm ON pm.src_id = s.PAPER_ID_FK
         LEFT JOIN temp.project_map pj ON pj.src_id = s.PROJECT_FK
         WHERE s.NOTE_UUID IS NULL
            OR NOT EXISTS (SELECT 1 FROM NOTE d WHERE d.NOTE_UUID = s.NOTE_UUID);",
    )?;
    r.notes = tx.changes();
    tx.execute_batch(
        "INSERT INTO ANNOTATION (SOURCE_FK, PROJECT_FK, ANCHOR, COMMENT, ANNOTATION_UUID,
                                 CREATED_AT, UPDATED_AT)
         SELECT rm.dst_fk, pj.dst_id, s.ANCHOR, s.COMMENT, s.ANNOTATION_UUID,
                s.CREATED_AT, s.UPDATED_AT
         FROM src.ANNOTATION s
         JOIN temp.root_map rm ON rm.src_fk = s.SOURCE_FK
         LEFT JOIN temp.project_map pj ON pj.src_id = s.PROJECT_FK
         WHERE s.ANNOTATION_UUID IS NULL
            OR NOT EXISTS (SELECT 1 FROM ANNOTATION d
                           WHERE d.ANNOTATION_UUID = s.ANNOTATION_UUID);",
    )?;
    r.annotations = tx.changes();

    // A backup carries PDF paths, not PDF files — drop references whose file
    // is not on this machine so papers read as re-downloadable, not broken.
    let stale: Vec<i64> = {
        let mut stmt = tx.prepare(
            "SELECT p.PAPER_ID, m.PDF_PATH
             FROM PAPER p
             JOIN temp.paper_map map ON map.dst_id = p.PAPER_ID
             LEFT JOIN PAPER_META m ON m.PAPER_ID = p.PAPER_ID
             WHERE p.HAS_PDF = 1 OR m.PDF_PATH IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        rows.filter_map(|row| match row {
            Ok((id, path)) => match path {
                Some(p) if Path::new(&p).exists() => None,
                _ => Some(Ok(id)),
            },
            Err(e) => Some(Err(e)),
        })
        .collect::<rusqlite::Result<_>>()?
    };
    for id in &stale {
        tx.execute(
            "UPDATE PAPER_META SET PDF_PATH = NULL, DOWNLOADED_SOURCE = 0 WHERE PAPER_ID = ?1",
            [id],
        )?;
        tx.execute("UPDATE PAPER SET HAS_PDF = 0 WHERE PAPER_ID = ?1", [id])?;
    }
    r.pdf_refs_cleared = stale.len() as u64;

    tx.execute_batch(
        "DROP TABLE temp.root_map; DROP TABLE temp.paper_map;
         DROP TABLE temp.author_map; DROP TABLE temp.tag_map;
         DROP TABLE temp.project_map;",
    )?;
    tx.commit()?;
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_db(path: &Path) -> Connection {
        let conn = crate::storage::open(path).unwrap();
        crate::storage::init_db(&conn).unwrap();
        conn
    }

    #[test]
    fn merge_imports_missing_rows_and_dedupes_on_natural_keys() {
        let dir = std::env::temp_dir().join(format!("linxiv-import-merge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Source: two papers (one shared with dest), a tag, a project, a note.
        let src_path = dir.join("src.db");
        let src = seeded_db(&src_path);
        src.execute_batch(
            "INSERT INTO PAPER_ROOTS (SOURCE_FK, SOURCE_ID) VALUES (10, 'arxiv:shared'), (11, 'arxiv:only-src');
             INSERT INTO PAPER (PAPER_ID, SOURCE_ID, VERSION, TITLE, HAS_PDF, SOURCE_FK) VALUES
               (100, 'arxiv:shared', 1, 'Shared', 0, 10),
               (101, 'arxiv:only-src', 1, 'Only In Source', 1, 11);
             INSERT INTO PAPER_META (PAPER_ID, SUMMARY, PDF_PATH) VALUES (101, 'sum', '/nonexistent/x.pdf');
             INSERT INTO TAG (TAG_FK, TAG) VALUES (5, 'ml');
             INSERT INTO PAPER_TO_TAG (PAPER_ID, TAG_FK) VALUES (101, 5);
             INSERT INTO AUTHOR (AUTHOR_FK, AUTHOR_FULL_NAME) VALUES (7, 'Ada Lovelace');
             INSERT INTO PAPER_TO_AUTHOR (PAPER_ID, AUTHOR_FK) VALUES (101, 7);
             INSERT INTO PROJECT (PROJECT_FK, NAME) VALUES (3, 'Thesis');
             INSERT INTO PROJECT_TO_PAPER (PROJECT_FK, SOURCE_FK) VALUES (3, 11);
             INSERT INTO NOTE (SOURCE_FK, PAPER_ID_FK, PROJECT_FK, TITLE, NOTE, NOTE_UUID)
               VALUES (11, 101, 3, 'n', X'00', 'uuid-1');",
        )
        .unwrap();
        drop(src);

        // Dest: the shared paper (other ids), a same-name tag differing in case.
        let mut dest = seeded_db(&dir.join("dest.db"));
        dest.execute_batch(
            "INSERT INTO PAPER_ROOTS (SOURCE_FK, SOURCE_ID) VALUES (1, 'arxiv:shared');
             INSERT INTO PAPER (PAPER_ID, SOURCE_ID, VERSION, TITLE, HAS_PDF, SOURCE_FK)
               VALUES (1, 'arxiv:shared', 1, 'Shared', 0, 1);
             INSERT INTO TAG (TAG) VALUES ('ML');",
        )
        .unwrap();

        let report = import_merge(&mut dest, &src_path).unwrap();
        assert_eq!(report.roots, 1, "only the unshared root imports");
        assert_eq!(report.papers, 1);
        assert_eq!(report.tags, 0, "'ml' matches existing 'ML' nocase");
        assert_eq!(report.authors, 1);
        assert_eq!(report.projects, 1);
        assert_eq!(report.notes, 1);
        assert_eq!(
            report.pdf_refs_cleared, 1,
            "missing PDF file → reference cleared"
        );

        // Remap landed: note points at the imported paper's new dest ids.
        let (src_fk, paper_fk, has_pdf): (i64, i64, bool) = dest
            .query_row(
                "SELECT n.SOURCE_FK, n.PAPER_ID_FK, p.HAS_PDF
                 FROM NOTE n JOIN PAPER p ON p.PAPER_ID = n.PAPER_ID_FK
                 WHERE n.NOTE_UUID = 'uuid-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        let dest_root: i64 = dest
            .query_row(
                "SELECT SOURCE_FK FROM PAPER_ROOTS WHERE SOURCE_ID = 'arxiv:only-src'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(src_fk, dest_root);
        assert!(paper_fk > 0);
        assert!(!has_pdf, "imported paper's dangling PDF flag cleared");

        // Idempotent: a second run imports nothing.
        let again = import_merge(&mut dest, &src_path).unwrap();
        assert_eq!(
            (again.roots, again.papers, again.notes, again.projects),
            (0, 0, 0, 0)
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
