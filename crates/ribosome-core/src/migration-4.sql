CREATE TABLE context_segments(id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(id), predecessor TEXT REFERENCES context_segments(id), reason TEXT NOT NULL);
CREATE INDEX context_segments_run ON context_segments(run_id);
CREATE TABLE context_items(segment_id TEXT NOT NULL REFERENCES context_segments(id), sequence INTEGER NOT NULL, previous_sequence INTEGER, kind TEXT NOT NULL, body TEXT, PRIMARY KEY(segment_id,sequence));
CREATE TABLE context_sources(segment_id TEXT NOT NULL REFERENCES context_segments(id), first_sequence INTEGER NOT NULL, kind TEXT NOT NULL, id TEXT NOT NULL, version TEXT NOT NULL, PRIMARY KEY(segment_id,kind,id,version));
CREATE INDEX context_sources_reverse ON context_sources(kind,id);
PRAGMA user_version=4;
