-- A ledger as schema version 8 wrote it, for the migration tests.
--
-- Made from ledger-v7.sql by schema 8's own step (`SCHEMA_V8`, the five
-- columns it gives an artifact), then given in SQL what a gather of a
-- schema 8 build writes, `saw_source` and `record` as that build spells
-- them: `acme-skills` gathered again with `--path skills --pin`, its two
-- skills recorded with where they are in the repository, their hash and
-- their licence (one its manifest declares, one nothing says), and a
-- source of rules, `acme-rules`, gathered with `--kind rules --path rules`,
-- its one rule under the licence of the folder it came from. No source is
-- listed by a marketplace, which no build of schema 8 gathered from, and
-- none has an entry, which schema 9 adds. Dumped with `sqlite3
-- mindweaver.db .dump`, the times a minute apart after version 7's. A
-- dump leaves out the schema version, so the last line puts it back.
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
INSERT INTO sources VALUES(1,'git','https://github.com/acme/skills',NULL,'skills','b41e7d0c9a2f5e8d1c3b6a9f0e2d4c7b8a1f3e5d','github.com-acme-skills',1795000000,1795000240,'acme-skills','https://github.com/acme/skills','','b41e7d0c9a2f5e8d1c3b6a9f0e2d4c7b8a1f3e5d',NULL);
INSERT INTO sources VALUES(2,'git','https://github.com/acme/skills','stable','skills','9c1e5b0a7f3d2e4c6b8a0f1d3e5c7b9a1f2e4d6c','github.com-acme-skills-2',1795000060,1795000060,'acme-skills-2','https://github.com/acme/skills','stable',NULL,NULL);
INSERT INTO sources VALUES(3,'git','git@github.com:acme/private.git',NULL,'skills','666abf2d70ce25ed286a9c65aa5fd323b1ac8612','github.com-acme-private',1795000120,1795000120,'team-private','https://github.com/acme/private','',NULL,NULL);
INSERT INTO sources VALUES(4,'git','https://github.com/acme/rules',NULL,'rules','7d3a9f1c5e8b2d4f6a0c9e1b3d5f7a8c2e4b6d0f','github.com-acme-rules',1795000300,1795000300,'acme-rules','https://github.com/acme/rules','',NULL,NULL);
CREATE TABLE artifacts (
    id          INTEGER PRIMARY KEY,
    kind        TEXT    NOT NULL,
    name        TEXT    NOT NULL,
    source_id   INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    path        TEXT    NOT NULL,
    summary     TEXT,
    revision    TEXT,
    gathered_at INTEGER NOT NULL, upstream_path TEXT, content_hash TEXT, licence_from TEXT, licence TEXT, licence_file TEXT,
    UNIQUE (kind, name, source_id)
);
INSERT INTO artifacts VALUES(1,'skill','deploy',1,'.mindweaver/skills/github.com-acme-skills/deploy','Ship the service to staging','b41e7d0c9a2f5e8d1c3b6a9f0e2d4c7b8a1f3e5d',1795000240,'skills/deploy','3c8f1e6a9b2d5f7c0e4a8b1d6f3c9e2a5b7d0f4c8e1a3b6d9f2c5e8a0b4d7f1c','manifest','MIT',NULL);
INSERT INTO artifacts VALUES(2,'skill','review',1,'.mindweaver/skills/github.com-acme-skills/review','Review a change before it merges','b41e7d0c9a2f5e8d1c3b6a9f0e2d4c7b8a1f3e5d',1795000240,'skills/review','9e2b5d8f1a4c7e0b3d6f9a2c5e8b1d4f7a0c3e6b9d2f5a8c1e4b7d0f3a6c9e2b','unknown',NULL,NULL);
INSERT INTO artifacts VALUES(3,'skill','deploy',2,'.mindweaver/skills/github.com-acme-skills-2/deploy','Ship the service to staging','9c1e5b0a7f3d2e4c6b8a0f1d3e5c7b9a1f2e4d6c',1795000060,NULL,NULL,NULL,NULL,NULL);
INSERT INTO artifacts VALUES(4,'skill','audit',3,'.mindweaver/skills/github.com-acme-private/audit','Audit what a change lets in','666abf2d70ce25ed286a9c65aa5fd323b1ac8612',1795000120,NULL,NULL,NULL,NULL,NULL);
INSERT INTO artifacts VALUES(5,'rule','git/commits',4,'.mindweaver/rules/github.com-acme-rules/git/commits.md','Write commits one step at a time','7d3a9f1c5e8b2d4f6a0c9e1b3d5f7a8c2e4b6d0f',1795000300,'rules/git/commits.md','5f0a3c6e9b2d4f8a1c7e0b3d5f9a2c4e6b8d1f3a5c7e9b0d2f4a6c8e1b3d5f7a','file','MIT License','LICENSE');
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
INSERT INTO actions VALUES(7,1795000240,'gather','https://github.com/acme/skills','ok','0 added, 2 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(8,1795000300,'gather','https://github.com/acme/rules','ok','1 added, 0 updated, 0 unchanged, 0 failed');
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
PRAGMA user_version = 8;
