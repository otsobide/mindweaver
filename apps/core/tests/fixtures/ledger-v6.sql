-- A ledger as schema version 6 wrote it, for the migration tests.
--
-- Made by the Mindweaver of schema 6 (dev at f794d8b): two projects marked
-- by their settings, one naming Claude alone and one every agent, each
-- known by its id; one repository gathered by three spellings of its URL,
-- two at the remote's HEAD and one at `main`; a second repository gathered
-- once; and a skill installed from each spelling of the first but `main`'s
-- and one from the second. No MCP server, and nothing of the workspace's
-- own, which the step to schema 7 does not touch. Dumped with `sqlite3
-- mindweaver.db .dump`, with the temporary paths it ran in replaced by the
-- addresses below and the times set a minute apart, so that the second
-- spelling of the first repository is the one gathered last.
--
-- The second repository's URL, and its log, are then as a Mindweaver from
-- before #25 stored them, with a token in them, which a read migrates
-- without scrubbing and so a ledger of schema 6 can still hold: no build of
-- schema 6 stores one, so they were written in by hand, and so was the
-- older spelling of it, gathered before and without the token. A token is
-- `<token>`, which a test fills in with one made as it runs, so that
-- nothing here looks like a secret. A dump leaves out the schema version,
-- so the last line puts it back.
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
    last_seen    INTEGER NOT NULL,
    UNIQUE (kind, url, reference, subdirectory)
);
INSERT INTO sources VALUES(1,'git','https://github.com/acme/skills',NULL,'skills','5a79753785072c5ce6f0463fc9540f025301a25e','github.com-acme-skills',1793956490,1793956490);
INSERT INTO sources VALUES(2,'git','git@github.com:acme/skills.git',NULL,'skills','5a79753785072c5ce6f0463fc9540f025301a25e','github.com-acme-skills-2',1793956550,1793956550);
INSERT INTO sources VALUES(3,'git','https://github.com/acme/skills','main','skills','5a79753785072c5ce6f0463fc9540f025301a25e','github.com-acme-skills-3',1793956610,1793956610);
INSERT INTO sources VALUES(4,'git','https://x-access-token:<token>@github.com/acme/private',NULL,'skills','666abf2d70ce25ed286a9c65aa5fd323b1ac8612','github.com-acme-private',1793956670,1793956670);
INSERT INTO sources VALUES(5,'git','https://github.com/Acme/Private.git',NULL,'skills','666abf2d70ce25ed286a9c65aa5fd323b1ac8612','github.com-acme-private-2',1793956430,1793956430);
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
INSERT INTO artifacts VALUES(1,'skill','deploy',1,'.mindweaver/skills/github.com-acme-skills/deploy','Ship the service to staging','5a79753785072c5ce6f0463fc9540f025301a25e',1793956490);
INSERT INTO artifacts VALUES(2,'skill','review',1,'.mindweaver/skills/github.com-acme-skills/review','Review a change before it merges','5a79753785072c5ce6f0463fc9540f025301a25e',1793956490);
INSERT INTO artifacts VALUES(3,'skill','deploy',2,'.mindweaver/skills/github.com-acme-skills-2/deploy','Ship the service to staging','5a79753785072c5ce6f0463fc9540f025301a25e',1793956550);
INSERT INTO artifacts VALUES(4,'skill','review',2,'.mindweaver/skills/github.com-acme-skills-2/review','Review a change before it merges','5a79753785072c5ce6f0463fc9540f025301a25e',1793956550);
INSERT INTO artifacts VALUES(5,'skill','deploy',3,'.mindweaver/skills/github.com-acme-skills-3/deploy','Ship the service to staging','5a79753785072c5ce6f0463fc9540f025301a25e',1793956610);
INSERT INTO artifacts VALUES(6,'skill','review',3,'.mindweaver/skills/github.com-acme-skills-3/review','Review a change before it merges','5a79753785072c5ce6f0463fc9540f025301a25e',1793956610);
INSERT INTO artifacts VALUES(7,'skill','audit',4,'.mindweaver/skills/github.com-acme-private/audit','Audit what a change lets in','666abf2d70ce25ed286a9c65aa5fd323b1ac8612',1793956670);
INSERT INTO artifacts VALUES(8,'skill','audit',5,'.mindweaver/skills/github.com-acme-private-2/audit','Audit what a change lets in','666abf2d70ce25ed286a9c65aa5fd323b1ac8612',1793956430);
CREATE TABLE actions (
    id      INTEGER PRIMARY KEY,
    at      INTEGER NOT NULL,
    action  TEXT    NOT NULL,
    target  TEXT,
    outcome TEXT    NOT NULL,
    detail  TEXT
);
INSERT INTO actions VALUES(1,1793956430,'gather','https://github.com/Acme/Private.git','ok','1 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(2,1793956490,'gather','https://github.com/acme/skills','ok','2 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(3,1793956550,'gather','git@github.com:acme/skills.git','ok','2 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(4,1793956610,'gather','https://github.com/acme/skills','ok','2 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(5,1793956640,'gather','https://x-access-token:<token>@github.com/acme/private','failed','https://x-access-token:<token>@github.com/acme/private: cannot be fetched: the server hung up');
INSERT INTO actions VALUES(6,1793956670,'gather','https://x-access-token:<token>@github.com/acme/private','ok','1 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(7,1793956730,'install','collapse','ok','deploy into collapse');
INSERT INTO actions VALUES(8,1793956730,'install','tanukeys','ok','review into tanukeys');
INSERT INTO actions VALUES(9,1793956730,'install','tanukeys','ok','audit into tanukeys');
CREATE TABLE installations (
    id           INTEGER PRIMARY KEY,
    project      TEXT    NOT NULL,
    kind         TEXT    NOT NULL,
    name         TEXT    NOT NULL,
    source_id    INTEGER REFERENCES sources(id) ON DELETE SET NULL,
    installed_at INTEGER NOT NULL, origin TEXT, content_hash TEXT, project_id TEXT REFERENCES projects(id) ON UPDATE CASCADE ON DELETE SET NULL,
    UNIQUE (project, kind, name)
);
INSERT INTO installations VALUES(1,'collapse','skill','deploy',1,1793956730,'https://github.com/acme/skills','333a6f445f9f99532a56dc9fb631f77e1b14275a737d7011845d6cc7d814d731','02cd8c9526b4fbb84acc4acdf08f8dfd');
INSERT INTO installations VALUES(2,'tanukeys','skill','review',2,1793956730,'git@github.com:acme/skills.git','d662bf9802167f34cacc8b1cd4f603e2d032199618a338888e7c48f08f72099b','7895c3c48388718f7a6902c45db7c441');
INSERT INTO installations VALUES(3,'tanukeys','skill','audit',5,1793956730,'https://github.com/Acme/Private.git','e2fae6d6aca7d2a4741af02530e2218df629efe8fb49cc8b0b7431cabbba6cfa','7895c3c48388718f7a6902c45db7c441');
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
INSERT INTO projects VALUES('02cd8c9526b4fbb84acc4acdf08f8dfd','collapse');
INSERT INTO projects VALUES('7895c3c48388718f7a6902c45db7c441','tanukeys');
CREATE TABLE installation_entries (
    installation_id INTEGER NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
    agent           TEXT    NOT NULL,
    file            TEXT    NOT NULL,
    name            TEXT    NOT NULL,
    fingerprint     TEXT    NOT NULL,
    UNIQUE (installation_id, agent)
);
CREATE INDEX artifacts_by_name ON artifacts (kind, name);
CREATE INDEX actions_by_time ON actions (at);
CREATE INDEX installations_by_project ON installations (project);
CREATE INDEX installations_by_project_id ON installations (project_id);
COMMIT;
PRAGMA user_version = 6;
