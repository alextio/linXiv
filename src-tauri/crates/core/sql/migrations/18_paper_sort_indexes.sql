-- Library sort orders (`PaperSort`): each ORDER BY drives its scan off one of
-- these instead of sorting the whole library in a temp b-tree.
CREATE INDEX IF NOT EXISTS idx_paper_meta_published ON PAPER_META (PUBLISHED);
-- The Added sort's SOURCE_FK scan rides migration 20's composite
-- idx_paper_source_fk_version (left prefix); no single-column index here.
CREATE INDEX IF NOT EXISTS idx_paper_title_nocase ON PAPER (TITLE COLLATE NOCASE);
-- Oldest-first leads with the undated-sinking term, so it needs the expression
-- itself indexed; column order and direction must match `PaperSort::order_by`.
CREATE INDEX IF NOT EXISTS idx_paper_meta_published_dated
    ON PAPER_META ((PUBLISHED > '0001-01-01') DESC, PUBLISHED);
