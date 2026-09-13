CREATE TABLE source_cleanup_items(sequence INTEGER PRIMARY KEY AUTOINCREMENT,job_id TEXT NOT NULL REFERENCES source_cleanup(source_id),kind TEXT NOT NULL,id TEXT NOT NULL,after_edge INTEGER NOT NULL DEFAULT 0,payload_done INTEGER NOT NULL DEFAULT 0,complete INTEGER NOT NULL DEFAULT 0,UNIQUE(job_id,kind,id));
CREATE INDEX source_cleanup_frontier ON source_cleanup_items(job_id,complete,sequence);
CREATE INDEX source_cleanup_source ON source_cleanup_items(kind,id,job_id);
-- Recheck earlier jobs with the resumable traversal. Existing tombstones remain authoritative.
UPDATE source_cleanup SET status='pending',error=NULL;
PRAGMA user_version=7;
