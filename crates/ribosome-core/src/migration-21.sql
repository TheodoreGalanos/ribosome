CREATE TABLE protected_exposures(client TEXT NOT NULL, project TEXT NOT NULL, family TEXT NOT NULL, experiment_id TEXT NOT NULL, policy_id TEXT NOT NULL, PRIMARY KEY(client,project,family));
INSERT OR IGNORE INTO protected_exposures
SELECT json_extract(g.body,'$.scope.client'),json_extract(g.body,'$.scope.project'),json_extract(c.value,'$.family'),e.id,e.policy_id
FROM experiments e JOIN grants g ON g.id=e.grant_id JOIN json_each(e.policy,'$.cases') c
WHERE json_extract(c.value,'$.split')='holdout' ORDER BY e.rowid;
ALTER TABLE archive ADD COLUMN evidence TEXT;
PRAGMA user_version=21;
CREATE TABLE evaluation_sources(grant_id TEXT NOT NULL REFERENCES grants(id), owner_grant_id TEXT NOT NULL REFERENCES grants(id), kind TEXT NOT NULL, source_id TEXT NOT NULL, deliverable INTEGER NOT NULL, PRIMARY KEY(grant_id,kind,source_id));
