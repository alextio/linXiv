-- ?1 = loser TAG_FK about to be deleted, ?2 = canonical TAG_FK it folds into.
INSERT INTO INTEGRITY_QUARANTINE (SOURCE_TABLE, SOURCE_KEY, REASON, PAYLOAD_JSON)
SELECT 'TAG', TAG_FK,
       'tag_label_unique_index: case-fold duplicate of TAG_FK ' || ?2,
       json_object('TAG_FK', TAG_FK, 'TAG', TAG,
                   'CREATED_AT', CREATED_AT, 'UPDATED_AT', UPDATED_AT)
FROM TAG WHERE TAG_FK = ?1
