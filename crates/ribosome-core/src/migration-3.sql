CREATE TABLE source_edges(subject_kind TEXT NOT NULL, subject_id TEXT NOT NULL, source_kind TEXT NOT NULL, source_id TEXT NOT NULL, source_version TEXT, requires_access INTEGER NOT NULL DEFAULT 1, PRIMARY KEY(subject_kind,subject_id,source_kind,source_id));
CREATE INDEX source_edges_reverse ON source_edges(source_kind,source_id);
CREATE TABLE source_tombstones(kind TEXT NOT NULL, id TEXT NOT NULL, client TEXT NOT NULL, project TEXT NOT NULL, deleted INTEGER NOT NULL, PRIMARY KEY(kind,id));
CREATE TABLE source_policy(client TEXT NOT NULL,project TEXT NOT NULL,generation INTEGER NOT NULL,PRIMARY KEY(client,project));
CREATE TABLE source_cleanup(source_id TEXT PRIMARY KEY, client TEXT NOT NULL, project TEXT NOT NULL, delete_content INTEGER NOT NULL, status TEXT NOT NULL, error TEXT);
PRAGMA user_version=3;
