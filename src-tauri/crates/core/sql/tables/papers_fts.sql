-- FTS5 index over PAPER_META.FULL_TEXT. `paper_id` holds the SOURCE_ID *string*
-- (search hydrates its hits from `latest_papers` by source_id), so there is at
-- most one row per paper, not one per version.
--
-- The index is DERIVED, never hand-maintained: triggers on PAPER_META re-derive
-- it from `paper_index_text` on every write to FULL_TEXT, so a writer that
-- stores text and forgets the index cannot desync search — there is nothing left
-- to forget. The view and those triggers live in views/paper_index_text.sql,
-- applied after the migrations for the reason that file gives.
CREATE VIRTUAL TABLE IF NOT EXISTS papers_fts USING fts5(paper_id, full_text);
