CREATE TABLE budget_allocations (
    id TEXT PRIMARY KEY,
    grant_id TEXT NOT NULL REFERENCES grants(id),
    parent_id TEXT REFERENCES budget_allocations(id),
    body TEXT NOT NULL
);
CREATE INDEX budget_children ON budget_allocations(parent_id);
CREATE UNIQUE INDEX budget_root ON budget_allocations(grant_id) WHERE parent_id IS NULL;
CREATE TABLE run_allocations (
    run_id TEXT PRIMARY KEY REFERENCES runs(id),
    allocation_id TEXT NOT NULL REFERENCES budget_allocations(id)
);
ALTER TABLE permits ADD COLUMN allocation_id TEXT REFERENCES budget_allocations(id);
ALTER TABLE permits ADD COLUMN call_id TEXT;
ALTER TABLE permits ADD COLUMN request TEXT;
-- Legacy permits have no proof that transport did not start. Keep their
-- liability until their existing usage says otherwise; never release it here.
ALTER TABLE permits ADD COLUMN state TEXT NOT NULL DEFAULT 'dispatched';
UPDATE permits SET state='settled' WHERE json_extract(usage,'$.complete')=1;
CREATE UNIQUE INDEX permit_call ON permits(run_id,call_id) WHERE call_id IS NOT NULL;
CREATE INDEX permit_allocation ON permits(allocation_id);
ALTER TABLE effects ADD COLUMN allocation_id TEXT REFERENCES budget_allocations(id);
CREATE INDEX effect_allocation ON effects(allocation_id);
ALTER TABLE work ADD COLUMN allocation_id TEXT REFERENCES budget_allocations(id);
CREATE INDEX work_allocation ON work(allocation_id);
PRAGMA user_version=16;
