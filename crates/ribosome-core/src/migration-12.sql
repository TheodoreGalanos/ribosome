ALTER TABLE effects ADD COLUMN phase TEXT NOT NULL DEFAULT 'prepared'
  CHECK(phase IN ('prepared','dispatching','observed','finalized'));
ALTER TABLE effects ADD COLUMN authority TEXT;
ALTER TABLE effects ADD COLUMN potential_writes TEXT NOT NULL DEFAULT '[]';
ALTER TABLE effects ADD COLUMN observation TEXT;
ALTER TABLE effects ADD COLUMN validation_generation INTEGER NOT NULL DEFAULT 0;
-- Old intents have no dispatch evidence. Treat them as potentially issued;
-- preserve terminal observations without inventing validation certificates.
UPDATE effects SET phase=CASE WHEN json_extract(body,'$.status')='started'
  THEN 'dispatching' ELSE 'finalized' END;
UPDATE effects SET authority=(SELECT body FROM grants WHERE grants.id=effects.grant_id);
ALTER TABLE invalidated ADD COLUMN generation INTEGER NOT NULL DEFAULT 0;
CREATE TABLE validity_clock(client TEXT NOT NULL, project TEXT NOT NULL,
  generation INTEGER NOT NULL, PRIMARY KEY(client,project));
PRAGMA user_version=12;
