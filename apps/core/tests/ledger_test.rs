//! The ledger: what an install records in it, and opening one an older
//! Mindweaver wrote: what it keeps, what it gains, and the copy of it left
//! behind.

mod common;

use std::path::{Path, PathBuf};

use common::{repository, skill, url};
use mindweaver_core::gather::{self, Request};
use mindweaver_core::hash;
use mindweaver_core::install::{self, Installed, Offer};
use mindweaver_core::ledger::{backup_path, InstallRecord, Target, SCHEMA_VERSION};
use mindweaver_core::mother::{InstallRecordMother, McpMother};
use mindweaver_core::AgentHomes;
use mindweaver_core::{Kind, Ledger, MindProject, ProjectId, WeaverWorkspace, LEDGER_FILE};
use rusqlite::types::Value;
use rusqlite::Connection;
use tempfile::TempDir;

/// Under one directory: `target`, a workspace linking `target/collapse`,
/// keeping `notes` as its own and with `deploy` on its shelf; and `team`, a
/// workspace `target` loads, keeping `lint` as its own.
fn three_places() -> (TempDir, WeaverWorkspace, MindProject, Ledger) {
    let dir = TempDir::new().unwrap();
    let (mut target, _) = WeaverWorkspace::init(dir.path().join("target")).unwrap();
    let (project, _) = MindProject::init(dir.path().join("target/collapse")).unwrap();
    target.link(&project).unwrap();
    mindweaver_core::create(&target.own(), Kind::Skill, "notes", "How we take notes").unwrap();
    let (team, _) = WeaverWorkspace::init(dir.path().join("team")).unwrap();
    mindweaver_core::create(&team.own(), Kind::Skill, "lint", "How we lint").unwrap();
    target.load(&team).unwrap();

    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let ledger = target.ledger().unwrap();
    gather::gather(&target, &ledger, &Request::git(url(&source))).unwrap();
    (dir, target, project, ledger)
}

/// Install the one thing called `name` that is offered to `project`.
fn install(workspace: &WeaverWorkspace, ledger: &Ledger, project: &MindProject, name: &str) {
    let candidate = install::offered(workspace, ledger, project, Kind::Skill, name, None).unwrap();
    install::install(
        workspace,
        ledger,
        project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();
}

#[test]
fn an_install_records_one_copy_at_the_folder_the_project_names() {
    let (_dir, workspace, project, ledger) = three_places();

    install(&workspace, &ledger, &project, "deploy");

    let recorded = ledger
        .installation("collapse", Kind::Skill, "deploy")
        .unwrap()
        .unwrap();
    assert_eq!(
        recorded.targets,
        [Target {
            agent: None,
            path: String::from("skills/deploy"),
            content_hash: None,
        }]
    );
}

/// What schema 2 wrote, dumped from a ledger it made.
const VERSION_2: &str = include_str!("fixtures/ledger-v2.sql");

/// What schema 3 wrote, dumped from a ledger it made.
const VERSION_3: &str = include_str!("fixtures/ledger-v3.sql");

/// What schema 4 wrote, dumped from a ledger it made.
const VERSION_4: &str = include_str!("fixtures/ledger-v4.sql");

/// What schema 5 wrote, dumped from a ledger it made.
const VERSION_5: &str = include_str!("fixtures/ledger-v5.sql");

/// Every column of `sources` a version before 7 had, which schema 7 adds
/// to: what a step after it keeps of each row it is not about.
const SOURCES_AS_WRITTEN: &str = "SELECT id, kind, url, reference, subdirectory, revision,
                                         directory, first_seen, last_seen
                                    FROM sources ORDER BY id";

/// A ledger file in `dir` holding what `fixture` describes, untouched by
/// anything of this build's.
fn written_at(dir: &Path, fixture: &str) -> PathBuf {
    let path = dir.join(LEDGER_FILE);
    Connection::open(&path)
        .unwrap()
        .execute_batch(fixture)
        .unwrap();
    path
}

/// The same, in memory: what the rows were, to compare with what they are.
fn as_written(fixture: &str) -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch(fixture).unwrap();
    connection
}

fn rows(connection: &Connection, query: &str) -> Vec<Vec<Value>> {
    let mut statement = connection.prepare(query).unwrap();
    let width = statement.column_count();
    statement
        .query_map([], |row| {
            (0..width)
                .map(|column| row.get::<_, Value>(column))
                .collect()
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn version_of(connection: &Connection) -> u32 {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}

fn text(value: &str) -> Value {
    Value::Text(value.to_owned())
}

#[test]
fn an_install_records_where_it_came_from_as_from_takes_it() {
    let (_dir, workspace, project, ledger) = three_places();
    let shelf = ledger.gathered().unwrap()[0].source.url.clone();

    for name in ["deploy", "notes", "lint"] {
        install(&workspace, &ledger, &project, name);
    }

    let recorded = |name| {
        ledger
            .installation("collapse", Kind::Skill, name)
            .unwrap()
            .unwrap()
    };
    assert_eq!(recorded("deploy").origin.as_deref(), Some(shelf.as_str()));
    assert_eq!(
        recorded("notes").origin.as_deref(),
        Some(install::WORKSPACE_ORIGIN)
    );
    assert_eq!(recorded("lint").origin.as_deref(), Some("load:../team"));
    // Only what was gathered has a source to point at as well.
    assert!(recorded("deploy").source_id.is_some());
    assert_eq!(recorded("notes").source_id, None);
    assert_eq!(recorded("lint").source_id, None);
    // And each names, to `--from`, the offer it was installed from.
    for name in ["deploy", "notes", "lint"] {
        let origin = recorded(name).origin.unwrap();
        let offered = install::offered(
            &workspace,
            &ledger,
            &project,
            Kind::Skill,
            name,
            Some(&origin),
        )
        .unwrap();
        assert_eq!(offered.origin(), origin);
    }
}

#[test]
fn an_install_records_the_hash_of_the_copy_it_wrote() {
    let (_dir, workspace, project, ledger) = three_places();

    install(&workspace, &ledger, &project, "deploy");

    let copy = project.directory_for(Kind::Skill).unwrap().join("deploy");
    let shelf = workspace.root().join(&ledger.gathered().unwrap()[0].path);
    let recorded = ledger
        .installation("collapse", Kind::Skill, "deploy")
        .unwrap()
        .unwrap();
    assert_eq!(recorded.content_hash, Some(hash::tree(&copy).unwrap()));
    // Which is the offer's too, the folder it sits in aside.
    assert_eq!(recorded.content_hash, Some(hash::tree(&shelf).unwrap()));
}

#[test]
fn installing_again_records_the_hash_of_whatever_is_there_now() {
    let (_dir, workspace, project, ledger) = three_places();
    install(&workspace, &ledger, &project, "notes");
    let hash_of = || {
        ledger
            .installation("collapse", Kind::Skill, "notes")
            .unwrap()
            .unwrap()
            .content_hash
            .unwrap()
    };
    let first = hash_of();

    install(&workspace, &ledger, &project, "notes");
    assert_eq!(hash_of(), first, "an install that changed nothing");

    let own = workspace
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("notes")
        .join("SKILL.md");
    let edited = std::fs::read_to_string(&own).unwrap() + "\nAnd one more thing.\n";
    std::fs::write(&own, edited).unwrap();
    install(&workspace, &ledger, &project, "notes");

    let copy = project.directory_for(Kind::Skill).unwrap().join("notes");
    assert_ne!(hash_of(), first);
    assert_eq!(hash_of(), hash::tree(&copy).unwrap());
}

#[cfg(unix)]
#[test]
fn a_forced_install_writes_over_a_copy_holding_a_link_and_records_its_hash() {
    let (dir, workspace, project, ledger) = three_places();
    install(&workspace, &ledger, &project, "notes");
    let hashed = ledger
        .installation("collapse", Kind::Skill, "notes")
        .unwrap()
        .unwrap()
        .content_hash;
    // A file in the copy swapped for a link to the same bytes, which
    // `copy::same` follows and the hash refuses.
    let copy = project.directory_for(Kind::Skill).unwrap().join("notes");
    let file = copy.join("SKILL.md");
    let elsewhere = dir.path().join("SKILL.md");
    std::fs::rename(&file, &elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &file).unwrap();

    let candidate =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "notes", None).unwrap();
    // No install writes a link, so one there is an edit, refused unless
    // the install is forced.
    let refused = install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap()
    .installed;
    assert!(matches!(refused, Installed::Edited { .. }), "{refused:?}");
    let done = install::install_with(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
        install::Edits::Forced,
    )
    .unwrap()
    .installed;

    // Written over, though it held the same bytes: left as it was, it
    // would be recorded with no hash and read as edited still.
    assert!(matches!(done, Installed::Updated { .. }), "{done:?}");
    assert_eq!(done.unhashed(), None);
    assert!(!std::fs::symlink_metadata(&file)
        .unwrap()
        .file_type()
        .is_symlink());
    let recorded = ledger
        .installation("collapse", Kind::Skill, "notes")
        .unwrap()
        .unwrap();
    assert_eq!(recorded.content_hash, hashed);
    assert_eq!(recorded.content_hash, Some(hash::tree(&copy).unwrap()));
    assert_eq!(
        install::standings(&workspace, &ledger, &project, Kind::Skill).unwrap(),
        [(String::from("notes"), install::Standing::Current)]
    );
}

#[test]
fn a_version_2_ledger_keeps_every_row_it_had() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_2);
    let before = as_written(VERSION_2);

    let ledger = Ledger::open(&path).unwrap();

    let after = Connection::open(&path).unwrap();
    assert_eq!(version_of(&after), SCHEMA_VERSION);
    for query in [
        SOURCES_AS_WRITTEN,
        "SELECT * FROM artifacts ORDER BY id",
        "SELECT * FROM actions ORDER BY id",
        "SELECT id, project, kind, name, source_id, installed_at
           FROM installations ORDER BY id",
    ] {
        let kept = rows(&after, query);
        assert_eq!(kept, rows(&before, query), "{query}");
        assert!(!kept.is_empty(), "{query} reads nothing to compare");
    }
    assert_eq!(ledger.history(100).unwrap().len(), 11);
    assert_eq!(
        ledger.installations("collapse", Kind::Skill).unwrap(),
        ["commit-style", "deploy", "lint", "notes"]
    );
    assert_eq!(
        ledger.installations("tanukeys", Kind::Skill).unwrap(),
        ["notes", "review"]
    );
}

#[test]
fn a_version_2_installation_becomes_one_target_at_the_path_it_had() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_2);
    let before = as_written(VERSION_2);

    Ledger::open(&path).unwrap();

    let after = Connection::open(&path).unwrap();
    assert_eq!(
        rows(
            &after,
            "SELECT installation_id, agent, path FROM installation_targets
              ORDER BY installation_id"
        ),
        rows(
            &before,
            "SELECT id, NULL, path FROM installations ORDER BY id"
        )
    );
}

#[test]
fn a_version_2_installation_from_a_source_gets_the_origin_its_offer_has() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_2);

    let ledger = Ledger::open(&path).unwrap();

    let after = Connection::open(&path).unwrap();
    let skills = text("https://github.com/acme/skills#next");
    let tools = text("https://github.com/acme/tools");
    assert_eq!(
        rows(
            &after,
            "SELECT name, origin, content_hash FROM installations ORDER BY id"
        ),
        [
            [text("deploy"), skills.clone(), Value::Null],
            [text("commit-style"), tools, Value::Null],
            // From the workspace and from a load: version 2 kept nothing
            // that says which.
            [text("notes"), Value::Null, Value::Null],
            [text("lint"), Value::Null, Value::Null],
            [text("review"), skills, Value::Null],
            [text("notes"), Value::Null, Value::Null],
        ]
    );
    // The spelling an install from the shelf records now, so a record the
    // step filled in and one written since compare alike.
    for gathered in ledger.gathered().unwrap() {
        let origin = Offer::Shelf(gathered.clone()).origin();
        let recorded: Vec<String> = after
            .prepare("SELECT DISTINCT origin FROM installations WHERE source_id = ?1")
            .unwrap()
            .query_map([gathered.source.id], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(
            recorded.iter().all(|recorded| *recorded == origin),
            "{recorded:?} is not {origin}"
        );
    }
}

#[test]
fn a_version_2_installation_reads_back_through_the_ledger() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_2);

    let ledger = Ledger::open(&path).unwrap();

    assert_eq!(
        ledger
            .installation("collapse", Kind::Skill, "deploy")
            .unwrap(),
        Some(InstallRecord {
            kind: Kind::Skill,
            name: "deploy".into(),
            source_id: Some(1),
            origin: Some(mindweaver_core::value::Origin::from(
                "https://github.com/acme/skills#next"
            )),
            content_hash: None,
            targets: vec![Target {
                agent: None,
                path: String::from("skills/deploy"),
                content_hash: None,
            }],
            entries: Vec::new(),
        })
    );
}

#[test]
fn a_version_2_ledger_is_copied_aside_and_the_copy_opens_at_version_2() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_2);
    let before = as_written(VERSION_2);

    Ledger::open(&path).unwrap();

    let backup = dir.path().join("mindweaver.db.v2.bak");
    assert_eq!(backup_path(&path, 2), backup);
    // Opened raw: opening it as a ledger would migrate the copy as well.
    let copy = Connection::open(&backup).unwrap();
    assert_eq!(version_of(&copy), 2);
    for query in [
        "SELECT * FROM sources ORDER BY id",
        "SELECT * FROM artifacts ORDER BY id",
        "SELECT * FROM actions ORDER BY id",
        "SELECT * FROM installations ORDER BY id",
        "SELECT type, name, sql FROM sqlite_master ORDER BY name",
    ] {
        assert_eq!(rows(&copy, query), rows(&before, query), "{query}");
    }
}

#[test]
fn reading_a_version_2_ledger_migrates_it_through_the_same_copy() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    written_at(&dir.path().join(".mindweaver"), VERSION_2);

    let ledger = workspace.ledger_for_reading().unwrap();

    assert_eq!(ledger.path(), workspace.ledger_path());
    assert_eq!(ledger.history(100).unwrap().len(), 11);
    let copy = Connection::open(backup_path(&workspace.ledger_path(), 2)).unwrap();
    assert_eq!(version_of(&copy), 2);
}

#[test]
fn a_version_3_ledger_keeps_every_row_it_had_and_its_copies_gain_no_hash() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_3);
    let before = as_written(VERSION_3);

    let ledger = Ledger::open(&path).unwrap();

    let after = Connection::open(&path).unwrap();
    assert_eq!(version_of(&after), SCHEMA_VERSION);
    for query in [
        SOURCES_AS_WRITTEN,
        "SELECT * FROM artifacts ORDER BY id",
        "SELECT * FROM actions ORDER BY id",
        "SELECT id, project, kind, name, source_id, installed_at, origin, content_hash
           FROM installations ORDER BY id",
        "SELECT installation_id, agent, path FROM installation_targets
          ORDER BY installation_id, path",
    ] {
        let kept = rows(&after, query);
        assert_eq!(kept, rows(&before, query), "{query}");
        assert!(!kept.is_empty(), "{query} reads nothing to compare");
    }
    // Every copy it recorded is a skill's, holding the tree its
    // installation's hash names.
    assert!(
        rows(&after, "SELECT content_hash FROM installation_targets")
            .iter()
            .all(|row| row == &[Value::Null])
    );
    assert!(rows(&after, "SELECT * FROM created_files").is_empty());
    let notes = ledger
        .installation("tanukeys", Kind::Skill, "notes")
        .unwrap()
        .unwrap();
    assert_eq!(
        notes.targets,
        [
            Target {
                agent: None,
                path: String::from(".agents/skills/notes"),
                content_hash: None,
            },
            Target {
                agent: Some(String::from("claude")),
                path: String::from(".claude/skills/notes"),
                content_hash: None,
            },
        ]
    );
    assert!(notes.content_hash.is_some());
}

#[test]
fn a_version_3_ledger_is_copied_aside_and_the_copy_opens_at_version_3() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_3);
    let before = as_written(VERSION_3);

    Ledger::open(&path).unwrap();

    let copy = Connection::open(backup_path(&path, 3)).unwrap();
    assert_eq!(version_of(&copy), 3);
    for query in [
        "SELECT * FROM sources ORDER BY id",
        "SELECT * FROM installations ORDER BY id",
        "SELECT * FROM installation_targets ORDER BY installation_id, path",
        "SELECT type, name, sql FROM sqlite_master ORDER BY name",
    ] {
        assert_eq!(rows(&copy, query), rows(&before, query), "{query}");
    }
}

/// The id `tanukeys` holds in the schema 4 fixture: what its settings said
/// when that build made it.
const TANUKEYS: &str = "376e550a51f67281fb714f604283f7a0";

#[test]
fn a_version_4_ledger_keeps_every_row_it_had_and_knows_no_project_it_is_not_told_of() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_4);
    let before = as_written(VERSION_4);

    let ledger = Ledger::open(&path).unwrap();

    let after = Connection::open(&path).unwrap();
    assert_eq!(version_of(&after), SCHEMA_VERSION);
    for query in [
        SOURCES_AS_WRITTEN,
        "SELECT * FROM artifacts ORDER BY id",
        "SELECT * FROM actions ORDER BY id",
        "SELECT id, project, kind, name, source_id, installed_at, origin, content_hash
           FROM installations ORDER BY id",
        "SELECT * FROM installation_targets ORDER BY installation_id, path",
        "SELECT * FROM created_files ORDER BY project, path",
    ] {
        let kept = rows(&after, query);
        assert_eq!(kept, rows(&before, query), "{query}");
        assert!(!kept.is_empty(), "{query} reads nothing to compare");
    }
    // The file kept no id, and nothing said one: every record keeps its
    // route alone.
    assert!(rows(&after, "SELECT * FROM projects").is_empty());
    assert!(rows(&after, "SELECT project_id FROM installations")
        .iter()
        .all(|row| row == &[Value::Null]));
    assert_eq!(ledger.project_at("tanukeys").unwrap(), None);
}

#[test]
fn a_version_4_ledger_names_each_record_by_the_id_its_linked_project_holds() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_4);
    let tanukeys = ProjectId::parse(TANUKEYS).unwrap();

    // What a workspace hands in: `tanukeys`, marked by its settings, and
    // not `collapse`, which a `mind.toml` marks and has no id.
    let ledger =
        Ledger::open_knowing(&path, || vec![(tanukeys.clone(), String::from("tanukeys"))]).unwrap();

    assert_eq!(ledger.project_at("tanukeys").unwrap(), Some(tanukeys));
    let after = Connection::open(&path).unwrap();
    assert_eq!(
        rows(
            &after,
            "SELECT project, name, project_id FROM installations ORDER BY id"
        ),
        [
            [text("collapse"), text("deploy"), Value::Null],
            [text("collapse"), text("notes"), Value::Null],
            [text("collapse"), text("lint"), Value::Null],
            [text("tanukeys"), text("review"), text(TANUKEYS)],
            [text("tanukeys"), text("notes"), text(TANUKEYS)],
            [text("tanukeys"), text("style"), text(TANUKEYS)],
        ]
    );
}

#[test]
fn a_version_4_ledger_is_copied_aside_and_the_copy_opens_at_version_4() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_4);
    let before = as_written(VERSION_4);

    Ledger::open(&path).unwrap();

    let backup = dir.path().join("mindweaver.db.v4.bak");
    assert_eq!(backup_path(&path, 4), backup);
    let copy = Connection::open(&backup).unwrap();
    assert_eq!(version_of(&copy), 4);
    for query in [
        "SELECT * FROM sources ORDER BY id",
        "SELECT * FROM installations ORDER BY id",
        "SELECT * FROM installation_targets ORDER BY installation_id, path",
        "SELECT * FROM created_files ORDER BY project, path",
        "SELECT type, name, sql FROM sqlite_master ORDER BY name",
    ] {
        assert_eq!(rows(&copy, query), rows(&before, query), "{query}");
    }
}

#[test]
fn a_version_5_ledger_keeps_every_row_it_had_and_its_installations_gain_no_entry() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_5);
    let before = as_written(VERSION_5);

    let ledger = Ledger::open(&path).unwrap();

    let after = Connection::open(&path).unwrap();
    assert_eq!(version_of(&after), SCHEMA_VERSION);
    for query in [
        SOURCES_AS_WRITTEN,
        "SELECT * FROM artifacts ORDER BY id",
        "SELECT * FROM actions ORDER BY id",
        "SELECT * FROM installations ORDER BY id",
        "SELECT * FROM installation_targets ORDER BY installation_id, path",
        "SELECT * FROM created_files ORDER BY project, path",
        "SELECT * FROM projects ORDER BY id",
    ] {
        let kept = rows(&after, query);
        assert_eq!(kept, rows(&before, query), "{query}");
        assert!(!kept.is_empty(), "{query} reads nothing to compare");
    }
    // No build of schema 5 installed a server, so nothing is filled in.
    assert!(rows(&after, "SELECT * FROM installation_entries").is_empty());
    let review = ledger
        .installation("tanukeys", Kind::Skill, "review")
        .unwrap()
        .unwrap();
    assert_eq!(review.targets.len(), 2);
    assert!(review.entries.is_empty());
}

#[test]
fn a_version_5_ledger_records_a_servers_entries_once_migrated() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_5);
    let name = McpMother::name();
    let server = InstallRecordMother::mcp(&name);

    let ledger = Ledger::open(&path).unwrap();
    ledger.installed("tanukeys", &server).unwrap();
    drop(ledger);

    let read = Ledger::open(&path)
        .unwrap()
        .installation("tanukeys", Kind::Mcp, &name)
        .unwrap();
    assert_eq!(read, Some(server));
    // Under the id the schema 5 ledger knew the project by.
    let after = Connection::open(&path).unwrap();
    assert_eq!(
        rows(
            &after,
            "SELECT project_id FROM installations WHERE kind = 'mcp'"
        ),
        [[text("c00c03a1f3c6676732405a8bf90a2d48")]]
    );
}

#[test]
fn a_version_5_ledger_is_copied_aside_and_the_copy_opens_at_version_5() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_5);
    let before = as_written(VERSION_5);

    Ledger::open(&path).unwrap();

    let backup = dir.path().join("mindweaver.db.v5.bak");
    assert_eq!(backup_path(&path, 5), backup);
    let copy = Connection::open(&backup).unwrap();
    assert_eq!(version_of(&copy), 5);
    for query in [
        "SELECT * FROM sources ORDER BY id",
        "SELECT * FROM installations ORDER BY id",
        "SELECT * FROM installation_targets ORDER BY installation_id, path",
        "SELECT * FROM created_files ORDER BY project, path",
        "SELECT * FROM projects ORDER BY id",
        "SELECT type, name, sql FROM sqlite_master ORDER BY name",
    ] {
        assert_eq!(rows(&copy, query), rows(&before, query), "{query}");
    }
}

/// What schema 6 wrote, dumped from a ledger it made, with what an older
/// build left beside it: a token as `<token>`, filled in by [`version_6`].
const VERSION_6: &str = include_str!("fixtures/ledger-v6.sql");

/// A token as GitHub hands one out, made as the test runs rather than
/// written down, so that nothing in the repository looks like a secret.
fn token() -> String {
    format!("ghp_{}", "T0k3n".repeat(7) + "x")
}

/// [`VERSION_6`] with `token` where an older build stored one.
fn version_6(token: &str) -> String {
    VERSION_6.replace("<token>", token)
}

#[test]
fn a_version_6_ledger_keeps_every_row_but_those_of_a_spelling_made_one_with_another() {
    let token = token();
    let fixture = version_6(&token);
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), &fixture);
    let before = as_written(&fixture);

    let ledger = Ledger::open(&path).unwrap();

    let after = Connection::open(&path).unwrap();
    assert_eq!(version_of(&after), SCHEMA_VERSION);
    // The spelling of the first repository gathered last and the one of
    // the second gathered with the token are kept, as is the branch, which
    // is a place of its own; what the others gathered went with them.
    for (now, then) in [
        (
            SOURCES_AS_WRITTEN,
            "SELECT id, kind, url, reference, subdirectory, revision, directory, first_seen,
                    last_seen
               FROM sources WHERE id IN (2, 3, 4) ORDER BY id",
        ),
        (
            "SELECT * FROM artifacts ORDER BY id",
            "SELECT * FROM artifacts WHERE source_id IN (2, 3, 4) ORDER BY id",
        ),
        (
            "SELECT id, project, kind, name, installed_at, content_hash, project_id
               FROM installations ORDER BY id",
            "SELECT id, project, kind, name, installed_at, content_hash, project_id
               FROM installations ORDER BY id",
        ),
        (
            "SELECT * FROM installation_targets ORDER BY installation_id, path",
            "SELECT * FROM installation_targets ORDER BY installation_id, path",
        ),
        (
            "SELECT * FROM projects ORDER BY id",
            "SELECT * FROM projects ORDER BY id",
        ),
        // The scrub is what takes a credential out, which a command that
        // writes runs next.
        (
            "SELECT * FROM actions ORDER BY id",
            "SELECT * FROM actions ORDER BY id",
        ),
    ] {
        let kept = rows(&after, now);
        assert_eq!(kept, rows(&before, then), "{now}");
        assert!(!kept.is_empty(), "{now} reads nothing to compare");
    }
    // An installation from a spelling that went is from the one kept, by
    // the origin that one is installed from, without the credential the
    // kept one still holds, which it never held.
    let skills = String::from("git@github.com:acme/skills.git");
    let private = String::from("https://github.com/acme/private");
    assert_eq!(
        rows(
            &after,
            "SELECT id, source_id, origin FROM installations ORDER BY id"
        ),
        [
            [Value::Integer(1), Value::Integer(2), text(&skills)],
            [Value::Integer(2), Value::Integer(2), text(&skills)],
            [Value::Integer(3), Value::Integer(4), text(&private)],
        ]
    );
    let merged = ledger.merged().unwrap();
    let merged: Vec<(&str, &str, i64)> = merged
        .iter()
        .map(|merged| {
            (
                merged.url.as_str(),
                merged.directory.as_str(),
                merged.into.id,
            )
        })
        .collect();
    assert_eq!(
        merged,
        [
            (
                "https://github.com/Acme/Private.git",
                "github.com-acme-private-2",
                4
            ),
            (
                "https://github.com/acme/skills",
                "github.com-acme-skills",
                2
            ),
        ]
    );
}

#[test]
fn a_version_6_ledger_names_each_source_and_finds_it_by_any_spelling_of_its_url() {
    let token = token();
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), &version_6(&token));

    let ledger = Ledger::open(&path).unwrap();

    let named: Vec<(i64, String, String)> = ledger
        .sources()
        .unwrap()
        .into_iter()
        .map(|source| {
            (
                source.id,
                source.name.to_string(),
                source.origin().into_string(),
            )
        })
        .collect();
    assert_eq!(
        named,
        [
            (
                2,
                String::from("acme-skills"),
                String::from("git@github.com:acme/skills.git")
            ),
            (
                4,
                String::from("acme-private"),
                String::from("https://github.com/acme/private")
            ),
            (
                3,
                String::from("acme-skills-2"),
                String::from("https://github.com/acme/skills#main")
            ),
        ]
    );
    for (spelling, reference, id) in [
        ("https://github.com/acme/skills.git", None, 2),
        ("acme/skills", None, 2),
        ("ssh://git@github.com/Acme/Skills", Some("main"), 3),
        ("git@github.com:acme/private.git", None, 4),
    ] {
        let found = ledger
            .source(
                mindweaver_core::SourceKind::Git,
                spelling,
                reference,
                "skills",
            )
            .unwrap();
        assert_eq!(found.map(|source| source.id), Some(id), "{spelling}");
    }
    let identities = rows(
        &Connection::open(&path).unwrap(),
        "SELECT canonical_url, reference_key, name FROM sources ORDER BY id",
    );
    assert!(
        !format!("{identities:?}").contains(&token),
        "{identities:?}"
    );
}

#[test]
fn a_version_6_ledger_is_copied_aside_and_the_copy_opens_at_version_6() {
    let fixture = version_6(&token());
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), &fixture);
    let before = as_written(&fixture);

    Ledger::open(&path).unwrap();

    let backup = dir.path().join("mindweaver.db.v6.bak");
    assert_eq!(backup_path(&path, 6), backup);
    let copy = Connection::open(&backup).unwrap();
    assert_eq!(version_of(&copy), 6);
    for query in [
        "SELECT * FROM sources ORDER BY id",
        "SELECT * FROM artifacts ORDER BY id",
        "SELECT * FROM actions ORDER BY id",
        "SELECT * FROM installations ORDER BY id",
        "SELECT type, name, sql FROM sqlite_master ORDER BY name",
    ] {
        assert_eq!(rows(&copy, query), rows(&before, query), "{query}");
    }
}

/// The folders each source of [`VERSION_6`] had in the workspace at
/// `workspace`: a clone, made from its URL as the fixture spells it, and a
/// shelf of what it supplied.
fn folders_of_version_6(workspace: &WeaverWorkspace, token: &str) {
    for (directory, url, skill_name) in [
        (
            "github.com-acme-skills",
            "https://github.com/acme/skills",
            "deploy",
        ),
        (
            "github.com-acme-skills-2",
            "git@github.com:acme/skills.git",
            "deploy",
        ),
        (
            "github.com-acme-skills-3",
            "https://github.com/acme/skills",
            "deploy",
        ),
        (
            "github.com-acme-private",
            &format!("https://x-access-token:{token}@github.com/acme/private"),
            "audit",
        ),
        (
            "github.com-acme-private-2",
            "https://github.com/Acme/Private.git",
            "audit",
        ),
    ] {
        let git = workspace.cache_dir().join(directory).join(".git");
        std::fs::create_dir_all(git.join("logs")).unwrap();
        std::fs::write(
            git.join("config"),
            format!("[remote \"origin\"]\n\turl = {url}\n"),
        )
        .unwrap();
        std::fs::write(
            git.join("logs").join("HEAD"),
            format!("0000 1111 Fixture <fixture@example.com> 0 +0000\tclone: from {url}\n"),
        )
        .unwrap();
        let shelf = workspace
            .gathered_dir(Kind::Skill)
            .join(directory)
            .join(skill_name);
        std::fs::create_dir_all(&shelf).unwrap();
        std::fs::write(shelf.join("SKILL.md"), skill(skill_name, "Gathered")).unwrap();
    }
}

/// Everything under `root` that reads as text, joined.
fn text_under(root: &Path) -> String {
    let mut text = String::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            text.push_str(&text_under(&path));
        } else if let Ok(contents) = std::fs::read_to_string(&path) {
            text.push_str(&contents);
        }
    }
    text
}

#[test]
fn opening_a_version_6_ledger_to_write_leaves_no_credential_and_nothing_a_merge_left() {
    let token = token();
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    written_at(&dir.path().join(".mindweaver"), &version_6(&token));
    folders_of_version_6(&workspace, &token);

    let (ledger, scrub) = workspace.ledger_to_write().unwrap();

    // Each merge is said, by what it was gathered as and what it is now.
    let merged: Vec<(&str, &str)> = scrub
        .merged
        .iter()
        .map(|merged| (merged.url.as_str(), merged.into.name.as_str()))
        .collect();
    assert_eq!(
        merged,
        [
            ("https://github.com/Acme/Private.git", "acme-private"),
            ("https://github.com/acme/skills", "acme-skills"),
        ]
    );
    assert!(scrub.leftovers.is_empty(), "{:?}", scrub.leftovers);
    // And so is the credential the scrub took out, once.
    let told: Vec<&str> = scrub
        .sources
        .iter()
        .map(|source| source.url.as_str())
        .collect();
    assert_eq!(told, ["https://github.com/acme/private"]);
    assert_eq!(scrub.actions, 2);
    assert!(scrub.failures.is_empty(), "{:?}", scrub.failures);
    // The folders of a spelling that went go with it; the others stay.
    for (directory, kept) in [
        ("github.com-acme-skills", false),
        ("github.com-acme-skills-2", true),
        ("github.com-acme-skills-3", true),
        ("github.com-acme-private", true),
        ("github.com-acme-private-2", false),
    ] {
        for folder in [
            workspace.cache_dir().join(directory),
            workspace.gathered_dir(Kind::Skill).join(directory),
        ] {
            assert_eq!(folder.exists(), kept, "{folder:?}");
        }
    }
    let audit = ledger
        .installation("tanukeys", Kind::Skill, "audit")
        .unwrap()
        .unwrap();
    assert_eq!(
        audit.origin.as_deref(),
        Some("https://github.com/acme/private")
    );
    drop(ledger);

    // No credential in the ledger, its log, any clone or the shelf.
    let ledger_dir = dir.path().join(".mindweaver");
    for entry in std::fs::read_dir(&ledger_dir).unwrap() {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(LEDGER_FILE)
            && path.extension().is_none_or(|extension| extension != "bak")
        {
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                !bytes
                    .windows(token.len())
                    .any(|bytes| bytes == token.as_bytes()),
                "{path:?}"
            );
        }
    }
    let cached =
        text_under(&workspace.cache_dir()) + &text_under(&workspace.gathered_dir(Kind::Skill));
    assert!(!cached.contains(&token));
    // The copy is the way back, the file as it was.
    let copy = Connection::open(backup_path(&workspace.ledger_path(), 6)).unwrap();
    assert_eq!(version_of(&copy), 6);
    assert_eq!(
        rows(&copy, "SELECT count(*) FROM sources"),
        [[Value::Integer(5)]]
    );

    // Said once: the second time there is nothing left to say.
    assert!(workspace.ledger_to_write().unwrap().1.is_empty());
}

#[test]
fn reading_a_version_6_ledger_leaves_what_a_merge_left_for_the_first_command_that_writes_to_say() {
    let token = token();
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    written_at(&dir.path().join(".mindweaver"), &version_6(&token));
    folders_of_version_6(&workspace, &token);

    // Read twice, as a `status` and a `list` after an upgrade would.
    for _ in 0..2 {
        let ledger = workspace.ledger_for_reading().unwrap();
        assert_eq!(ledger.merged().unwrap().len(), 2);
    }
    // A read removes no folder and scrubs nothing.
    for folder in [
        workspace.cache_dir().join("github.com-acme-skills"),
        workspace
            .gathered_dir(Kind::Skill)
            .join("github.com-acme-private-2"),
    ] {
        assert!(folder.exists(), "{folder:?}");
    }

    // The next command that writes says each merge, clears its folders and
    // scrubs the rest, whichever command took the step.
    let (_ledger, scrub) = workspace.ledger_to_write().unwrap();
    let merged: Vec<&str> = scrub
        .merged
        .iter()
        .map(|merged| merged.directory.as_str())
        .collect();
    assert_eq!(
        merged,
        ["github.com-acme-private-2", "github.com-acme-skills"]
    );
    assert!(scrub.leftovers.is_empty(), "{:?}", scrub.leftovers);
    assert_eq!(scrub.sources.len(), 1, "{:?}", scrub.sources);
    for directory in ["github.com-acme-skills", "github.com-acme-private-2"] {
        assert!(!workspace.cache_dir().join(directory).exists());
        assert!(!workspace.gathered_dir(Kind::Skill).join(directory).exists());
    }
    assert!(!text_under(&workspace.cache_dir()).contains(&token));
    assert!(workspace.ledger_to_write().unwrap().1.is_empty());
}

/// A folder of a merge that will not go is said by every command that
/// writes, with its merge, and tried again, until it goes.
#[cfg(unix)]
#[test]
fn a_folder_a_merge_left_that_will_not_go_is_tried_and_said_again_until_it_goes() {
    use std::os::unix::fs::PermissionsExt;

    let token = token();
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    written_at(&dir.path().join(".mindweaver"), &version_6(&token));
    folders_of_version_6(&workspace, &token);
    let stuck = workspace
        .cache_dir()
        .join("github.com-acme-skills")
        .join(".git");
    let mode = |mode| std::fs::set_permissions(&stuck, std::fs::Permissions::from_mode(mode));
    mode(0o555).unwrap();

    let (_ledger, first) = workspace.ledger_to_write().unwrap();
    let (_ledger, second) = workspace.ledger_to_write().unwrap();
    mode(0o755).unwrap();
    let (_ledger, third) = workspace.ledger_to_write().unwrap();

    for scrub in [&first, &second] {
        let said: Vec<&str> = scrub
            .merged
            .iter()
            .map(|merged| merged.directory.as_str())
            .collect();
        assert!(said.contains(&"github.com-acme-skills"), "{said:?}");
        assert_eq!(scrub.leftovers.len(), 1, "{:?}", scrub.leftovers);
        assert!(scrub.leftovers[0].path.ends_with("github.com-acme-skills"));
    }
    // The other merge, whose folders went, is said once.
    assert_eq!(second.merged.len(), 1, "{:?}", second.merged);
    assert_eq!(third.merged.len(), 1, "{:?}", third.merged);
    assert!(third.leftovers.is_empty(), "{:?}", third.leftovers);
    assert!(!workspace
        .cache_dir()
        .join("github.com-acme-skills")
        .exists());
    assert!(workspace.ledger_to_write().unwrap().1.is_empty());
}

#[test]
fn a_ledger_of_a_schema_after_this_builds_is_refused_and_left_as_it_is() {
    let dir = TempDir::new().unwrap();
    let path = written_at(dir.path(), VERSION_5);
    Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .unwrap();

    let error = Ledger::open(&path).unwrap_err();

    assert_eq!(error.code(), "ledger_too_new");
    assert_eq!(
        version_of(&Connection::open(&path).unwrap()),
        SCHEMA_VERSION + 1
    );
    assert!(rows(
        &Connection::open(&path).unwrap(),
        "SELECT name FROM sqlite_master WHERE name = 'installation_entries'"
    )
    .is_empty());
    assert!(!backup_path(&path, SCHEMA_VERSION + 1).exists());
    assert!(!backup_path(&path, 5).exists());
}

/// A workspace linking `names`, each a project marked by its settings under
/// the workspace, shared so no repository is needed.
fn linking(names: &[&str]) -> (TempDir, WeaverWorkspace, Vec<MindProject>) {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let mut projects = Vec::new();
    for name in names {
        let (project, _) = MindProject::init_settings(
            dir.path().join(name),
            &mindweaver_core::Agent::ALL,
            mindweaver_core::Share::Shared,
            &mindweaver_core::Directories::default(),
        )
        .unwrap();
        workspace.link(&project).unwrap();
        projects.push(project);
    }
    (dir, workspace, projects)
}

fn id_of(project: &MindProject) -> ProjectId {
    project.settings().unwrap().id.clone()
}

#[test]
fn a_ledger_made_for_a_workspace_knows_each_project_it_links_by_its_id() {
    let (dir, mut workspace, projects) = linking(&["collapse", "tanukeys"]);
    let (legacy, _) = MindProject::init(dir.path().join("legacy")).unwrap();
    workspace.link(&legacy).unwrap();

    let ledger = workspace.ledger().unwrap();

    assert_eq!(
        ledger.known_projects().unwrap(),
        [
            (id_of(&projects[0]), String::from("collapse")),
            (id_of(&projects[1]), String::from("tanukeys")),
        ]
    );
}

#[test]
fn reading_a_version_4_ledger_names_its_records_by_the_ids_of_the_projects_linked() {
    let (dir, workspace, projects) = linking(&["tanukeys"]);
    written_at(&dir.path().join(".mindweaver"), VERSION_4);

    let ledger = workspace.ledger_for_reading().unwrap();

    let tanukeys = id_of(&projects[0]);
    assert_eq!(
        ledger.project_at("tanukeys").unwrap(),
        Some(tanukeys.clone())
    );
    let after = Connection::open(workspace.ledger_path()).unwrap();
    assert_eq!(
        rows(
            &after,
            "SELECT DISTINCT project_id FROM installations WHERE project = 'tanukeys'"
        ),
        [[text(tanukeys.as_str())]]
    );
}

#[test]
fn opening_the_ledger_to_write_follows_a_project_linked_since_and_one_given_a_new_id() {
    let (dir, mut workspace, projects) = linking(&["collapse"]);
    drop(workspace.ledger().unwrap());
    let (later, _) = MindProject::init_settings(
        dir.path().join("later"),
        &mindweaver_core::Agent::ALL,
        mindweaver_core::Share::Shared,
        &mindweaver_core::Directories::default(),
    )
    .unwrap();
    workspace.link(&later).unwrap();
    // What `mind init --new-id` leaves, written here by hand.
    let settings = projects[0].marker_path();
    let renewed = ProjectId::generate();
    let text = std::fs::read_to_string(&settings)
        .unwrap()
        .replace(id_of(&projects[0]).as_str(), renewed.as_str());
    std::fs::write(&settings, text).unwrap();

    let (ledger, _) = workspace.ledger_to_write().unwrap();

    assert_eq!(
        ledger.known_projects().unwrap(),
        [
            (renewed, String::from("collapse")),
            (id_of(&later), String::from("later")),
        ]
    );
}

#[test]
fn a_stale_entry_keeps_its_id_and_a_copy_linked_beside_its_project_gives_none() {
    let (dir, mut workspace, projects) = linking(&["collapse", "tanukeys"]);
    drop(workspace.ledger().unwrap());
    // `collapse` moves away, and `tanukeys` is copied with its `.mind`.
    std::fs::rename(dir.path().join("collapse"), dir.path().join("moved")).unwrap();
    let copy = dir.path().join("copy");
    std::fs::create_dir_all(copy.join(".mind")).unwrap();
    std::fs::copy(
        projects[1].marker_path(),
        copy.join(".mind").join(mindweaver_core::SETTINGS_FILE),
    )
    .unwrap();
    workspace.link(&MindProject::open(&copy).unwrap()).unwrap();

    let (ledger, _) = workspace.ledger_to_write().unwrap();

    // The duplicate is left out, so what was known of `tanukeys` stays.
    assert_eq!(
        ledger.known_projects().unwrap(),
        [
            (id_of(&projects[0]), String::from("collapse")),
            (id_of(&projects[1]), String::from("tanukeys")),
        ]
    );
}
