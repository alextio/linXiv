-- Composite (SOURCE_FK, VERSION) for the root -> versions lookups: the
-- deleted_papers view's correlated MAX(VERSION) subquery, write.rs's
-- latest-version-by-fk, merge.rs's row targeting, and the PAPER_ROOTS FK
-- cascade. Purely for speed; every reader here is order-safe, so plans may
-- change but results don't.
-- The DROP retires 18's old single-column idx_paper_source_fk on upgraded
-- installs (18 no longer creates it); the composite's left prefix serves
-- every SOURCE_FK-only lookup it used to.
DROP INDEX IF EXISTS idx_paper_source_fk;
CREATE INDEX IF NOT EXISTS idx_paper_source_fk_version ON PAPER (SOURCE_FK, VERSION);
