-- Rebuild papers_fts keyed by rowid == SOURCE_FK (see tables/papers_fts.sql).
-- The old shape indexed `paper_id`, so every trigger/refresh delete addressed
-- by it was a full scan of the FTS table. Repopulation inlines the
-- `paper_index_text` derivation against the base tables: at migration time the
-- view (re)created by the later views phase may be absent or the old shape.
BEGIN;
 DROP TABLE papers_fts;
 CREATE VIRTUAL TABLE papers_fts USING fts5(paper_id UNINDEXED, full_text);
 INSERT INTO papers_fts (rowid, paper_id, full_text)
 SELECT p.SOURCE_FK, p.SOURCE_ID, m.FULL_TEXT
 FROM PAPER p
 JOIN PAPER_META m ON m.PAPER_ID = p.PAPER_ID
 JOIN PAPER_ROOTS r ON r.SOURCE_FK = p.SOURCE_FK
 WHERE r.STATUS = 'active'
   AND COALESCE(m.FULL_TEXT, '') != ''
   AND p.VERSION = (
       SELECT MAX(x.VERSION) FROM PAPER x
       JOIN PAPER_META y ON y.PAPER_ID = x.PAPER_ID
       WHERE x.SOURCE_FK = p.SOURCE_FK AND COALESCE(y.FULL_TEXT, '') != ''
   );
 COMMIT;
