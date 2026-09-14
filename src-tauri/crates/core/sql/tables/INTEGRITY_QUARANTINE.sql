-- Rows a destructive migration would otherwise drop without a record --
-- the row itself lands here as JSON, not just a count. RESOLVED_AT marks
-- entries a later manual repair has handled.
CREATE TABLE IF NOT EXISTS INTEGRITY_QUARANTINE (
    QUARANTINE_FK INTEGER   PRIMARY KEY AUTOINCREMENT,
    SOURCE_TABLE  TEXT      NOT NULL,
    SOURCE_KEY    TEXT      NOT NULL,
    REASON        TEXT      NOT NULL,
    PAYLOAD_JSON  TEXT      NOT NULL,
    CREATED_AT    TIMESTAMP NOT NULL DEFAULT (datetime('now')),
    RESOLVED_AT   TIMESTAMP
);
