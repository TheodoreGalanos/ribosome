CREATE TABLE run_waits (
    run_id TEXT PRIMARY KEY REFERENCES runs(id),
    work_ids TEXT NOT NULL CHECK(json_valid(work_ids))
);
ALTER TABLE work ADD COLUMN source_ref TEXT REFERENCES events(id);
PRAGMA user_version=17;
