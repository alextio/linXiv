-- Per-paper reading status inside a reading-list PROJECT, stored SPARSELY: unread
-- is the ABSENCE of a row, so setting a paper back to unread deletes its row and
-- only 'reading'/'read' are ever stored. One status per paper per reading list,
-- covering every version of the paper (SOURCE_FK → PAPER_ROOTS, like membership).
-- The composite FK targets the paper's PROJECT_TO_PAPER membership row, not
-- PROJECT/PAPER_ROOTS: dropping a paper from a project cascades its status away,
-- so status can't outlive membership and silently resurrect on re-add. That needs
-- PROJECT_TO_PAPER(PROJECT_FK, SOURCE_FK) uniquely indexed by DML time (see that
-- file), and needs the removal to be a genuine delete, not a blanket
-- delete+reinsert of unchanged rows (that would cascade-clear this table too;
-- see save_source_fks).
CREATE TABLE IF NOT EXISTS PAPER_TO_READING(
    PROJECT_FK  INTEGER NOT NULL,
    SOURCE_FK   INTEGER NOT NULL,
    STATUS      TEXT    NOT NULL CHECK (STATUS IN ('reading', 'read')),
    UPDATED_AT  TIMESTAMP NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (PROJECT_FK, SOURCE_FK),
    FOREIGN KEY (PROJECT_FK, SOURCE_FK) REFERENCES PROJECT_TO_PAPER(PROJECT_FK, SOURCE_FK) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_paper_to_reading_source_fk ON PAPER_TO_READING (SOURCE_FK);
