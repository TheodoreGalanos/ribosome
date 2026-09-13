-- Older copies cannot prove which ancestry they observed when a referenced
-- record has changed. Preserve their payloads, but deny them conservatively.
-- Schema-ten exports already captured their complete reviewed closure.
WITH subjects AS (
  SELECT 'record' AS kind,id,client,project FROM records
  UNION ALL SELECT 'event',id,client,project FROM events
  UNION ALL SELECT 'artifact',id,client,project FROM artifact_snapshots WHERE export_file IS NULL
)
INSERT OR IGNORE INTO source_tombstones(kind,id,client,project,deleted)
SELECT DISTINCT s.kind,s.id,s.client,s.project,0
FROM subjects s JOIN source_edges e ON e.subject_kind=s.kind AND e.subject_id=s.id
JOIN records r ON e.source_kind='record' AND e.source_id=r.id
WHERE CAST(r.version AS INTEGER)>1 AND (e.source_version IS NULL OR e.source_version<>r.version);
UPDATE artifact_snapshots SET available=0 WHERE id IN (SELECT id FROM source_tombstones WHERE kind='artifact');
INSERT INTO source_policy(client,project,generation)
SELECT DISTINCT client,project,1 FROM source_tombstones WHERE 1
ON CONFLICT(client,project) DO UPDATE SET generation=generation+1;
PRAGMA user_version=11;
