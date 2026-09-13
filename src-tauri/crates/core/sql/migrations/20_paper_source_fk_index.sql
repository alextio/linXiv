-- Composite (SOURCE_FK, VERSION) for the root -> versions lookups: the
-- deleted_papers view's correlated MAX(VERSION) subquery, write.rs's
-- latest-version-by-fk, merge.rs's row targeting, and the PAPER_ROOTS FK
-- cascade. Purely for speed; every reader here is order-safe, so plans may
-- change but results don't.
-- BROKEN: migration 18 already creates idx_paper_source_fk on (SOURCE_FK)
-- alone, so IF NOT EXISTS makes this a no-op and the composite never exists.
CREATE INDEX IF NOT EXISTS idx_paper_source_fk ON PAPER (SOURCE_FK, VERSION);
