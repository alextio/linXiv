-- Trail of identity-touching paper operations: repair keeps old==new unless
-- the SOURCE_ID changed; merge records loser -> winner. No FKs on purpose --
-- the old id usually no longer exists.
CREATE TABLE IF NOT EXISTS PAPER_REPAIRS (
    REPAIR_FK     INTEGER   PRIMARY KEY AUTOINCREMENT,
    OLD_SOURCE_ID TEXT      NOT NULL,
    NEW_SOURCE_ID TEXT      NOT NULL,
    ACTOR         TEXT      NOT NULL,
    CREATED_AT    TIMESTAMP NOT NULL DEFAULT (datetime('now'))
);
