ALTER TABLE runs ADD COLUMN timings TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(timings));
ALTER TABLE permits ADD COLUMN dispatched_ms TEXT;
ALTER TABLE permits ADD COLUMN observed_ms TEXT;
-- Legacy rows have no measured dispatch/observation timestamps.
PRAGMA user_version=18;
