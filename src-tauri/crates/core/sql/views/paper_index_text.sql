-- papers_fts is derived from PAPER_META.FULL_TEXT. This file is the derivation:
-- one view saying what belongs in the index, and the triggers that keep the
-- index equal to it.
--
-- `paper_index_text` is the ONE definition of "the bodies a paper is searchable
-- by": every version with any text, keyed by PAPER_ID (papers_fts's rowid) and
-- gated on the root still being active — inherited from `papers`, so a
-- soft-deleted paper yields no rows and cannot be indexed. Rust's `refresh_fts`
-- runs the same two statements against the same view, so the automatic path and
-- the hand-called one cannot disagree.
--
-- Applied in the VIEWS phase, after the migrations, and the triggers ship with
-- the view rather than with the table: SQLite compiles a trigger body when it
-- prepares the DML that fires it, so a trigger that reads this view must not
-- exist during a phase where the view doesn't (a migration that touches
-- PAPER_META would fail to prepare at all).
DROP VIEW IF EXISTS paper_index_text;

CREATE VIEW paper_index_text AS
SELECT
    v.paper_id  AS paper_id,
    v.source_id AS source_id,
    v.version   AS version,
    v.full_text AS full_text
FROM papers v
WHERE COALESCE(v.full_text, '') != '';

-- DELETE then INSERT, not UPDATE: the INSERT selects from the view, so it
-- writes nothing when the version no longer belongs in the index (text cleared,
-- or the root soft-deleted while its FULL_TEXT is still stored). Versions are
-- independent rows, so each trigger touches exactly its own PAPER_ID.
--
-- DROP-then-CREATE, like the view above, NOT `CREATE ... IF NOT EXISTS`: these
-- ship in the views phase, which runs on every open, so dropping first is what
-- makes an edited trigger body reach an install that already has the old one.
-- `IF NOT EXISTS` would silently pin every existing DB to the body it first saw.
DROP TRIGGER IF EXISTS papers_fts_meta_ai;
CREATE TRIGGER papers_fts_meta_ai AFTER INSERT ON PAPER_META
    WHEN COALESCE(new.FULL_TEXT, '') != ''
BEGIN
    DELETE FROM papers_fts WHERE rowid = new.PAPER_ID;
    INSERT INTO papers_fts (rowid, source_id, version, full_text)
    SELECT paper_id, source_id, version, full_text FROM paper_index_text
     WHERE paper_id = new.PAPER_ID;
END;

DROP TRIGGER IF EXISTS papers_fts_meta_au;
CREATE TRIGGER papers_fts_meta_au AFTER UPDATE OF FULL_TEXT ON PAPER_META
    WHEN old.FULL_TEXT IS NOT new.FULL_TEXT
BEGIN
    DELETE FROM papers_fts WHERE rowid = new.PAPER_ID;
    INSERT INTO papers_fts (rowid, source_id, version, full_text)
    SELECT paper_id, source_id, version, full_text FROM paper_index_text
     WHERE paper_id = new.PAPER_ID;
END;

-- Deleting a version's meta row only removes its own FTS row — other versions'
-- rows are independent, so there is nothing to re-derive. Guarded on the old
-- row having had text: deleting an empty version was never indexed.
DROP TRIGGER IF EXISTS papers_fts_meta_ad;
CREATE TRIGGER papers_fts_meta_ad AFTER DELETE ON PAPER_META
    WHEN COALESCE(old.FULL_TEXT, '') != ''
BEGIN
    DELETE FROM papers_fts WHERE rowid = old.PAPER_ID;
END;
