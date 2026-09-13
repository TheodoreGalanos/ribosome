-- Pinned checker and intervention identities at preflight. Existing effects
-- have no invented check prerequisites or validation evidence.
ALTER TABLE effects ADD COLUMN preflight TEXT;
PRAGMA user_version=13;
