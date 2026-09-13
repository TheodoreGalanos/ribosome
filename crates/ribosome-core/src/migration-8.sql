-- Immutable tool-result artifacts share source authorization and cleanup with artifact observations.
ALTER TABLE artifact_snapshots ADD COLUMN result_run_id TEXT REFERENCES runs(id);
ALTER TABLE artifact_snapshots ADD COLUMN result_call_id TEXT;
ALTER TABLE artifact_snapshots ADD COLUMN result_method TEXT;
ALTER TABLE artifact_snapshots ADD COLUMN result_args_hash TEXT;
ALTER TABLE artifact_snapshots ADD COLUMN result_content TEXT;
ALTER TABLE artifact_snapshots ADD COLUMN result_sources TEXT;
CREATE UNIQUE INDEX artifact_result_call ON artifact_snapshots(result_run_id,result_call_id) WHERE result_run_id IS NOT NULL;
PRAGMA user_version=8;
