CREATE TABLE context_summaries(id TEXT PRIMARY KEY,segment_id TEXT NOT NULL REFERENCES context_segments(id),after_sequence INTEGER NOT NULL,through_sequence INTEGER NOT NULL,predecessor_id TEXT REFERENCES context_summaries(id),status TEXT NOT NULL DEFAULT 'prepared',body TEXT,permit_id TEXT,created_ms TEXT NOT NULL);
CREATE INDEX context_summaries_frontier ON context_summaries(segment_id,through_sequence);
ALTER TABLE permits ADD COLUMN compaction_id TEXT REFERENCES context_summaries(id);
ALTER TABLE context_items ADD COLUMN summary_ref TEXT REFERENCES context_summaries(id);
PRAGMA user_version=6;
