-- Rebuild papers_fts keyed by rowid == PAPER_ID, one row per version with text
-- (see tables/papers_fts.sql). Replaces both legacy shapes in one step: the
-- v0.2.0 shape (INDEXED paper_id, so every keyed delete was a full FTS scan)
-- and the short-lived dev-only rowid == SOURCE_FK shape (one row per lineage,
-- newest body wins — older versions' text was unsearchable). Repopulation
-- inlines the `paper_index_text` derivation against the base tables: at
-- migration time the view (re)created by the later views phase may be absent
-- or an old shape.
BEGIN;
 DROP TABLE papers_fts;
 CREATE VIRTUAL TABLE papers_fts USING fts5(source_id UNINDEXED, version UNINDEXED, full_text);
 INSERT INTO papers_fts (rowid, source_id, version, full_text)
 SELECT p.PAPER_ID, p.SOURCE_ID, p.VERSION, m.FULL_TEXT
 FROM PAPER p
 JOIN PAPER_META m ON m.PAPER_ID = p.PAPER_ID
 JOIN PAPER_ROOTS r ON r.SOURCE_FK = p.SOURCE_FK
 WHERE r.STATUS = 'active'
   AND COALESCE(m.FULL_TEXT, '') != '';
 COMMIT;
