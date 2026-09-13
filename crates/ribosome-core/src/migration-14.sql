CREATE TABLE artifact_invalidation_generations (
  client TEXT NOT NULL, project TEXT NOT NULL, path TEXT NOT NULL,
  generation INTEGER NOT NULL, PRIMARY KEY(client,project,path)
);
INSERT INTO artifact_invalidation_generations SELECT client,project,path,generation FROM invalidated;
CREATE TABLE property_validations (
  client TEXT NOT NULL, project TEXT NOT NULL, path TEXT NOT NULL,
  obligation_id TEXT NOT NULL, obligation_version TEXT NOT NULL,
  artifact_version TEXT NOT NULL, operation_id TEXT NOT NULL REFERENCES effects(id),
  generation INTEGER NOT NULL,
  PRIMARY KEY(client,project,path,obligation_id)
);
ALTER TABLE artifact_snapshots ADD COLUMN version_only INTEGER NOT NULL DEFAULT 0;
PRAGMA user_version=14;
