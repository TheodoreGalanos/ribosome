CREATE TABLE discovery_corpora (
    client TEXT NOT NULL,
    project TEXT NOT NULL,
    id TEXT NOT NULL,
    version TEXT NOT NULL,
    body TEXT NOT NULL CHECK(json_valid(body)),
    PRIMARY KEY(client, project, id, version)
);
PRAGMA user_version=19;
