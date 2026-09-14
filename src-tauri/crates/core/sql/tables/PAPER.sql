CREATE TABLE IF NOT EXISTS PAPER(
    PAPER_ID    INTEGER PRIMARY KEY AUTOINCREMENT,
    SOURCE_ID   TEXT    NOT NULL,
    VERSION     INTEGER NOT NULL,
    TITLE       TEXT    NOT NULL,
    CATEGORY    TEXT,
    HAS_PDF     BOOL NOT NULL DEFAULT 0,
    CREATED_AT  TIMESTAMP NOT NULL DEFAULT (datetime('now')),
    UPDATED_AT  TIMESTAMP NOT NULL DEFAULT (datetime('now')),
    SOURCE_FK   INTEGER NOT NULL,
    UNIQUE (SOURCE_ID, VERSION),
    FOREIGN KEY (SOURCE_FK) REFERENCES PAPER_ROOTS(SOURCE_FK) ON DELETE CASCADE
);

-- Parent side of NOTE's composite same-lineage FK. In the base file, not a
-- migration: apply_tables re-runs this on every open, and the index must exist
-- before any NOTE DML on both fresh and legacy installs. PAPER_ID leads so the
-- planner never prefers it over idx_paper_source_fk_version for SOURCE_FK scans.
CREATE UNIQUE INDEX IF NOT EXISTS idx_paper_lineage_unique ON PAPER (PAPER_ID, SOURCE_FK);
