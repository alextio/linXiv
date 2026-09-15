-- SQLite cannot ADD a FK to an existing table: rebuild NOTE with the composite
-- same-lineage FK (see tables/NOTE.sql). A pre-existing cross-lineage pin is
-- nulled by the scalar subquery -- the repair ON DELETE SET NULL would produce
-- -- so no note row is ever dropped. The DROP takes NOTE's indexes and the
-- notes_fts triggers with it; indexes are recreated here, triggers by the
-- caller re-running tables/notes_fts.sql (apply_tables already ran this open).
BEGIN;
 CREATE TABLE NOTE_NEW(
     NOTE_SK     INTEGER NOT NULL,
     SOURCE_FK   INTEGER NOT NULL,
     PAPER_ID_FK INTEGER,
     PROJECT_FK  INTEGER,
     TITLE       TEXT,
     NOTE        BLOB,
     NOTE_UUID   TEXT,
     CREATED_AT  TIMESTAMP NOT NULL DEFAULT (datetime('now')),
     UPDATED_AT  TIMESTAMP NOT NULL DEFAULT (datetime('now')),
     PRIMARY KEY (NOTE_SK),
     FOREIGN KEY (SOURCE_FK)   REFERENCES PAPER_ROOTS(SOURCE_FK) ON DELETE CASCADE,
     FOREIGN KEY (PAPER_ID_FK) REFERENCES PAPER(PAPER_ID)        ON DELETE SET NULL,
     FOREIGN KEY (PROJECT_FK)  REFERENCES PROJECT(PROJECT_FK),
     FOREIGN KEY (PAPER_ID_FK, SOURCE_FK) REFERENCES PAPER(PAPER_ID, SOURCE_FK)
         DEFERRABLE INITIALLY DEFERRED
 );
 INSERT INTO NOTE_NEW (NOTE_SK, SOURCE_FK, PAPER_ID_FK, PROJECT_FK,
                       TITLE, NOTE, NOTE_UUID, CREATED_AT, UPDATED_AT)
     SELECT n.NOTE_SK, n.SOURCE_FK,
            (SELECT p.PAPER_ID FROM PAPER p
              WHERE p.PAPER_ID = n.PAPER_ID_FK AND p.SOURCE_FK = n.SOURCE_FK),
            n.PROJECT_FK, n.TITLE, n.NOTE, n.NOTE_UUID, n.CREATED_AT, n.UPDATED_AT
     FROM NOTE n;
 DROP TABLE NOTE;
 ALTER TABLE NOTE_NEW RENAME TO NOTE;
 CREATE INDEX IF NOT EXISTS idx_note_source_fk ON NOTE (SOURCE_FK);
 CREATE INDEX IF NOT EXISTS idx_note_project_fk ON NOTE (PROJECT_FK);
 CREATE INDEX IF NOT EXISTS idx_note_paper_id_fk ON NOTE (PAPER_ID_FK);
 CREATE UNIQUE INDEX IF NOT EXISTS idx_note_uuid_unique ON NOTE (NOTE_UUID);
 COMMIT;
