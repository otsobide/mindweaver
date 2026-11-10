-- A ledger as schema version 7 wrote it, for the migration tests.
--
-- Made by the Mindweaver of schema 7 (feature/98-source-names at 17ae482),
-- through core's own calls, a remote of the test's own standing in for
-- GitHub so that every URL is one a person would gather: two projects
-- marked by their settings, one naming Claude alone and one every agent,
-- each known by its id; one repository gathered at the remote's HEAD and
-- at `stable`, the second source of its name and so numbered; a second
-- repository gathered over ssh under a name of its own (`--name
-- team-private`); and a skill installed from each source but `stable`'s.
-- No source is pinned, which no build of schema 7 writes, and no artifact
-- has an upstream path, a content hash or a licence, which schema 8 adds.
-- Dumped with `sqlite3 mindweaver.db .dump`, the times pinned a minute
-- apart by the clock the ledger was handed. A dump leaves out the schema
-- version, so the last line puts it back.
PRAGMA foreign_keys=OFF;
BEGIN TRANSACTION;
CREATE TABLE sources (
    id           INTEGER PRIMARY KEY,
    kind         TEXT    NOT NULL,
    url          TEXT    NOT NULL,
    reference    TEXT,
    subdirectory TEXT    NOT NULL,
    revision     TEXT,
    directory    TEXT    NOT NULL UNIQUE,
    first_seen   INTEGER NOT NULL,
    last_seen    INTEGER NOT NULL, name TEXT NOT NULL DEFAULT '', canonical_url TEXT NOT NULL DEFAULT '', reference_key TEXT NOT NULL DEFAULT '', pin TEXT, parent_id INTEGER REFERENCES sources(id) ON DELETE CASCADE,
    UNIQUE (kind, url, reference, subdirectory)
);
INSERT INTO sources VALUES(1,'git','https://github.com/acme/skills',NULL,'skills','5a79753785072c5ce6f0463fc9540f025301a25e','github.com-acme-skills',1795000000,1795000000,'acme-skills','https://github.com/acme/skills','',NULL,NULL);
INSERT INTO sources VALUES(2,'git','https://github.com/acme/skills','stable','skills','9c1e5b0a7f3d2e4c6b8a0f1d3e5c7b9a1f2e4d6c','github.com-acme-skills-2',1795000060,1795000060,'acme-skills-2','https://github.com/acme/skills','stable',NULL,NULL);
INSERT INTO sources VALUES(3,'git','git@github.com:acme/private.git',NULL,'skills','666abf2d70ce25ed286a9c65aa5fd323b1ac8612','github.com-acme-private',1795000120,1795000120,'team-private','https://github.com/acme/private','',NULL,NULL);
CREATE TABLE artifacts (
    id          INTEGER PRIMARY KEY,
    kind        TEXT    NOT NULL,
    name        TEXT    NOT NULL,
    source_id   INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    path        TEXT    NOT NULL,
    summary     TEXT,
    revision    TEXT,
    gathered_at INTEGER NOT NULL,
    UNIQUE (kind, name, source_id)
);
INSERT INTO artifacts VALUES(1,'skill','deploy',1,'.mindweaver/skills/github.com-acme-skills/deploy','Ship the service to staging','5a79753785072c5ce6f0463fc9540f025301a25e',1795000000);
INSERT INTO artifacts VALUES(2,'skill','review',1,'.mindweaver/skills/github.com-acme-skills/review','Review a change before it merges','5a79753785072c5ce6f0463fc9540f025301a25e',1795000000);
INSERT INTO artifacts VALUES(3,'skill','deploy',2,'.mindweaver/skills/github.com-acme-skills-2/deploy','Ship the service to staging','9c1e5b0a7f3d2e4c6b8a0f1d3e5c7b9a1f2e4d6c',1795000060);
INSERT INTO artifacts VALUES(4,'skill','audit',3,'.mindweaver/skills/github.com-acme-private/audit','Audit what a change lets in','666abf2d70ce25ed286a9c65aa5fd323b1ac8612',1795000120);
CREATE TABLE actions (
    id      INTEGER PRIMARY KEY,
    at      INTEGER NOT NULL,
    action  TEXT    NOT NULL,
    target  TEXT,
    outcome TEXT    NOT NULL,
    detail  TEXT
);
INSERT INTO actions VALUES(1,1795000000,'gather','https://github.com/acme/skills','ok','2 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(2,1795000060,'gather','https://github.com/acme/skills','ok','1 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(3,1795000120,'gather','git@github.com:acme/private.git','ok','1 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(4,1795000180,'install','collapse','ok','deploy into collapse');
INSERT INTO actions VALUES(5,1795000180,'install','tanukeys','ok','review into tanukeys');
INSERT INTO actions VALUES(6,1795000180,'install','tanukeys','ok','audit into tanukeys');
CREATE TABLE installations (
    id           INTEGER PRIMARY KEY,
    project      TEXT    NOT NULL,
    kind         TEXT    NOT NULL,
    name         TEXT    NOT NULL,
    source_id    INTEGER REFERENCES sources(id) ON DELETE SET NULL,
    installed_at INTEGER NOT NULL, origin TEXT, content_hash TEXT, project_id TEXT REFERENCES projects(id) ON UPDATE CASCADE ON DELETE SET NULL,
    UNIQUE (project, kind, name)
);
INSERT INTO installations VALUES(1,'collapse','skill','deploy',1,1795000180,'https://github.com/acme/skills','f739b82ff0a9246dc21294934531ea8fe92c35d1fe0d0eee146dc9de1cab9d68','19872c71d3f39fc339e98e7f7d7f60d7');
INSERT INTO installations VALUES(2,'tanukeys','skill','review',1,1795000180,'https://github.com/acme/skills','0751377f031de19e4e54bfde0b70864343730422988ecf4ee990df7311046d85','36c2de99b56215ba1b6a227aab999f56');
INSERT INTO installations VALUES(3,'tanukeys','skill','audit',3,1795000180,'git@github.com:acme/private.git','44a100e014eeb92c29af3ac9c51027890b546305639a84d06e1d71902ce8cd1e','36c2de99b56215ba1b6a227aab999f56');
CREATE TABLE installation_targets (
    installation_id INTEGER NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
    agent           TEXT,
    path            TEXT    NOT NULL, content_hash TEXT,
    UNIQUE (installation_id, path)
);
INSERT INTO installation_targets VALUES(1,'claude','.claude/skills/deploy',NULL);
INSERT INTO installation_targets VALUES(2,NULL,'.agents/skills/review',NULL);
INSERT INTO installation_targets VALUES(2,'claude','.claude/skills/review',NULL);
INSERT INTO installation_targets VALUES(3,NULL,'.agents/skills/audit',NULL);
INSERT INTO installation_targets VALUES(3,'claude','.claude/skills/audit',NULL);
CREATE TABLE created_files (
    project    TEXT    NOT NULL,
    path       TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (project, path)
);
CREATE TABLE projects (
    id         TEXT    PRIMARY KEY,
    last_route TEXT    NOT NULL UNIQUE
);
INSERT INTO projects VALUES('19872c71d3f39fc339e98e7f7d7f60d7','collapse');
INSERT INTO projects VALUES('36c2de99b56215ba1b6a227aab999f56','tanukeys');
CREATE TABLE installation_entries (
    installation_id INTEGER NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
    agent           TEXT    NOT NULL,
    file            TEXT    NOT NULL,
    name            TEXT    NOT NULL,
    fingerprint     TEXT    NOT NULL,
    UNIQUE (installation_id, agent)
);
CREATE TABLE merged_sources (
    id        INTEGER PRIMARY KEY,
    directory TEXT    NOT NULL UNIQUE,
    url       TEXT    NOT NULL,
    reference TEXT,
    into_id   INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE
);
CREATE INDEX artifacts_by_name ON artifacts (kind, name);
CREATE INDEX actions_by_time ON actions (at);
CREATE INDEX installations_by_project ON installations (project);
CREATE INDEX installations_by_project_id ON installations (project_id);
CREATE UNIQUE INDEX sources_by_identity
    ON sources (kind, canonical_url, reference_key, subdirectory);
CREATE UNIQUE INDEX sources_by_name ON sources (name);
COMMIT;
PRAGMA user_version = 7;
