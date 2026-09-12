ALTER TABLE subscriptions ADD COLUMN source_run TEXT;
ALTER TABLE subscriptions ADD COLUMN attachment_id TEXT;
CREATE TABLE attachments(id TEXT PRIMARY KEY, grant_id TEXT NOT NULL REFERENCES grants(id), execution_id TEXT NOT NULL, body TEXT NOT NULL, UNIQUE(grant_id,execution_id));
CREATE TABLE attachment_work(work_id TEXT PRIMARY KEY REFERENCES work(id), attachment_id TEXT NOT NULL REFERENCES attachments(id));
CREATE INDEX attachment_work_owner ON attachment_work(attachment_id);
CREATE TABLE attachment_records(run_id TEXT NOT NULL REFERENCES runs(id), record_id TEXT NOT NULL REFERENCES records(id), PRIMARY KEY(run_id,record_id));
CREATE TABLE attachment_feedback(id TEXT PRIMARY KEY, attachment_id TEXT NOT NULL REFERENCES attachments(id), body TEXT NOT NULL);
CREATE INDEX attachment_feedback_owner ON attachment_feedback(attachment_id);
PRAGMA user_version=2;
