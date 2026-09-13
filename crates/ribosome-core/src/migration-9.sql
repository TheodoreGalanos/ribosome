-- Old message and completion bodies remain observations, with unknown lineage.
ALTER TABLE messages ADD COLUMN source_format INTEGER NOT NULL DEFAULT 0;
ALTER TABLE runs ADD COLUMN result_source TEXT REFERENCES events(id);
CREATE INDEX run_result_source ON runs(result_source) WHERE result_source IS NOT NULL;
-- These schema-eight observations did not retain sender lineage. Their known
-- descendants become unavailable without inventing the missing relationships.
UPDATE artifact_snapshots SET available=0 WHERE result_method IN ('message.send','message.inbox');
INSERT INTO source_policy(client,project,generation)
SELECT DISTINCT client,project,1 FROM artifact_snapshots WHERE result_method IN ('message.send','message.inbox')
ON CONFLICT(client,project) DO UPDATE SET generation=generation+1;
-- Revisit completed deletions now that additional copied stores are covered.
UPDATE source_cleanup SET status='pending',error=NULL WHERE delete_content=1;
UPDATE source_cleanup_items SET payload_done=0,complete=0,after_edge=0
WHERE job_id IN (SELECT source_id FROM source_cleanup WHERE delete_content=1);
PRAGMA user_version=9;
