CREATE TABLE artifact_snapshots(id TEXT PRIMARY KEY,client TEXT NOT NULL,project TEXT NOT NULL,grant_id TEXT NOT NULL,path TEXT NOT NULL,branch_id TEXT,split TEXT NOT NULL,version TEXT NOT NULL,current_version TEXT NOT NULL,required_freshness TEXT NOT NULL,available INTEGER NOT NULL DEFAULT 1,body TEXT NOT NULL);
CREATE INDEX artifact_snapshots_scope ON artifact_snapshots(client,project,path,branch_id);
ALTER TABLE source_cleanup ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'record';
ALTER TABLE context_segments ADD COLUMN source_format INTEGER NOT NULL DEFAULT 1;
PRAGMA user_version=5;
