-- Make link_author_to_paper's INSERT OR IGNORE actually idempotent: without a
-- UNIQUE constraint a re-link (double-click before the refetch lands) inserted
-- a duplicate row, inflating paper_count and per-author listings. Existing
-- duplicates keep their oldest row (the original AUTHOR_INDEX).
DELETE FROM PAPER_TO_AUTHOR
 WHERE PTA_FK NOT IN (
     SELECT MIN(PTA_FK) FROM PAPER_TO_AUTHOR GROUP BY PAPER_ID, AUTHOR_FK
 );
CREATE UNIQUE INDEX IF NOT EXISTS idx_paper_to_author_unique
    ON PAPER_TO_AUTHOR (PAPER_ID, AUTHOR_FK);
