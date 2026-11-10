//! Unit tests for `ledger.rs`, kept beside it rather than inside it.

use super::*;
use crate::ledger::LEDGER_FILE;

/// A hash as the ledger holds what it is given, which these tests need not
/// be a real one: what is stored is read back as it was written.
fn recorded(text: &str) -> ContentHash {
    ContentHash::recorded(text.to_owned())
}

#[test]
fn a_url_becomes_a_readable_directory_name() {
    assert_eq!(
        slug("https://github.com/acme/skills.git"),
        "github.com-acme-skills"
    );
    assert_eq!(
        slug("git@github.com:acme/skills.git"),
        "github.com-acme-skills"
    );
    assert_eq!(slug("https://user:token@host/x"), "host-x");
    assert_eq!(slug("///"), "source");
}

#[test]
fn registering_the_same_source_twice_returns_the_same_row() {
    let ledger = Sqlite::in_memory().unwrap();
    let first = ledger
        .source_for(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap();
    let again = ledger
        .source_for(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap();
    assert_eq!(first, again);
}

#[test]
fn two_urls_with_one_readable_name_get_separate_directories() {
    let ledger = Sqlite::in_memory().unwrap();
    // Both reduce to `h-a`, and both hold files, so they cannot share one.
    let first = ledger
        .source_for(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap();
    let second = ledger
        .source_for(SourceKind::Git, "ssh://h/a", None, "skills")
        .unwrap();
    assert_eq!(first.directory, "h-a");
    assert_eq!(second.directory, "h-a-2");
}

/// The cache holds the answers read over HTTP beside the clones, in a
/// folder of its own: a source whose URL reads as that folder's name is
/// given another, or its clone would be made over the answers, and its
/// files read as them.
#[test]
fn a_source_is_never_given_a_name_the_cache_keeps_for_itself() {
    let ledger = Sqlite::in_memory().unwrap();
    for (n, url) in ["https://http", "file:///http", "https://HTTP.git"]
        .into_iter()
        .enumerate()
    {
        let source = ledger
            .source_for(SourceKind::Git, url, None, "skills")
            .unwrap();

        assert!(!RESERVED_IN_CACHE.contains(&source.directory.as_str()));
        assert_eq!(source.directory, format!("http-{}", n + 2), "{url}");
    }
}

#[test]
fn a_forgotten_source_frees_its_directory_and_takes_its_artifacts() {
    let ledger = Sqlite::in_memory().unwrap();
    let source = ledger
        .source_for(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap();
    ledger
        .record(
            &source,
            Kind::Skill,
            "deploy",
            "skills/h-a/deploy",
            None,
            None,
        )
        .unwrap();

    assert!(ledger.forget_source(source.id).unwrap());
    assert!(!ledger.forget_source(source.id).unwrap(), "already gone");

    assert!(ledger.gathered().unwrap().is_empty());
    assert_eq!(
        ledger
            .source(SourceKind::Git, "https://h/a", None, "skills")
            .unwrap(),
        None
    );
    // The name it had is free for the next source to reduce to it.
    let next = ledger
        .source_for(SourceKind::Git, "ssh://h/a", None, "skills")
        .unwrap();
    assert_eq!(next.directory, "h-a");
}

#[test]
fn a_reference_is_part_of_what_makes_a_source() {
    let ledger = Sqlite::in_memory().unwrap();
    let main = ledger
        .source_for(SourceKind::Git, "https://h/a", Some("main"), "skills")
        .unwrap();
    let next = ledger
        .source_for(SourceKind::Git, "https://h/a", Some("next"), "skills")
        .unwrap();
    assert_ne!(main.id, next.id);
}

/// Two sources of one address are told apart by their branch, so the
/// origin `--from` takes names the branch too, and only when one was asked
/// for: the address alone is the remote's HEAD.
#[test]
fn a_sources_origin_is_its_address_with_the_branch_asked_for() {
    let ledger = Sqlite::in_memory().unwrap();
    let head = ledger
        .source_for(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap();
    let main = ledger
        .source_for(SourceKind::Git, "https://h/a", Some("main"), "skills")
        .unwrap();

    assert_eq!(head.origin(), "https://h/a");
    assert_eq!(main.origin(), "https://h/a#main");
}

#[test]
fn one_name_from_two_sources_is_two_rows() {
    let ledger = Sqlite::in_memory().unwrap();
    let a = ledger
        .source_for(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap();
    let b = ledger
        .source_for(SourceKind::Git, "https://h/b", None, "skills")
        .unwrap();
    ledger
        .record(&a, Kind::Skill, "deploy", "p/a", Some("From a"), Some("1"))
        .unwrap();
    ledger
        .record(&b, Kind::Skill, "deploy", "p/b", Some("From b"), Some("2"))
        .unwrap();

    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 2);
    assert_eq!(gathered[0].source.url, "https://h/a");
    assert_eq!(gathered[1].source.url, "https://h/b");
}

#[test]
fn gathering_the_same_name_again_updates_rather_than_duplicates() {
    let ledger = Sqlite::in_memory().unwrap();
    let source = ledger
        .source_for(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap();
    ledger
        .record(&source, Kind::Skill, "deploy", "p", Some("Old"), Some("1"))
        .unwrap();
    ledger
        .record(&source, Kind::Skill, "deploy", "p", Some("New"), Some("2"))
        .unwrap();

    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 1);
    assert_eq!(gathered[0].summary.as_deref(), Some("New"));
    assert_eq!(gathered[0].revision.as_deref(), Some("2"));
}

#[test]
fn the_log_keeps_failures_too_and_reads_back_newest_first() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger
        .log(
            Action::Gather,
            Some("https://h/a"),
            Outcome::Ok,
            Some("3 skills"),
        )
        .unwrap();
    ledger
        .log(
            Action::Gather,
            Some("https://h/b"),
            Outcome::Failed,
            Some("no such host"),
        )
        .unwrap();

    let history = ledger.history(10).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].target.as_deref(), Some("https://h/b"));
    assert_eq!(history[0].outcome, "failed");
    assert_eq!(history[1].outcome, "ok");
}

#[test]
fn a_database_from_the_future_is_refused_by_its_version() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    let future = SCHEMA_VERSION + 1;
    {
        let ledger = Sqlite::open(&path).unwrap();
        ledger
            .connection
            .pragma_update(None, "user_version", future)
            .unwrap();
    }

    for error in [
        Sqlite::open(&path).unwrap_err(),
        Sqlite::open_existing(&path).unwrap_err(),
    ] {
        assert!(
            matches!(error, LedgerError::Version { found, .. } if found == future),
            "{error}"
        );
        assert!(
            error.to_string().contains(&format!(
                "schema version {future}, this one reads {SCHEMA_VERSION}"
            )),
            "{error}"
        );
    }
    // Left exactly as it is, and nothing copied for a step this build
    // cannot take.
    assert_eq!(version_of(&path), future);
    assert!(!backup_path(&path, future).exists());
}

/// Every text column `table` holds, as stored, for asserting on what reached
/// the file rather than on what the ledger hands back.
fn stored(ledger: &Sqlite, table: &str) -> String {
    let mut statement = ledger
        .connection
        .prepare(&format!("SELECT * FROM {table}"))
        .unwrap();
    let columns = statement.column_count();
    let rows = statement
        .query_map([], |row| {
            let mut values = Vec::new();
            for column in 0..columns {
                values.push(match row.get_ref(column)? {
                    rusqlite::types::ValueRef::Text(text) => {
                        String::from_utf8_lossy(text).into_owned()
                    }
                    rusqlite::types::ValueRef::Integer(number) => number.to_string(),
                    _ => String::new(),
                });
            }
            Ok(values.join("|"))
        })
        .unwrap();
    rows.map(Result::unwrap).collect::<Vec<_>>().join("\n")
}

#[test]
fn a_source_is_stored_and_found_without_the_credentials_in_its_url() {
    let ledger = Sqlite::in_memory().unwrap();
    let source = ledger
        .source_for(SourceKind::Git, "https://me:hunter2@h/a", None, "skills")
        .unwrap();

    assert_eq!(source.url, "https://h/a");
    assert!(!stored(&ledger, "sources").contains("hunter2"));
    // The same place, asked for without them, is the same source.
    let again = ledger
        .source_for(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap();
    assert_eq!(again, source);
}

#[test]
fn the_log_keeps_no_credential_in_what_it_was_about_or_what_it_said() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger
        .log(
            Action::Gather,
            Some("https://me:hunter2@h/a"),
            Outcome::Failed,
            Some("https://me:hunter2@h/a: cannot be fetched"),
        )
        .unwrap();

    assert!(!stored(&ledger, "actions").contains("hunter2"));
    let history = ledger.history(1).unwrap();
    assert_eq!(history[0].target.as_deref(), Some("https://h/a"));
    assert_eq!(
        history[0].detail.as_deref(),
        Some("https://h/a: cannot be fetched")
    );
}

#[test]
fn a_row_an_older_build_wrote_with_credentials_is_read_back_without_them() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger
        .connection
        .execute_batch(
            "INSERT INTO sources (id, kind, url, reference, subdirectory, directory, first_seen, last_seen)
                 VALUES (1, 'git', 'https://me:hunter2@h/a', NULL, 'skills', 'h-a', 0, 0);
             INSERT INTO artifacts (kind, name, source_id, path, gathered_at)
                 VALUES ('skill', 'deploy', 1, '.mindweaver/skills/h-a/deploy', 0);
             INSERT INTO actions (at, action, target, outcome, detail)
                 VALUES (0, 'gather', 'https://me:hunter2@h/a', 'ok', 'from https://me:hunter2@h/a');",
        )
        .unwrap();

    assert_eq!(ledger.gathered().unwrap()[0].source.url, "https://h/a");
    let history = ledger.history(1).unwrap();
    assert_eq!(history[0].target.as_deref(), Some("https://h/a"));
    assert_eq!(history[0].detail.as_deref(), Some("from https://h/a"));
}

/// A ledger on disk, as a Mindweaver from before the redaction left it: one
/// source gathered with a token in its URL, and the log of that gather.
fn ledger_with_a_token_in_it() -> (tempfile::TempDir, Sqlite) {
    let dir = tempfile::TempDir::new().unwrap();
    let ledger = Sqlite::open(dir.path().join(LEDGER_FILE)).unwrap();
    ledger
        .connection
        .execute_batch(
            "INSERT INTO sources (id, kind, url, reference, subdirectory, directory, first_seen,
                                  last_seen, name, canonical_url)
                 VALUES (1, 'git', 'https://me:hunter2@h/a', NULL, 'skills', 'h-a', 0, 0, 'h-a',
                         'https://h/a');
             INSERT INTO artifacts (kind, name, source_id, path, gathered_at)
                 VALUES ('skill', 'deploy', 1, '.mindweaver/skills/h-a/deploy', 0);
             INSERT INTO actions (at, action, target, outcome, detail)
                 VALUES (0, 'gather', 'https://me:hunter2@h/a', 'ok', '1 added');",
        )
        .unwrap();
    (dir, ledger)
}

#[test]
fn only_a_url_with_credentials_is_tainted_and_it_is_handed_back_without_them() {
    let (_dir, ledger) = ledger_with_a_token_in_it();
    ledger
        .connection
        .execute_batch(
            "INSERT INTO sources (kind, url, reference, subdirectory, directory, first_seen,
                                  last_seen, name, canonical_url)
                 VALUES ('git', 'ssh://git@h/b', NULL, 'skills', 'h-b', 0, 0, 'h-b', 'ssh://h/b'),
                        ('git', 'https://h/c', NULL, 'skills', 'h-c', 0, 0, 'h-c', 'https://h/c');",
        )
        .unwrap();

    let tainted = ledger.tainted_sources().unwrap();

    assert_eq!(tainted.len(), 1, "{tainted:?}");
    assert_eq!(tainted[0].id, 1);
    assert_eq!(tainted[0].url, "https://h/a");
}

#[test]
fn a_scrubbed_source_leaves_no_copy_of_its_credentials_in_the_file() {
    let (dir, ledger) = ledger_with_a_token_in_it();

    ledger.scrub_source(1).unwrap();

    assert!(ledger.tainted_sources().unwrap().is_empty());
    assert_eq!(ledger.gathered().unwrap()[0].source.url, "https://h/a");
    // Found by its URL now, which is what the next gather looks it up by.
    assert!(ledger
        .source(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap()
        .is_some());
    ledger.scrub_log().unwrap();
    let file = std::fs::read(dir.path().join(LEDGER_FILE)).unwrap();
    assert!(!file.windows(7).any(|bytes| bytes == b"hunter2"));
    // Again is nothing.
    ledger.scrub_source(1).unwrap();
    assert_eq!(ledger.gathered().unwrap().len(), 1);
}

/// A ledger on disk at schema 6, holding what `rows` inserts, for the step
/// to schema 7 to take when it is opened.
fn at_six(rows: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    let connection = Connection::open(&path).unwrap();
    for schema in [
        SCHEMA_V1, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4, SCHEMA_V5, SCHEMA_V6,
    ] {
        connection.execute_batch(schema).unwrap();
    }
    connection.execute_batch(rows).unwrap();
    connection.pragma_update(None, "user_version", 6).unwrap();
    (dir, path)
}

#[test]
fn the_step_to_seven_makes_two_spellings_of_one_url_one_source_the_fresher_kept() {
    // Gathered first by its https URL, which failed and left an empty row,
    // then over ssh, which worked: the kept row is the one that filled a
    // shelf, though the other was tried later.
    let (_dir, path) = at_six(
        "INSERT INTO sources (id, kind, url, reference, subdirectory, directory, first_seen, last_seen)
             VALUES (1, 'git', 'git@github.com:acme/skills.git', NULL, 'skills',
                     'github.com-acme-skills', 0, 10),
                    (2, 'git', 'https://github.com/Acme/Skills', NULL, 'skills',
                     'github.com-acme-skills-2', 20, 20);
         INSERT INTO artifacts (kind, name, source_id, path, gathered_at)
             VALUES ('skill', 'deploy', 1, '.mindweaver/skills/github.com-acme-skills/deploy', 10);
         INSERT INTO installations (project, kind, name, source_id, installed_at, origin)
             VALUES ('p', 'skill', 'deploy', 2, 0, 'https://github.com/Acme/Skills');",
    );

    let ledger = Sqlite::open(&path).unwrap();

    let sources = ledger.sources().unwrap();
    assert_eq!(sources.len(), 1, "{sources:?}");
    assert_eq!(sources[0].id, 1);
    assert_eq!(sources[0].name, "acme-skills");
    assert_eq!(
        ledger.merged().unwrap(),
        [Merged {
            url: Redacted::new("https://github.com/Acme/Skills"),
            reference: None,
            directory: String::from("github.com-acme-skills-2"),
            into: sources[0].clone(),
        }]
    );
    // An installation from the row that went is from the kept one, by the
    // origin `--from` takes for it now.
    let record = ledger
        .installation("p", Kind::Skill, "deploy")
        .unwrap()
        .unwrap();
    assert_eq!(record.source_id, Some(1));
    assert_eq!(
        record.origin.as_deref(),
        Some("git@github.com:acme/skills.git")
    );
    // Found by either spelling, or a third.
    for spelling in [
        "https://github.com/acme/skills",
        "ssh://git@github.com/acme/skills.git",
        "acme/skills",
    ] {
        assert_eq!(
            ledger
                .source(SourceKind::Git, spelling, None, "skills")
                .unwrap()
                .map(|source| source.id),
            Some(1),
            "{spelling}"
        );
    }
    // Opened again, there is nothing more to make one, and the merge is
    // kept for a command that writes to say, until it clears it.
    drop(ledger);
    let again = Sqlite::open(&path).unwrap();
    assert_eq!(again.merged().unwrap().len(), 1);
    again.cleared_merge("github.com-acme-skills-2").unwrap();
    assert!(again.merged().unwrap().is_empty());
}

#[test]
fn a_folder_name_a_merge_is_kept_by_is_given_to_no_new_source_until_it_is_cleared() {
    let (_dir, path) = at_six(
        "INSERT INTO sources (id, kind, url, reference, subdirectory, directory, first_seen, last_seen)
             VALUES (1, 'git', 'https://h/a', NULL, 'skills', 'h-a', 0, 0),
                    (2, 'git', 'https://h/a/', NULL, 'skills', 'h-a-2', 10, 10);",
    );
    let ledger = Sqlite::open(&path).unwrap();
    // The row tried earlier went, and the folder name `h-a` with it.
    assert_eq!(ledger.merged().unwrap()[0].directory, "h-a");

    // A new source whose readable name is `h-a` is given another folder.
    let held = ledger
        .source_for(SourceKind::Git, "https://h-a", None, "skills")
        .unwrap();
    assert_eq!(held.directory, "h-a-3");

    // Once its folders are gone it is free, for another place there.
    ledger.cleared_merge("h-a").unwrap();
    let freed = ledger
        .source_for(SourceKind::Git, "https://h-a", None, "agents")
        .unwrap();
    assert_eq!(freed.directory, "h-a");
}

#[test]
fn the_step_to_seven_keeps_the_fresher_rows_artifacts_and_lets_the_others_go_with_its_row() {
    // A branch is part of the identity, empty at the remote's HEAD, so these
    // two are one place, as SQLite's UNIQUE could never say at the HEAD.
    let (_dir, path) = at_six(
        "INSERT INTO sources (id, kind, url, reference, subdirectory, directory, first_seen, last_seen)
             VALUES (1, 'git', 'https://h/a/', 'main', 'skills', 'h-a', 0, 0),
                    (2, 'git', 'HTTPS://H/a', 'main', 'skills', 'h-a-2', 10, 10),
                    (3, 'git', 'https://h/a', 'next', 'skills', 'h-a-3', 0, 0),
                    (4, 'git', 'https://h/a', NULL, 'skills', 'h-a-4', 0, 0),
                    (5, 'git', 'https://h/a/', NULL, 'skills', 'h-a-5', 0, 0);
         INSERT INTO artifacts (kind, name, source_id, path, gathered_at)
             VALUES ('skill', 'only-here', 1, '.mindweaver/skills/h-a/only-here', 0),
                    ('skill', 'deploy', 2, '.mindweaver/skills/h-a-2/deploy', 10);
         INSERT INTO installations (project, kind, name, source_id, installed_at)
             VALUES ('p', 'skill', 'only-here', 1, 0);",
    );

    let ledger = Sqlite::open(&path).unwrap();

    // Nothing points into the folder of a row that went, so the next
    // source given that folder cannot gather over files a record claims.
    let gathered = ledger.gathered().unwrap();
    let names: Vec<(&str, i64, &str)> = gathered
        .iter()
        .map(|g| (g.name.as_str(), g.source.id, g.path.as_str()))
        .collect();
    assert_eq!(names, [("deploy", 2, ".mindweaver/skills/h-a-2/deploy")]);
    let merged = ledger.merged().unwrap();
    let merged: Vec<(&str, i64)> = merged
        .iter()
        .map(|merged| (merged.directory.as_str(), merged.into.id))
        .collect();
    // At the HEAD neither filled a shelf nor was tried later, so the later
    // row is kept.
    assert_eq!(merged, [("h-a-4", 5), ("h-a", 2)]);
    // An installation is about the place, which is the kept row now.
    assert!(stored(&ledger, "installations").contains("|only-here|2|"));
    // Each kept row named in the order it was first gathered, the later
    // of two that would take one name numbered.
    let named: Vec<(i64, String)> = ledger
        .sources()
        .unwrap()
        .into_iter()
        .map(|source| (source.id, source.name.into_string()))
        .collect();
    assert_eq!(
        named,
        [
            (2, String::from("a")),
            (3, String::from("a-2")),
            (5, String::from("a-3")),
        ]
    );
}

#[test]
fn the_step_to_seven_leaves_a_credential_for_the_scrub_and_keeps_none_in_what_it_adds() {
    let (dir, path) = at_six(
        "INSERT INTO sources (id, kind, url, reference, subdirectory, directory, first_seen, last_seen)
             VALUES (1, 'git', 'https://me:hunter2@h/a', NULL, 'skills', 'h-a', 0, 0);",
    );

    let ledger = Sqlite::open(&path).unwrap();

    // The scrub rewrites the clone before the row, so the row is what says
    // the clone still holds it.
    assert_eq!(ledger.tainted_sources().unwrap().len(), 1);
    let identity: String = ledger
        .connection
        .query_row("SELECT canonical_url || name FROM sources", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(!identity.contains("hunter2"), "{identity}");
    ledger.scrub_source(1).unwrap();
    drop(ledger);
    let file = std::fs::read(dir.path().join(LEDGER_FILE)).unwrap();
    assert!(!file.windows(7).any(|bytes| bytes == b"hunter2"));
}

#[test]
fn an_origin_is_recorded_and_read_back_without_credentials() {
    let ledger = Sqlite::in_memory().unwrap();
    let mut written = installation_of("deploy", &[(None, "skills/deploy")]);
    written.origin = Some(Origin::from("https://me:hunter2@h/a#main"));

    ledger.installed("collapse", &written).unwrap();

    assert!(!stored(&ledger, "installations").contains("hunter2"));
    let read = ledger
        .installation("collapse", Kind::Skill, "deploy")
        .unwrap()
        .unwrap();
    assert_eq!(read.origin.as_deref(), Some("https://h/a#main"));
}

#[test]
fn scrubbing_a_source_takes_the_credentials_out_of_the_origins_spelling_it() {
    let (dir, ledger) = ledger_with_a_token_in_it();
    // As schema 3 fills it in from a source an older build stored, and an
    // ssh user name, which is no credential, in another source's.
    ledger
        .connection
        .execute_batch(
            "INSERT INTO sources (id, kind, url, reference, subdirectory, directory, first_seen,
                                  last_seen, name, canonical_url)
                 VALUES (2, 'git', 'ssh://git@h/b', NULL, 'skills', 'h-b', 0, 0, 'h-b', 'ssh://h/b');
             INSERT INTO installations (project, kind, name, source_id, origin, installed_at)
                 VALUES ('../p', 'skill', 'deploy', 1, 'https://me:hunter2@h/a', 0),
                        ('../p', 'skill', 'lint', 2, 'ssh://git@h/b', 0);",
        )
        .unwrap();
    // Handed back without them before the scrub too.
    let origin = |name: &str| {
        ledger
            .installation("../p", Kind::Skill, name)
            .unwrap()
            .unwrap()
            .origin
    };
    assert_eq!(origin("deploy").as_deref(), Some("https://h/a"));

    ledger.scrub_source(1).unwrap();
    ledger.scrub_log().unwrap();

    let installations = stored(&ledger, "installations");
    assert!(installations.contains("|https://h/a|"), "{installations}");
    assert!(installations.contains("|ssh://git@h/b|"), "{installations}");
    assert_eq!(origin("lint").as_deref(), Some("ssh://git@h/b"));
    let file = std::fs::read(dir.path().join(LEDGER_FILE)).unwrap();
    assert!(!file.windows(7).any(|bytes| bytes == b"hunter2"));
}

#[test]
fn scrubbing_the_log_rewrites_only_the_entries_with_credentials_and_counts_them() {
    let (_dir, ledger) = ledger_with_a_token_in_it();
    ledger
        .log(Action::Gather, Some("ssh://git@h/b"), Outcome::Ok, None)
        .unwrap();
    ledger
        .connection
        .execute_batch(
            "INSERT INTO actions (at, action, target, outcome, detail)
                 VALUES (1, 'gather', 'https://h/c', 'failed', 'https://me:hunter2@h/c: refused');",
        )
        .unwrap();

    assert_eq!(ledger.scrub_log().unwrap(), 2);

    assert!(!stored(&ledger, "actions").contains("hunter2"));
    assert!(stored(&ledger, "actions").contains("ssh://git@h/b"));
    assert_eq!(ledger.scrub_log().unwrap(), 0);
}

#[test]
fn opening_only_an_existing_ledger_makes_nothing_when_there_is_none() {
    let dir = tempfile::TempDir::new().unwrap();
    let folder = dir.path().join(".mindweaver");
    let path = folder.join(LEDGER_FILE);

    assert!(Sqlite::open_existing(&path).unwrap().is_none());

    // Not the database, and not the folder it would have gone in either.
    assert!(!folder.exists());
}

#[test]
fn opening_only_an_existing_ledger_still_brings_an_older_one_up_to_date() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    {
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(SCHEMA_V1).unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
    }

    let ledger = Sqlite::open_existing(&path).unwrap().unwrap();

    let version: u32 = ledger
        .connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    assert!(ledger
        .installations("collapse", Kind::Skill)
        .unwrap()
        .is_empty());
}

#[test]
fn a_step_that_fails_leaves_the_ledger_as_it_was() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    {
        // A version 1 ledger that already has an index by the name the second
        // step gives its own, so that step fails after creating its table.
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(SCHEMA_V1).unwrap();
        connection
            .execute_batch("CREATE INDEX installations_by_project ON sources (url);")
            .unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
    }

    let error = Sqlite::open(&path).unwrap_err();

    assert!(
        error.to_string().contains("adding the installations table"),
        "{error}"
    );
    let connection = Connection::open(&path).unwrap();
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 1);
    let tables: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'installations'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tables, 0, "the table the failed step made is still there");
}

/// A ledger as version 1 left it, with one source in it.
fn version_1_at(path: &Path) {
    let connection = Connection::open(path).unwrap();
    connection.execute_batch(SCHEMA_V1).unwrap();
    connection
        .execute(
            "INSERT INTO sources
                 (kind, url, reference, subdirectory, revision, directory, first_seen, last_seen)
             VALUES ('git', 'https://h/a', NULL, 'skills', NULL, 'h-a', 1, 1)",
            [],
        )
        .unwrap();
    connection.pragma_update(None, "user_version", 1).unwrap();
}

fn version_of(path: &Path) -> u32 {
    Connection::open(path)
        .unwrap()
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}

#[test]
fn an_older_ledger_is_copied_aside_before_it_is_migrated() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    version_1_at(&path);

    Sqlite::open(&path).unwrap();

    let backup = dir.path().join("mindweaver.db.v1.bak");
    assert_eq!(backup_path(&path, 1), backup);
    // Opened raw: opening it as a ledger would migrate the copy too.
    assert_eq!(version_of(&backup), 1);
    let copy = Connection::open(&backup).unwrap();
    let url: String = copy
        .query_row("SELECT url FROM sources", [], |row| row.get(0))
        .unwrap();
    assert_eq!(url, "https://h/a");
    let installations: i64 = copy
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'installations'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(installations, 0, "the copy was taken after the step");
    assert_eq!(version_of(&path), SCHEMA_VERSION);
}

#[test]
fn a_ledger_another_process_migrates_meanwhile_is_left_to_that_ones_copy() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    version_1_at(&path);
    // The copy the other one took, which only it may write.
    let backup = backup_path(&path, 1);
    std::fs::write(&backup, "the other one's copy").unwrap();

    // The other one, holding the write lock the migration takes.
    let other = Connection::open(&path).unwrap();
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    let opening = std::thread::spawn({
        let path = path.clone();
        move || Sqlite::open(path).map(drop)
    });
    // Long enough to read version 1 and wait for the lock, which is when a
    // copy taken outside it would be taken.
    std::thread::sleep(std::time::Duration::from_millis(300));
    other.execute_batch(SCHEMA_V2).unwrap();
    other.execute_batch(SCHEMA_V3).unwrap();
    other
        .pragma_update(None, "user_version", SCHEMA_VERSION)
        .unwrap();
    other.execute_batch("COMMIT").unwrap();

    opening.join().unwrap().unwrap();
    assert!(
        std::fs::read(&backup).unwrap() == b"the other one's copy",
        "the other one's copy was replaced"
    );
    assert_eq!(version_of(&path), SCHEMA_VERSION);
}

#[test]
fn a_ledger_made_now_or_already_current_is_not_copied() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);

    Sqlite::open(&path).unwrap();
    Sqlite::open(&path).unwrap();

    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, [LEDGER_FILE], "{names:?}");
}

#[test]
fn what_an_earlier_attempt_left_beside_the_ledger_is_replaced() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    version_1_at(&path);
    let backup = backup_path(&path, 1);
    std::fs::write(&backup, "an older copy").unwrap();
    std::fs::write(dir.path().join("mindweaver.db.v1.bak.tmp"), "half a copy").unwrap();
    std::fs::write(dir.path().join("mindweaver.db.v1.bak.tmp-journal"), "junk").unwrap();

    Sqlite::open(&path).unwrap();

    assert_eq!(version_of(&backup), 1);
    assert!(!dir.path().join("mindweaver.db.v1.bak.tmp").exists());
    assert!(!dir.path().join("mindweaver.db.v1.bak.tmp-journal").exists());
}

#[test]
fn a_ledger_that_cannot_be_copied_is_not_migrated() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    version_1_at(&path);
    // A folder holding something, where the copy would go, which no rename
    // replaces.
    let backup = backup_path(&path, 1);
    std::fs::create_dir(&backup).unwrap();
    std::fs::write(backup.join("keep"), "mine").unwrap();

    let error = Sqlite::open(&path).unwrap_err();

    assert!(matches!(error, LedgerError::Backup { .. }), "{error}");
    assert!(
        error.to_string().contains("so it is left as it is"),
        "{error}"
    );
    assert_eq!(version_of(&path), 1);
    assert!(backup.join("keep").is_file());
}

#[cfg(unix)]
#[test]
fn the_copy_is_no_more_readable_than_the_ledger() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    version_1_at(&path);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

    Sqlite::open(&path).unwrap();

    let mode = std::fs::metadata(backup_path(&path, 1))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[cfg(unix)]
#[test]
fn the_copy_is_no_more_readable_than_the_ledger_before_anything_is_in_it() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    version_1_at(&path);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let temporary = dir.path().join("mindweaver.db.v1.bak.tmp");

    create_like(&temporary, &path).unwrap();

    let made = std::fs::metadata(&temporary).unwrap();
    assert_eq!(made.len(), 0);
    assert_eq!(made.permissions().mode() & 0o777, 0o600);
}

/// An installation of `name` with a copy at each of `targets`.
fn installation_of(name: &str, targets: &[(Option<&str>, &str)]) -> InstallRecord {
    InstallRecord {
        kind: Kind::Skill,
        name: name.to_owned().into(),
        source_id: None,
        origin: Some(Origin::from("workspace")),
        content_hash: Some(recorded("e3b0c442")),
        targets: targets
            .iter()
            .map(|(agent, path)| Target {
                agent: agent.map(str::to_owned),
                path: (*path).to_owned(),
                content_hash: None,
            })
            .collect(),
        entries: Vec::new(),
    }
}

#[test]
fn each_copy_reads_back_with_the_hash_of_its_own_it_was_given() {
    let ledger = Sqlite::in_memory().unwrap();
    let mut written = installation_of(
        "git/no-force-push",
        &[
            (None, "AGENTS.md"),
            (
                Some("claude"),
                ".claude/rules/mindweaver/git/no-force-push.md",
            ),
        ],
    );
    written.kind = Kind::Rule;
    written.targets[0].content_hash = Some(recorded("section"));
    written.targets[1].content_hash = Some(recorded("file"));

    ledger.installed("collapse", &written).unwrap();

    let read = ledger
        .installation("collapse", Kind::Rule, "git/no-force-push")
        .unwrap()
        .unwrap();
    // By path, as every copy reads back.
    let mut expected = written;
    expected.targets.reverse();
    assert_eq!(read, expected);
    assert_eq!(ledger.records("collapse", Kind::Rule).unwrap(), [expected]);
}

#[test]
fn a_file_shared_by_several_installations_is_one_copy() {
    let ledger = Sqlite::in_memory().unwrap();
    for name in ["commit-style", "review"] {
        let mut rule = installation_of(name, &[(None, "AGENTS.md")]);
        rule.kind = Kind::Rule;
        ledger.installed("collapse", &rule).unwrap();
    }

    assert_eq!(
        ledger.copies("collapse", Kind::Rule).unwrap(),
        ["AGENTS.md"]
    );
}

/// A rule's installation with a copy at each of `targets`, each with a hash
/// of its own.
fn rule_of(name: &str, targets: &[(Option<&str>, &str, &str)]) -> InstallRecord {
    InstallRecord {
        kind: Kind::Rule,
        name: name.to_owned().into(),
        source_id: None,
        origin: Some(Origin::from("workspace")),
        content_hash: Some(recorded(&format!("{name}-rule"))),
        targets: targets
            .iter()
            .map(|(agent, path, hash)| Target {
                agent: agent.map(str::to_owned),
                path: (*path).to_owned(),
                content_hash: Some(recorded(hash)),
            })
            .collect(),
        entries: Vec::new(),
    }
}

#[test]
fn a_render_records_every_rule_it_left_and_forgets_the_rest_of_its_kind() {
    let ledger = Sqlite::in_memory().unwrap();
    let commits = rule_of("git/commits", &[(None, "AGENTS.md", "a")]);
    let review = rule_of("review", &[(None, "AGENTS.md", "b")]);
    ledger
        .rendered(
            "collapse",
            Kind::Rule,
            &[commits.clone(), review],
            &[("AGENTS.md", true)],
        )
        .unwrap();
    ledger
        .installed(
            "collapse",
            &installation_of("deploy", &[(None, "skills/deploy")]),
        )
        .unwrap();
    ledger
        .rendered("tanukeys", Kind::Rule, std::slice::from_ref(&commits), &[])
        .unwrap();
    let changed = rule_of(
        "git/commits",
        &[
            (None, "AGENTS.md", "c"),
            (
                Some("claude"),
                ".claude/rules/mindweaver/git/commits.md",
                "d",
            ),
        ],
    );

    ledger
        .rendered(
            "collapse",
            Kind::Rule,
            std::slice::from_ref(&changed),
            &[("AGENTS.md", false)],
        )
        .unwrap();

    let mut expected = changed;
    expected.targets.reverse();
    assert_eq!(ledger.records("collapse", Kind::Rule).unwrap(), [expected]);
    assert!(!ledger.created("collapse", "AGENTS.md").unwrap());
    // Another kind, and another project, are not the render's.
    assert_eq!(
        ledger.installations("collapse", Kind::Skill).unwrap(),
        ["deploy"]
    );
    assert_eq!(ledger.records("tanukeys", Kind::Rule).unwrap(), [commits]);
}

#[test]
fn a_render_keeps_the_time_an_installation_was_first_recorded_at() {
    let ledger = Sqlite::in_memory().unwrap();
    let commits = rule_of("git/commits", &[(None, "AGENTS.md", "a")]);
    ledger
        .rendered("collapse", Kind::Rule, std::slice::from_ref(&commits), &[])
        .unwrap();
    ledger
        .connection
        .execute("UPDATE installations SET installed_at = 1", [])
        .unwrap();

    ledger
        .rendered("collapse", Kind::Rule, &[commits], &[])
        .unwrap();

    let at: i64 = ledger
        .connection
        .query_row("SELECT installed_at FROM installations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(at, 1);
}

#[test]
fn whether_mindweaver_made_a_file_is_kept_per_project_until_it_is_forgotten() {
    let ledger = Sqlite::in_memory().unwrap();
    assert!(!ledger.created("collapse", "AGENTS.md").unwrap());

    ledger.set_created("collapse", "AGENTS.md", true).unwrap();
    // Noted twice is noted once.
    ledger.set_created("collapse", "AGENTS.md", true).unwrap();

    assert!(ledger.created("collapse", "AGENTS.md").unwrap());
    assert!(!ledger.created("tanukeys", "AGENTS.md").unwrap());
    assert!(!ledger.created("collapse", "CLAUDE.md").unwrap());

    ledger.set_created("collapse", "AGENTS.md", false).unwrap();
    assert!(!ledger.created("collapse", "AGENTS.md").unwrap());
    // Forgetting what was never noted is nothing to do.
    ledger.set_created("tanukeys", "AGENTS.md", false).unwrap();
}

#[test]
fn an_installation_reads_back_with_every_copy_it_wrote() {
    let ledger = Sqlite::in_memory().unwrap();
    let written = installation_of(
        "deploy",
        &[
            (Some("claude"), ".claude/skills/deploy"),
            (None, ".agents/skills/deploy"),
        ],
    );

    ledger.installed("collapse", &written).unwrap();

    let read = ledger
        .installation("collapse", Kind::Skill, "deploy")
        .unwrap()
        .unwrap();
    // By path, whatever order they were written in.
    assert_eq!(read.targets[0].path, ".agents/skills/deploy");
    assert_eq!(read.targets[1].path, ".claude/skills/deploy");
    let mut expected = written;
    expected.targets.reverse();
    assert_eq!(read, expected);
    assert!(ledger
        .installation("collapse", Kind::Rule, "deploy")
        .unwrap()
        .is_none());
    assert!(ledger
        .installation("tanukeys", Kind::Skill, "deploy")
        .unwrap()
        .is_none());
}

#[test]
fn installing_again_replaces_the_copies_recorded_rather_than_adding_to_them() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger
        .installed(
            "collapse",
            &installation_of(
                "deploy",
                &[(None, ".agents/skills/deploy"), (None, "skills/deploy")],
            ),
        )
        .unwrap();
    let mut again = installation_of("deploy", &[(None, "skills/deploy")]);
    again.content_hash = Some(recorded("another"));

    ledger.installed("collapse", &again).unwrap();

    assert_eq!(
        ledger
            .installation("collapse", Kind::Skill, "deploy")
            .unwrap(),
        Some(again)
    );
    assert_eq!(
        ledger.installations("collapse", Kind::Skill).unwrap(),
        ["deploy"]
    );
}

#[test]
fn a_record_naming_one_copy_twice_is_refused_and_the_old_one_stands() {
    let ledger = Sqlite::in_memory().unwrap();
    let first = installation_of("deploy", &[(None, "skills/deploy")]);
    ledger.installed("collapse", &first).unwrap();

    let twice = installation_of(
        "deploy",
        &[(Some("claude"), "skills/deploy"), (None, "skills/deploy")],
    );
    assert!(ledger.installed("collapse", &twice).is_err());

    assert_eq!(
        ledger
            .installation("collapse", Kind::Skill, "deploy")
            .unwrap(),
        Some(first)
    );
}

#[test]
fn forgetting_an_installation_forgets_its_copies() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger
        .installed(
            "collapse",
            &installation_of(
                "deploy",
                &[
                    (None, ".agents/skills/deploy"),
                    (None, ".claude/skills/deploy"),
                ],
            ),
        )
        .unwrap();

    assert!(ledger
        .uninstalled("collapse", Kind::Skill, "deploy")
        .unwrap());

    assert!(ledger
        .installation("collapse", Kind::Skill, "deploy")
        .unwrap()
        .is_none());
    let targets: i64 = ledger
        .connection
        .query_row("SELECT count(*) FROM installation_targets", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(targets, 0);
}

#[test]
fn forgetting_one_copy_keeps_the_installation_until_its_last_copy_goes() {
    let ledger = Sqlite::in_memory().unwrap();
    let written = installation_of(
        "deploy",
        &[
            (None, ".agents/skills/deploy"),
            (Some("claude"), ".claude/skills/deploy"),
        ],
    );
    ledger.installed("collapse", &written).unwrap();

    let forgot = |path| {
        ledger
            .uninstalled_copy("collapse", Kind::Skill, "deploy", path)
            .unwrap()
    };
    assert_eq!(forgot("skills/deploy"), Forgotten::Nothing);
    assert_eq!(forgot(".claude/skills/deploy"), Forgotten::Copy);
    assert_eq!(
        ledger
            .installation("collapse", Kind::Skill, "deploy")
            .unwrap(),
        Some(InstallRecord {
            targets: written.targets[..1].to_vec(),
            ..written.clone()
        })
    );
    assert_eq!(forgot(".claude/skills/deploy"), Forgotten::Nothing);
    assert_eq!(forgot(".agents/skills/deploy"), Forgotten::Installation);
    assert!(ledger
        .installation("collapse", Kind::Skill, "deploy")
        .unwrap()
        .is_none());
    assert_eq!(forgot(".agents/skills/deploy"), Forgotten::Nothing);
}

#[test]
fn a_renamed_copy_takes_its_installation_with_it_when_it_was_the_only_one() {
    let ledger = Sqlite::in_memory().unwrap();
    let written = installation_of("deploy", &[(None, "skills/deploy")]);
    ledger.installed("collapse", &written).unwrap();
    let when = |ledger: &Sqlite| -> i64 {
        ledger
            .connection
            .query_row("SELECT installed_at FROM installations", [], |row| {
                row.get(0)
            })
            .unwrap()
    };
    let installed_at = when(&ledger);

    assert_eq!(
        ledger
            .renamed_copy(
                "collapse",
                Kind::Skill,
                "deploy",
                "skills/deploy",
                "ship",
                "skills/ship"
            )
            .unwrap(),
        Carried::Moved
    );

    assert!(ledger
        .installation("collapse", Kind::Skill, "deploy")
        .unwrap()
        .is_none());
    assert_eq!(
        ledger
            .installation("collapse", Kind::Skill, "ship")
            .unwrap(),
        Some(InstallRecord {
            name: "ship".into(),
            ..installation_of("ship", &[(None, "skills/ship")])
        })
    );
    assert_eq!(when(&ledger), installed_at);
}

#[test]
fn a_renamed_copy_is_an_installation_of_its_own_beside_the_copies_it_left() {
    let ledger = Sqlite::in_memory().unwrap();
    let mut written = installation_of(
        "git/no-force-push",
        &[(None, "AGENTS.md"), (None, "rules/git/no-force-push.md")],
    );
    written.kind = Kind::Rule;
    written.targets[1].content_hash = Some(recorded("4f9c2a7e"));
    ledger.installed("collapse", &written).unwrap();

    let carried = ledger
        .renamed_copy(
            "collapse",
            Kind::Rule,
            "git/no-force-push",
            "rules/git/no-force-push.md",
            "git/never-force-push",
            "rules/git/never-force-push.md",
        )
        .unwrap();

    assert_eq!(carried, Carried::Moved);
    assert_eq!(
        ledger
            .installation("collapse", Kind::Rule, "git/no-force-push")
            .unwrap(),
        Some(InstallRecord {
            targets: written.targets[..1].to_vec(),
            ..written.clone()
        })
    );
    // With the hash of what was written there, which the copy still holds.
    assert_eq!(
        ledger
            .installation("collapse", Kind::Rule, "git/never-force-push")
            .unwrap(),
        Some(InstallRecord {
            name: "git/never-force-push".into(),
            targets: vec![Target {
                agent: None,
                path: String::from("rules/git/never-force-push.md"),
                content_hash: Some(recorded("4f9c2a7e")),
            }],
            ..written
        })
    );
}

#[test]
fn a_copy_renamed_onto_a_name_installed_already_is_forgotten() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger
        .installed(
            "collapse",
            &installation_of("deploy", &[(None, "skills/deploy")]),
        )
        .unwrap();
    let ship = installation_of("ship", &[(None, ".agents/skills/ship")]);
    ledger.installed("collapse", &ship).unwrap();
    let renamed = |path| {
        ledger
            .renamed_copy(
                "collapse",
                Kind::Skill,
                "deploy",
                path,
                "ship",
                "skills/ship",
            )
            .unwrap()
    };

    assert_eq!(renamed("skills/elsewhere"), Carried::Nothing);
    assert_eq!(renamed("skills/deploy"), Carried::Forgotten);

    assert_eq!(
        ledger.installations("collapse", Kind::Skill).unwrap(),
        ["ship"]
    );
    assert_eq!(
        ledger
            .installation("collapse", Kind::Skill, "ship")
            .unwrap(),
        Some(ship)
    );
    assert_eq!(renamed("skills/deploy"), Carried::Nothing);
}

#[test]
fn an_installation_recording_no_copy_is_forgotten_whole_by_any_one() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger
        .installed("collapse", &installation_of("deploy", &[]))
        .unwrap();

    assert_eq!(
        ledger
            .uninstalled_copy("collapse", Kind::Skill, "deploy", "skills/deploy")
            .unwrap(),
        Forgotten::Installation
    );
    assert!(ledger
        .installations("collapse", Kind::Skill)
        .unwrap()
        .is_empty());
}

#[test]
fn a_rules_installation_recording_no_copy_is_kept_by_a_change_elsewhere() {
    let ledger = Sqlite::in_memory().unwrap();
    let rule = InstallRecord {
        kind: Kind::Rule,
        ..installation_of("rust/errors", &[])
    };
    ledger.installed("collapse", &rule).unwrap();

    assert_eq!(
        ledger
            .uninstalled_copy(
                "collapse",
                Kind::Rule,
                "rust/errors",
                "rules/rust/errors.md"
            )
            .unwrap(),
        Forgotten::Nothing
    );
    assert_eq!(
        ledger
            .installation("collapse", Kind::Rule, "rust/errors")
            .unwrap(),
        Some(rule)
    );
}

#[test]
fn retargeting_an_installation_moves_its_copies_and_keeps_the_rest_of_its_record() {
    let ledger = Sqlite::in_memory().unwrap();
    let written = installation_of("deploy", &[(Some("claude"), ".claude/skills/deploy")]);
    ledger.installed("collapse", &written).unwrap();
    let when = |ledger: &Sqlite| -> i64 {
        ledger
            .connection
            .query_row("SELECT installed_at FROM installations", [], |row| {
                row.get(0)
            })
            .unwrap()
    };
    let installed_at = when(&ledger);
    let moved = installation_of("deploy", &[(None, ".agents/skills/deploy")]).targets;

    assert!(ledger
        .retargeted("collapse", Kind::Skill, "deploy", &moved)
        .unwrap());

    assert_eq!(
        ledger
            .installation("collapse", Kind::Skill, "deploy")
            .unwrap(),
        Some(InstallRecord {
            targets: moved,
            ..written
        })
    );
    // Not installed again: it is the same installation, from when it was.
    assert_eq!(when(&ledger), installed_at);
}

#[test]
fn retargeting_an_installation_to_no_copy_forgets_it_and_one_never_made_is_nothing() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger
        .installed(
            "collapse",
            &installation_of("deploy", &[(None, ".agents/skills/deploy")]),
        )
        .unwrap();

    assert!(ledger
        .retargeted("collapse", Kind::Skill, "deploy", &[])
        .unwrap());
    assert!(ledger
        .installations("collapse", Kind::Skill)
        .unwrap()
        .is_empty());
    assert!(!ledger
        .retargeted("collapse", Kind::Skill, "deploy", &[])
        .unwrap());
    assert!(!ledger
        .retargeted(
            "collapse",
            Kind::Skill,
            "review",
            &installation_of("review", &[(None, ".agents/skills/review")]).targets,
        )
        .unwrap());
    assert!(ledger
        .installations("collapse", Kind::Skill)
        .unwrap()
        .is_empty());
}

#[test]
fn a_migrated_project_moves_every_installation_fills_a_missing_hash_and_logs_it_at_once() {
    let ledger = Sqlite::in_memory().unwrap();
    let hashed = installation_of("deploy", &[(None, "skills/deploy")]);
    let unhashed = InstallRecord {
        content_hash: None,
        origin: None,
        ..installation_of("review", &[(None, "skills/review")])
    };
    ledger.installed("collapse", &hashed).unwrap();
    ledger.installed("collapse", &unhashed).unwrap();
    let moved = |name: &str, hash: Option<&str>| Moved {
        kind: Kind::Skill,
        name: name.to_owned().into(),
        targets: installation_of(name, &[(Some("claude"), &format!(".claude/skills/{name}"))])
            .targets,
        content_hash: hash.map(recorded),
    };

    ledger
        .migrated(
            "collapse",
            &id('a'),
            &[
                moved("deploy", Some("ffff")),
                moved("review", Some("abcd")),
                moved("gone", Some("0000")),
            ],
            &[(Action::Install, String::from("collapse moved to settings"))],
        )
        .unwrap();

    let record = |name: &str| {
        ledger
            .installation("collapse", Kind::Skill, name)
            .unwrap()
            .unwrap()
    };
    // A hash it had is kept: what was installed, not what is there now.
    assert_eq!(record("deploy").content_hash.as_deref(), Some("e3b0c442"));
    assert_eq!(record("review").content_hash.as_deref(), Some("abcd"));
    assert_eq!(
        record("review").targets,
        moved("review", None).targets,
        "{:?}",
        record("review")
    );
    assert!(ledger
        .installation("collapse", Kind::Skill, "gone")
        .unwrap()
        .is_none());
    // Known by the id its settings gave it, and its records with it.
    assert_eq!(ledger.project_at("collapse").unwrap(), Some(id('a')));
    assert_eq!(
        project_ids(&ledger, "collapse"),
        [
            (String::from("deploy"), Some(id('a').to_string())),
            (String::from("review"), Some(id('a').to_string()))
        ]
    );
    let history = ledger.history(5).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].target.as_deref(), Some("collapse"));
    assert_eq!(
        history[0].detail.as_deref(),
        Some("collapse moved to settings")
    );
}

#[test]
fn a_migration_the_ledger_cannot_finish_records_none_of_it() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger
        .installed(
            "collapse",
            &installation_of("deploy", &[(None, "skills/deploy")]),
        )
        .unwrap();
    // Two copies at one path break the table's uniqueness, after the first
    // installation's copies were written.
    let twice = Target {
        agent: None,
        path: String::from(".agents/skills/deploy"),
        content_hash: None,
    };

    let failed = ledger.migrated(
        "collapse",
        &id('a'),
        &[Moved {
            kind: Kind::Skill,
            name: "deploy".into(),
            targets: vec![twice.clone(), twice],
            content_hash: None,
        }],
        &[(Action::Install, String::from("collapse moved"))],
    );

    assert!(failed.is_err());
    assert_eq!(
        ledger.copies("collapse", Kind::Skill).unwrap(),
        ["skills/deploy"]
    );
    assert_eq!(ledger.project_at("collapse").unwrap(), None);
    assert!(ledger.history(5).unwrap().is_empty());
}

#[test]
fn every_copy_installed_into_a_project_is_listed_by_path_whichever_install_wrote_it() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger
        .installed(
            "collapse",
            &installation_of(
                "deploy",
                &[
                    (Some("claude"), ".claude/skills/deploy"),
                    (None, ".agents/skills/deploy"),
                ],
            ),
        )
        .unwrap();
    ledger
        .installed(
            "collapse",
            &installation_of("audit", &[(None, ".agents/skills/audit")]),
        )
        .unwrap();
    ledger
        .installed(
            "tanukeys",
            &installation_of("deploy", &[(None, "skills/deploy")]),
        )
        .unwrap();

    assert_eq!(
        ledger.copies("collapse", Kind::Skill).unwrap(),
        [
            ".agents/skills/audit",
            ".agents/skills/deploy",
            ".claude/skills/deploy"
        ]
    );
    assert_eq!(
        ledger.copies("tanukeys", Kind::Skill).unwrap(),
        ["skills/deploy"]
    );
    assert!(ledger.copies("collapse", Kind::Rule).unwrap().is_empty());
    assert!(ledger.copies("elsewhere", Kind::Skill).unwrap().is_empty());
}

#[test]
fn every_installation_into_a_project_reads_back_with_its_copies_in_name_order() {
    let ledger = Sqlite::in_memory().unwrap();
    let deploy = installation_of(
        "deploy",
        &[
            (Some("claude"), ".claude/skills/deploy"),
            (None, ".agents/skills/deploy"),
        ],
    );
    let audit = InstallRecord {
        origin: Some(Origin::from("https://github.com/acme/skills")),
        source_id: None,
        ..installation_of("audit", &[(None, ".agents/skills/audit")])
    };
    let empty = installation_of("bare", &[]);
    for record in [&deploy, &audit, &empty] {
        ledger.installed("collapse", record).unwrap();
    }
    ledger
        .installed(
            "tanukeys",
            &installation_of("deploy", &[(None, "skills/deploy")]),
        )
        .unwrap();

    let records = ledger.records("collapse", Kind::Skill).unwrap();

    let mut deploy = deploy;
    deploy.targets.reverse();
    assert_eq!(records, [audit, empty, deploy]);
    for record in &records {
        assert_eq!(
            ledger
                .installation("collapse", Kind::Skill, &record.name)
                .unwrap()
                .as_ref(),
            Some(record)
        );
    }
    assert!(ledger.records("collapse", Kind::Rule).unwrap().is_empty());
    assert!(ledger.records("elsewhere", Kind::Skill).unwrap().is_empty());
}

#[test]
fn every_installation_reads_back_without_the_credentials_an_older_build_stored() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger
        .installed(
            "collapse",
            &installation_of("deploy", &[(None, "skills/deploy")]),
        )
        .unwrap();
    ledger
        .connection
        .execute(
            "UPDATE installations SET origin = 'https://user:ghp_t0ken@github.com/acme/skills'",
            [],
        )
        .unwrap();

    let records = ledger.records("collapse", Kind::Skill).unwrap();

    assert_eq!(
        records[0].origin.as_deref(),
        Some("https://github.com/acme/skills")
    );
}

#[test]
fn a_ledger_waits_for_another_writer_as_long_as_a_writer_waits_for_the_workspace() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(LEDGER_FILE);
    let opened = Sqlite::open(&path).unwrap();
    let existing = Sqlite::open_existing(&path).unwrap().unwrap();

    // Read off each connection rather than shown by two writers racing:
    // rusqlite waits five seconds of its own accord, which is long enough
    // for any race a test can stage.
    for ledger in [&opened, &existing] {
        let waits: i64 = ledger
            .connection
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .unwrap();
        assert_eq!(
            u128::try_from(waits).unwrap(),
            crate::lock::WAIT.as_millis()
        );
    }
}

fn id(digit: char) -> ProjectId {
    ProjectId::parse(&digit.to_string().repeat(ProjectId::LEN)).unwrap()
}

/// The id each installation of `project` names, by name.
fn project_ids(ledger: &Sqlite, project: &str) -> Vec<(String, Option<String>)> {
    let mut statement = ledger
        .connection
        .prepare("SELECT name, project_id FROM installations WHERE project = ?1 ORDER BY name")
        .unwrap();
    statement
        .query_map([project], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn an_install_names_the_id_known_at_its_route_and_none_where_none_is() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger
        .knew(&[(id('a'), String::from("collapse"))], &[])
        .unwrap();

    for project in ["collapse", "legacy"] {
        ledger
            .installed(
                project,
                &installation_of("deploy", &[(None, "skills/deploy")]),
            )
            .unwrap();
    }

    assert_eq!(
        project_ids(&ledger, "collapse"),
        [(String::from("deploy"), Some(id('a').to_string()))]
    );
    assert_eq!(
        project_ids(&ledger, "legacy"),
        [(String::from("deploy"), None)]
    );
}

#[test]
fn a_route_known_by_an_id_is_read_back_both_ways() {
    let ledger = Sqlite::in_memory().unwrap();

    let learned = ledger
        .knew(
            &[
                (id('b'), String::from("tanukeys")),
                (id('a'), String::from("collapse")),
            ],
            &[],
        )
        .unwrap();

    assert_eq!(learned, 2);
    assert_eq!(ledger.project_at("collapse").unwrap(), Some(id('a')));
    assert_eq!(ledger.project_at("elsewhere").unwrap(), None);
    assert_eq!(
        ledger.known_projects().unwrap(),
        [
            (id('a'), String::from("collapse")),
            (id('b'), String::from("tanukeys"))
        ]
    );
    // Knowing it again is nothing new.
    assert_eq!(
        ledger
            .knew(&[(id('a'), String::from("collapse"))], &[])
            .unwrap(),
        0
    );
}

#[test]
fn a_route_whose_project_holds_another_id_now_is_known_by_that_one_with_its_records() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger
        .knew(&[(id('a'), String::from("collapse"))], &[])
        .unwrap();
    ledger
        .installed(
            "collapse",
            &installation_of("deploy", &[(None, "skills/deploy")]),
        )
        .unwrap();

    ledger
        .knew(&[(id('c'), String::from("collapse"))], &[])
        .unwrap();

    assert_eq!(ledger.project_at("collapse").unwrap(), Some(id('c')));
    assert_eq!(ledger.known_projects().unwrap().len(), 1);
    assert_eq!(
        project_ids(&ledger, "collapse"),
        [(String::from("deploy"), Some(id('c').to_string()))]
    );
}

#[test]
fn an_id_last_seen_at_a_stale_entry_stays_there_until_a_relink_moves_it() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger.knew(&[(id('a'), String::from("old"))], &[]).unwrap();

    ledger
        .knew(&[(id('a'), String::from("new"))], &[String::from("old")])
        .unwrap();
    assert_eq!(ledger.project_at("old").unwrap(), Some(id('a')));
    assert_eq!(ledger.project_at("new").unwrap(), None);

    // One no entry links any more is no reason to keep it there.
    ledger.knew(&[(id('a'), String::from("new"))], &[]).unwrap();
    assert_eq!(ledger.project_at("old").unwrap(), None);
    assert_eq!(ledger.project_at("new").unwrap(), Some(id('a')));
}

#[test]
fn a_relink_moves_every_record_and_file_made_to_the_new_route() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger.knew(&[(id('a'), String::from("old"))], &[]).unwrap();
    for name in ["deploy", "review"] {
        let path = format!("skills/{name}");
        ledger
            .installed("old", &installation_of(name, &[(None, &path)]))
            .unwrap();
    }
    ledger.set_created("old", "AGENTS.md", true).unwrap();

    let mut wrote = false;
    let moved = ledger
        .relinked_by("old", "new", &id('a'), &mut || {
            wrote = true;
            true
        })
        .unwrap();

    assert!(wrote, "the write runs inside the move");
    assert_eq!(moved, Some(2));
    assert!(ledger.records("old", Kind::Skill).unwrap().is_empty());
    assert_eq!(
        ledger.installations("new", Kind::Skill).unwrap(),
        ["deploy", "review"]
    );
    assert_eq!(
        project_ids(&ledger, "new"),
        [
            (String::from("deploy"), Some(id('a').to_string())),
            (String::from("review"), Some(id('a').to_string()))
        ]
    );
    assert!(ledger.created("new", "AGENTS.md").unwrap());
    assert!(!ledger.holds("old").unwrap());
    assert_eq!(ledger.project_at("new").unwrap(), Some(id('a')));
    assert_eq!(ledger.project_at("old").unwrap(), None);
    let logged = &ledger.history(1).unwrap()[0];
    assert_eq!(logged.action, "relink");
    assert_eq!(logged.detail.as_deref(), Some("old to new"));
}

#[test]
fn a_relink_drops_what_another_project_left_at_the_new_route() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger
        .knew(
            &[
                (id('a'), String::from("old")),
                (id('b'), String::from("new")),
            ],
            &[],
        )
        .unwrap();
    ledger
        .installed(
            "old",
            &installation_of("deploy", &[(None, "skills/deploy")]),
        )
        .unwrap();
    for name in ["deploy", "lint"] {
        let path = format!("skills/{name}");
        ledger
            .installed("new", &installation_of(name, &[(None, &path)]))
            .unwrap();
    }
    ledger.set_created("new", "AGENTS.md", true).unwrap();

    ledger
        .relinked_by("old", "new", &id('a'), &mut || true)
        .unwrap()
        .unwrap();

    assert_eq!(
        ledger.installations("new", Kind::Skill).unwrap(),
        ["deploy"]
    );
    assert!(!ledger.created("new", "AGENTS.md").unwrap());
    assert_eq!(
        ledger.known_projects().unwrap(),
        [(id('a'), String::from("new"))]
    );
}

#[test]
fn a_relink_whose_marker_is_not_written_moves_nothing() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger.knew(&[(id('a'), String::from("old"))], &[]).unwrap();
    ledger
        .installed(
            "old",
            &installation_of("deploy", &[(None, "skills/deploy")]),
        )
        .unwrap();

    let refused = ledger
        .relinked_by("old", "new", &id('a'), &mut || false)
        .unwrap();

    assert_eq!(refused, None);
    assert_eq!(
        ledger.installations("old", Kind::Skill).unwrap(),
        ["deploy"]
    );
    assert_eq!(ledger.project_at("old").unwrap(), Some(id('a')));
    assert!(ledger.history(1).unwrap().is_empty());
}

#[test]
fn a_route_holds_what_an_install_or_a_made_file_records_there() {
    let ledger = Sqlite::in_memory().unwrap();
    assert!(!ledger.holds("collapse").unwrap());

    ledger.set_created("collapse", "AGENTS.md", true).unwrap();
    assert!(ledger.holds("collapse").unwrap());

    ledger
        .installed(
            "tanukeys",
            &installation_of("deploy", &[(None, "skills/deploy")]),
        )
        .unwrap();
    assert!(ledger.holds("tanukeys").unwrap());
}

#[test]
fn a_new_ledger_knows_each_project_it_is_told_of_once() {
    let ledger = Sqlite::open_knowing(":memory:", || {
        vec![
            (id('a'), String::from("collapse")),
            (id('a'), String::from("copy")),
            (id('b'), String::from("tanukeys")),
        ]
    })
    .unwrap();

    assert_eq!(
        ledger.known_projects().unwrap(),
        [
            (id('a'), String::from("collapse")),
            (id('b'), String::from("tanukeys"))
        ]
    );
}

/// A gathered artifact as a source a test can build, whose URL is whatever
/// it is given.
fn gathered_from(url: &str) -> Gathered {
    Gathered {
        kind: Kind::Skill,
        name: "deploy".into(),
        path: String::from(".mindweaver/skills/github.com-acme-skills/deploy"),
        summary: Some(String::from("Ship it")),
        revision: Some(Revision::from("4f9c2a7")),
        gathered_at: 1_790_000_000,
        source: Source {
            id: 3,
            kind: SourceKind::Git,
            url: Redacted::new(url),
            reference: Some(String::from("main")),
            subdirectory: String::from("skills"),
            revision: Some(Revision::from("4f9c2a7")),
            directory: String::from("github.com-acme-skills"),
            name: SourceName::recorded(String::from("acme-skills")),
        },
    }
}

/// What a gathered artifact serializes to is a contract, as a status
/// report's is: each field by its name, its source's among them, but the
/// row id, which is the ledger's.
#[test]
fn a_gathered_artifact_serializes_every_field_and_its_source() {
    let serialized =
        serde_json::to_string(&gathered_from("https://github.com/acme/skills.git")).unwrap();

    assert_eq!(
        serialized,
        concat!(
            r#"{"kind":"skill","name":"deploy","#,
            r#""path":".mindweaver/skills/github.com-acme-skills/deploy","#,
            r#""summary":"Ship it","revision":"4f9c2a7","gathered_at":1790000000,"#,
            r#""source":{"kind":"git","url":"https://github.com/acme/skills.git","#,
            r#""reference":"main","subdirectory":"skills","revision":"4f9c2a7","#,
            r#""directory":"github.com-acme-skills","name":"acme-skills"}}"#,
        )
    );
}

/// The ledger hands back no URL with a credential in it, but a source built
/// any other way may hold one, and a report must not print it whatever
/// built it.
#[test]
fn a_source_is_serialized_without_the_credentials_its_url_carries() {
    let serialized = serde_json::to_string(&gathered_from(
        "https://me:s3cret@github.com/acme/skills.git",
    ))
    .unwrap();

    assert!(!serialized.contains("s3cret"), "{serialized}");
    assert!(!serialized.contains("me:"), "{serialized}");
    assert!(
        serialized.contains(r#""url":"https://github.com/acme/skills.git""#),
        "{serialized}"
    );
}

#[test]
fn a_source_kind_is_serialized_as_its_slug() {
    assert_eq!(serde_json::to_string(&SourceKind::Git).unwrap(), r#""git""#);
}

/// What a line of the action log serializes to is a contract too: each
/// field by its name.
#[test]
fn an_action_log_entry_serializes_every_field() {
    let entry = Entry {
        at: 1_790_000_000,
        action: String::from("install"),
        target: Some(Redacted::new("collapse")),
        outcome: String::from("ok"),
        detail: None,
    };

    assert_eq!(
        serde_json::to_string(&entry).unwrap(),
        r#"{"at":1790000000,"action":"install","target":"collapse","outcome":"ok","detail":null}"#
    );
}

/// An entry built some other way than [`Sqlite::history`] reads one may
/// hold a credential, in its target or quoted in its detail, and a report
/// must not print it whatever built it.
#[test]
fn an_action_log_entry_is_serialized_without_the_credentials_its_urls_carry() {
    let url = "https://me:s3cret@github.com/acme/skills.git";
    let entry = Entry {
        at: 0,
        action: String::from("gather"),
        target: Some(Redacted::new(url)),
        outcome: String::from("failed"),
        detail: Some(Redacted::new(&format!("{url}: cannot be fetched"))),
    };

    let serialized = serde_json::to_string(&entry).unwrap();

    assert!(!serialized.contains("s3cret"), "{serialized}");
    assert!(
        serialized.contains(r#""target":"https://github.com/acme/skills.git""#),
        "{serialized}"
    );
    assert!(
        serialized.contains(r#""detail":"https://github.com/acme/skills.git: cannot be fetched""#),
        "{serialized}"
    );
}

/// Every source is listed, one nothing was gathered from included, by URL,
/// then branch, then folder, and none with the credentials an older build
/// stored in it.
#[test]
fn every_source_is_listed_in_url_order_without_credentials() {
    let ledger = Sqlite::in_memory().unwrap();
    ledger
        .source_for(SourceKind::Git, "https://h/b", None, "skills")
        .unwrap();
    ledger
        .source_for(SourceKind::Git, "https://h/a", Some("main"), "skills")
        .unwrap();
    ledger
        .source_for(SourceKind::Git, "https://h/a", None, "skills")
        .unwrap();
    // What a Mindweaver from before the refusal could have stored.
    ledger
        .connection
        .execute(
            "INSERT INTO sources (kind, url, reference, subdirectory, directory, first_seen, last_seen)
             VALUES ('git', 'https://me:t0ken@h/aa', NULL, 'skills', 'h-aa', 0, 0)",
            [],
        )
        .unwrap();

    let listed: Vec<(String, Option<String>)> = ledger
        .sources()
        .unwrap()
        .into_iter()
        .map(|source| (source.url.into_string(), source.reference))
        .collect();

    assert_eq!(
        listed,
        [
            (String::from("https://h/a"), None),
            (String::from("https://h/a"), Some(String::from("main"))),
            (String::from("https://h/aa"), None),
            (String::from("https://h/b"), None),
        ]
    );
}
