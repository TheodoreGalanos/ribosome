-- Export payloads live in managed files; snapshots retain their source closure.
-- available=0 until file publication commits, so interrupted writes are unreadable.
ALTER TABLE artifact_snapshots ADD COLUMN export_file TEXT;
CREATE UNIQUE INDEX artifact_export_file ON artifact_snapshots(export_file) WHERE export_file IS NOT NULL;
PRAGMA user_version=10;
