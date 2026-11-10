//! Unit tests for `ledger.rs`, kept beside it rather than inside it.
//!
//! The words the ledger writes (a source's kind, an action, an outcome) are
//! in every database a build has made, and what its read types serialize to
//! is what `--json` and the panel print: both are contracts a rename of a
//! variant or a field would break without a compiler noticing. So each is
//! pinned here word for word, over values the mothers draw.

use super::*;

use std::io;

use serde_json::Value;

use crate::mother::random;
use crate::mother::{
    ArtifactNameMother, EntryMother, GatheredMother, InstallRecordMother, OriginMother,
    ProjectIdMother, RedactedMother, RevisionMother, SourceMother, UrlMother,
};

const DRAWS: usize = 200;

/// Every kind of source, by a match with no wildcard, so a new one does not
/// compile until it is listed here and its slug pinned below.
fn every_source_kind() -> Vec<SourceKind> {
    let listed = |kind: SourceKind| match kind {
        SourceKind::Git => kind,
    };
    vec![listed(SourceKind::Git)]
}

/// Every action, as [`every_source_kind`] lists the kinds.
fn every_action() -> Vec<Action> {
    let listed = |action: Action| match action {
        Action::Gather | Action::Install | Action::Uninstall | Action::Relink | Action::Adopt => {
            action
        }
    };
    [
        Action::Gather,
        Action::Install,
        Action::Uninstall,
        Action::Relink,
        Action::Adopt,
    ]
    .into_iter()
    .map(listed)
    .collect()
}

fn json<T: Serialize + ?Sized>(value: &T) -> String {
    serde_json::to_string(value).expect("a read type serializes")
}

/// The names of an object's fields, in the order they are written.
///
/// Read off the text rather than a `Value`, whose map sorts its keys, since
/// the order a report's fields come in is what a person reading it sees.
fn keys(text: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(text).expect("serialized JSON parses");
    let object = value.as_object().expect("a record serializes as an object");
    let mut keys: Vec<(usize, String)> = object
        .keys()
        .map(|key| {
            let quoted = format!("{}:", json(key));
            let at = text.find(&quoted).expect("every key is in the text");
            (at, key.clone())
        })
        .collect();
    keys.sort();
    keys.into_iter().map(|(_, key)| key).collect()
}

/// What a source must serialize to, written out field by field from what it
/// holds, never through its own serializer.
fn source_as_json(source: &Source) -> String {
    format!(
        r#"{{"kind":{},"url":{},"reference":{},"subdirectory":{},"revision":{},"directory":{},"name":{}}}"#,
        json(source.kind.slug()),
        json(source.url.as_str()),
        json(&source.reference),
        json(&source.subdirectory),
        json(&source.revision.as_ref().map(|revision| revision.as_str())),
        json(&source.directory),
        json(source.name.as_str()),
    )
}

fn lock_busy(path: &str) -> crate::lock::LockError {
    crate::lock::LockError::Busy {
        path: PathBuf::from(path),
        pid: Some(4242),
        waited: crate::lock::WAIT,
    }
}

fn lock_unopened(path: &str) -> crate::lock::LockError {
    crate::lock::LockError::Open {
        path: PathBuf::from(path),
        source: io::Error::other("denied"),
    }
}

/// One of each variant with a code of its own, beside the code it gives.
fn every_own_error(path: &str) -> Vec<(LedgerError, &'static str)> {
    let path = PathBuf::from(path);
    vec![
        (
            LedgerError::Create {
                path: path.clone(),
                source: io::Error::other("no room"),
            },
            "ledger_create_failed",
        ),
        (
            LedgerError::Open {
                path: path.clone(),
                source: Box::new(io::Error::other("not a database")),
            },
            "ledger_open_failed",
        ),
        (
            LedgerError::Version {
                path: path.clone(),
                found: SCHEMA_VERSION + 1,
            },
            "ledger_too_new",
        ),
        (
            LedgerError::Backup {
                path: path.clone(),
                backup: path.with_extension("db.v1.bak"),
                source: io::Error::other("read-only"),
            },
            "ledger_backup_failed",
        ),
        (LedgerError::Unmade { path: path.clone() }, "ledger_unmade"),
        (
            LedgerError::Query {
                path,
                doing: String::from("recording an installation"),
                source: Box::new(io::Error::other("disk I/O error")),
            },
            "ledger_query_failed",
        ),
    ]
}

#[test]
fn every_source_kind_is_parsed_back_from_its_slug() {
    for kind in every_source_kind() {
        assert_eq!(SourceKind::parse(kind.slug()), Some(kind), "{kind:?}");
    }
}

/// The slug is in every database a build has written: renaming one would
/// leave each row an older build wrote naming a kind no build reads.
#[test]
fn a_git_source_is_written_as_git() {
    assert_eq!(SourceKind::Git.slug(), "git");
}

#[test]
fn a_source_kind_serializes_as_its_slug_and_nothing_else() {
    for kind in every_source_kind() {
        assert_eq!(json(&kind), json(kind.slug()), "{kind:?}");
    }
}

/// Parsed as written, with no folding: a row holding another spelling was
/// written by no build, and is skipped rather than read as a git source.
#[test]
fn a_slug_spelled_any_other_way_is_no_source_kind() {
    for slug in [
        "", "Git", "GIT", " git", "git ", "git\n", "github", "svn", "hg",
    ] {
        assert_eq!(SourceKind::parse(slug), None, "{slug:?}");
    }
}

#[test]
fn text_nobody_chose_is_no_source_kind_unless_it_is_a_slug() {
    let slugs: Vec<&str> = every_source_kind()
        .into_iter()
        .map(SourceKind::slug)
        .collect();
    for _ in 0..DRAWS {
        let text = random::pick(&[random::text(8), random::word()]);

        assert_eq!(
            SourceKind::parse(&text).is_some(),
            slugs.contains(&text.as_str()),
            "{text:?}"
        );
    }
}

/// What the action log has said of every action since it was kept: a front
/// end filters the history by these words.
#[test]
fn each_action_is_written_as_the_word_the_log_has_always_held() {
    let written: Vec<(Action, &str)> = every_action()
        .into_iter()
        .map(|action| (action, action.slug()))
        .collect();

    assert_eq!(
        written,
        vec![
            (Action::Gather, "gather"),
            (Action::Install, "install"),
            (Action::Uninstall, "uninstall"),
            (Action::Relink, "relink"),
            (Action::Adopt, "adopt"),
        ]
    );
}

#[test]
fn no_two_actions_are_written_alike() {
    let mut slugs: Vec<&str> = every_action().into_iter().map(Action::slug).collect();
    slugs.sort_unstable();
    slugs.dedup();

    assert_eq!(slugs.len(), every_action().len(), "{slugs:?}");
}

#[test]
fn an_outcome_is_written_as_ok_or_failed() {
    let listed = |outcome: Outcome| match outcome {
        Outcome::Ok => "ok",
        Outcome::Failed => "failed",
    };
    for outcome in [Outcome::Ok, Outcome::Failed] {
        assert_eq!(outcome.slug(), listed(outcome), "{outcome:?}");
    }
}

#[test]
fn a_source_asked_for_at_no_branch_comes_from_its_url_alone() {
    for url in UrlMother::every_clean() {
        let source = SourceMother::create(&url, None);

        assert_eq!(source.origin(), OriginMother::source(&url, None), "{url}");
        assert_eq!(source.origin().as_str(), url, "{url}");
    }
}

#[test]
fn a_source_asked_for_at_a_branch_comes_from_its_url_and_the_branch_after_a_hash() {
    for _ in 0..DRAWS {
        let (url, branch) = (UrlMother::clean(), random::word());
        let source = SourceMother::create(&url, Some(&branch));

        assert_eq!(
            source.origin().as_str(),
            format!("{url}#{branch}"),
            "{url} at {branch}"
        );
        assert_eq!(
            source.origin(),
            OriginMother::source(&url, Some(&branch)),
            "{url} at {branch}"
        );
    }
}

/// The branch is put after the `#` as it was asked for, a slash or a dot
/// in it included, since a tag or a branch in folders is spelled so.
#[test]
fn a_branch_with_slashes_and_dots_is_kept_whole_in_the_origin() {
    let url = UrlMother::https();
    for branch in ["release/1.2", "v2.0.0", "feature/a-b/c"] {
        let source = SourceMother::create(&url, Some(branch));

        assert_eq!(source.origin().as_str(), format!("{url}#{branch}"));
    }
}

/// An origin is spelled from the redacted URL the source holds, so neither
/// what `--from` is offered nor what an install records repeats a
/// credential the URL was typed with.
#[test]
fn the_origin_of_a_source_typed_with_a_credential_holds_none() {
    for typed in UrlMother::every_credentialed() {
        let branch = random::chance().then(random::word);
        let source = SourceMother::create(&typed.typed, branch.as_deref());
        let origin = source.origin();

        assert!(!origin.contains(&typed.secret), "{typed:?} gave {origin}");
        assert_eq!(
            origin,
            OriginMother::source(&typed.clean, branch.as_deref()),
            "{typed:?}"
        );
    }
}

/// A source's origin is what an install of one of its own records, so it
/// says the same of the source as the URL serialized beside it.
#[test]
fn the_origin_of_a_source_begins_with_the_url_it_serializes() {
    for _ in 0..DRAWS {
        let source = SourceMother::random();
        let serialized: Value = serde_json::from_str(&json(&source)).unwrap();
        let url = serialized["url"].as_str().unwrap().to_owned();

        assert!(
            source.origin().starts_with(&url),
            "{source:?} serialized {url}"
        );
    }
}

#[test]
fn a_source_serializes_each_field_by_its_name_in_the_order_it_is_declared() {
    for _ in 0..DRAWS {
        let source = SourceMother::random();

        assert_eq!(json(&source), source_as_json(&source), "{source:?}");
    }
}

#[test]
fn a_source_serializes_exactly_these_fields() {
    let source = SourceMother::random();

    assert_eq!(
        keys(&json(&source)),
        [
            "kind",
            "url",
            "reference",
            "subdirectory",
            "revision",
            "directory",
            "name"
        ],
        "{source:?}"
    );
}

/// The row id is the ledger's, numbered as rows were written: two sources
/// that differ in nothing else are the same place to whoever reads them.
#[test]
fn a_source_s_row_id_is_never_serialized() {
    for _ in 0..DRAWS {
        let source = SourceMother::random();
        let renumbered = Source {
            id: source.id + random::between(1, 1000) as i64,
            ..source.clone()
        };

        assert_eq!(json(&source), json(&renumbered), "{source:?}");
        assert!(!keys(&json(&source)).contains(&String::from("id")));
    }
}

#[test]
fn a_source_that_was_never_gathered_says_so_with_nulls() {
    let source = Source {
        reference: None,
        revision: None,
        ..SourceMother::random()
    };
    let serialized: Value = serde_json::from_str(&json(&source)).unwrap();

    assert_eq!(serialized["reference"], Value::Null, "{source:?}");
    assert_eq!(serialized["revision"], Value::Null, "{source:?}");
}

#[test]
fn a_source_typed_with_any_credential_serializes_its_url_without_it() {
    for typed in UrlMother::every_credentialed() {
        let source = SourceMother::create(&typed.typed, None);
        let serialized = json(&source);

        assert!(!serialized.contains(&typed.secret), "{serialized}");
        assert!(
            serialized.contains(&format!(r#""url":{}"#, json(&typed.clean))),
            "{typed:?} gave {serialized}"
        );
    }
}

#[test]
fn a_gathered_artifact_serializes_each_field_by_its_name_its_source_nested() {
    for _ in 0..DRAWS {
        let kind = random::pick(&Kind::ALL);
        let gathered =
            GatheredMother::create(kind, ArtifactNameMother::random(), SourceMother::random());
        let expected = format!(
            r#"{{"kind":{},"name":{},"path":{},"summary":{},"revision":{},"gathered_at":{},"source":{}}}"#,
            json(kind.slug()),
            json(gathered.name.as_str()),
            json(&gathered.path),
            json(&gathered.summary),
            json(&gathered.revision.as_ref().map(|revision| revision.as_str())),
            gathered.gathered_at,
            source_as_json(&gathered.source),
        );

        assert_eq!(json(&gathered), expected, "{gathered:?}");
    }
}

#[test]
fn a_gathered_artifact_serializes_exactly_these_fields() {
    let gathered = GatheredMother::random();

    assert_eq!(
        keys(&json(&gathered)),
        [
            "kind",
            "name",
            "path",
            "summary",
            "revision",
            "gathered_at",
            "source"
        ],
        "{gathered:?}"
    );
}

/// A summary is read from somebody's front matter, so it may hold any
/// character: it reaches the JSON escaped and comes back as it was.
#[test]
fn a_gathered_summary_of_any_text_reads_back_as_it_was() {
    for _ in 0..DRAWS {
        let summary = random::text(40);
        let gathered = Gathered {
            summary: Some(summary.clone()),
            ..GatheredMother::random()
        };
        let serialized: Value = serde_json::from_str(&json(&gathered)).unwrap();

        assert_eq!(
            serialized["summary"],
            Value::from(summary.clone()),
            "{summary:?}"
        );
    }
}

#[test]
fn a_gathered_artifact_from_a_source_typed_with_a_credential_serializes_none() {
    for typed in UrlMother::every_credentialed() {
        let gathered = GatheredMother::create(
            Kind::Skill,
            ArtifactNameMother::random(),
            SourceMother::create(&typed.typed, Some("main")),
        );
        let serialized = json(&gathered);

        assert!(!serialized.contains(&typed.secret), "{serialized}");
    }
}

#[test]
fn an_entry_serializes_each_field_by_its_name_in_the_order_it_is_declared() {
    for _ in 0..DRAWS {
        let entry = EntryMother::random();
        let expected = format!(
            r#"{{"at":{},"action":{},"target":{},"outcome":{},"detail":{}}}"#,
            entry.at,
            json(&entry.action),
            json(&entry.target.as_ref().map(|target| target.as_str())),
            json(&entry.outcome),
            json(&entry.detail.as_ref().map(|detail| detail.as_str())),
        );

        assert_eq!(json(&entry), expected, "{entry:?}");
    }
}

#[test]
fn an_entry_serializes_exactly_these_fields() {
    assert_eq!(
        keys(&json(&EntryMother::random())),
        ["at", "action", "target", "outcome", "detail"]
    );
}

#[test]
fn an_entry_about_nothing_that_said_nothing_serializes_nulls() {
    let entry = Entry {
        target: None,
        detail: None,
        ..EntryMother::random()
    };
    let serialized: Value = serde_json::from_str(&json(&entry)).unwrap();

    assert_eq!(serialized["target"], Value::Null);
    assert_eq!(serialized["detail"], Value::Null);
}

/// A failed gather's entry names its URL and quotes it in what went wrong:
/// whatever built the entry, neither field prints the credential.
#[test]
fn an_entry_quoting_a_credentialed_url_anywhere_serializes_none() {
    for typed in UrlMother::every_credentialed() {
        let quoted = UrlMother::quoted(&typed);
        let entry = Entry {
            target: Some(RedactedMother::create(&typed.typed)),
            detail: Some(RedactedMother::create(&quoted.typed)),
            ..EntryMother::random()
        };
        let serialized: Value = serde_json::from_str(&json(&entry)).unwrap();

        assert!(!json(&entry).contains(&typed.secret), "{entry:?}");
        assert_eq!(serialized["target"], Value::from(typed.clean.clone()));
        assert_eq!(serialized["detail"], Value::from(quoted.clean.clone()));
    }
}

#[test]
fn each_ledger_error_of_its_own_gives_its_own_code() {
    for (error, code) in every_own_error("/w/.mindweaver/mindweaver.db") {
        assert_eq!(error.code(), code, "{error:?}");
    }
}

#[test]
fn the_codes_a_ledger_error_gives_of_its_own_are_listed_in_order() {
    let given: Vec<&str> = every_own_error("/w/.mindweaver/mindweaver.db")
        .into_iter()
        .map(|(_, code)| code)
        .collect();

    assert_eq!(LedgerError::CODES, given.as_slice());
}

/// A busy workspace met while opening the ledger to write is the lock's
/// condition, and says the lock's code, which the panel already knows, and
/// is no code of the ledger's own.
#[test]
fn a_ledger_error_carrying_the_lock_s_gives_the_lock_s_code() {
    for lock in [
        lock_busy("/w/.mindweaver/lock"),
        lock_unopened("/w/.mindweaver/lock"),
    ] {
        let code = lock.code();
        let error = LedgerError::from(lock);

        assert_eq!(error.code(), code, "{error:?}");
        assert!(!LedgerError::CODES.contains(&code), "{code}");
    }
}

#[test]
fn a_ledger_error_carrying_the_lock_s_says_what_the_lock_says() {
    let lock = lock_busy("/w/.mindweaver/lock");
    let said = lock.to_string();

    assert_eq!(LedgerError::Lock(lock).to_string(), said);
}

#[test]
fn every_ledger_error_names_the_file_it_is_about() {
    for _ in 0..20 {
        let path = format!("/{}/.mindweaver/{LEDGER_FILE}", random::word());
        for (error, _) in every_own_error(&path) {
            assert_eq!(error.path(), Path::new(&path), "{error:?}");
            assert!(error.to_string().starts_with(&path), "{error}");
        }
    }
}

#[test]
fn a_ledger_error_carrying_the_lock_s_names_the_lock_file() {
    let error = LedgerError::Lock(lock_unopened("/w/.mindweaver/lock"));

    assert_eq!(error.path(), Path::new("/w/.mindweaver/lock"));
}

#[test]
fn a_ledger_too_new_says_which_schema_it_holds_and_which_this_build_reads() {
    let found = SCHEMA_VERSION + random::between(1, 50) as u32;
    let error = LedgerError::Version {
        path: PathBuf::from("/w/.mindweaver/mindweaver.db"),
        found,
    };

    assert_eq!(
        error.to_string(),
        format!(
            "/w/.mindweaver/mindweaver.db: written by a newer Mindweaver \
             (schema version {found}, this one reads {SCHEMA_VERSION})"
        )
    );
}

#[test]
fn a_failed_query_says_what_it_was_doing_and_what_the_store_said() {
    let error = LedgerError::Query {
        path: PathBuf::from(":memory:"),
        doing: String::from("recording an installation"),
        source: Box::new(io::Error::other("disk I/O error")),
    };

    assert_eq!(
        error.to_string(),
        ":memory:: recording an installation failed: disk I/O error"
    );
}

#[test]
fn an_unmade_ledger_says_no_workspace_is_there_to_write_to() {
    let error = LedgerError::Unmade {
        path: PathBuf::from("/w/.mindweaver/mindweaver.db"),
    };

    assert_eq!(
        error.to_string(),
        "/w/.mindweaver/mindweaver.db: no workspace has been made here yet, \
         so there is no ledger to write to"
    );
}

#[test]
fn a_ledger_in_memory_is_kept_nowhere_and_says_so() {
    let ledger = Ledger::in_memory().unwrap();

    assert_eq!(ledger.path(), Path::new(":memory:"));
}

/// A store handed over is the one every question reaches, through the
/// facade's `Deref`, as the SQLite one opened by the facade itself is.
#[test]
fn a_ledger_over_a_store_handed_to_it_asks_that_store() {
    let ledger = Ledger::new(Box::new(Sqlite::in_memory().unwrap()));
    let record = InstallRecordMother::random();

    ledger.installed("app", &record).unwrap();

    assert_eq!(
        ledger
            .installation("app", record.kind, &record.name)
            .unwrap()
            .map(|found| found.name),
        Some(record.name.clone())
    );
    assert_eq!(ledger.path(), Path::new(":memory:"));
}

#[test]
fn a_ledger_debugs_as_the_store_it_holds() {
    let ledger = Ledger::in_memory().unwrap();

    assert!(
        format!("{ledger:?}").starts_with("Ledger(Sqlite"),
        "{ledger:?}"
    );
}

#[test]
fn a_relink_whose_write_succeeds_hands_back_what_it_wrote_and_every_installation_moved() {
    let ledger = Ledger::in_memory().unwrap();
    let id = ProjectIdMother::random();
    let count = random::between(0, 5);
    for _ in 0..count {
        ledger
            .installed("old", &InstallRecordMother::random())
            .unwrap();
    }
    let names = ledger.installations("old", Kind::Skill).unwrap();

    let moved = ledger
        .relinked("old", "new", &id, || Ok::<_, io::Error>(count * 10))
        .unwrap()
        .unwrap();

    assert_eq!(moved, (count * 10, names.len()), "{names:?}");
    assert_eq!(ledger.installations("new", Kind::Skill).unwrap(), names);
    assert!(ledger.installations("old", Kind::Skill).unwrap().is_empty());
    assert_eq!(ledger.project_at("new").unwrap(), Some(id));
}

#[test]
fn a_relink_whose_write_fails_hands_back_its_error_and_moves_no_record() {
    let ledger = Ledger::in_memory().unwrap();
    let id = ProjectIdMother::random();
    let record = InstallRecordMother::random();
    ledger.installed("old", &record).unwrap();
    let logged = ledger.history(100).unwrap().len();

    let refused = ledger
        .relinked("old", "new", &id, || Err::<(), _>(String::from("refused")))
        .unwrap();

    assert_eq!(refused, Err(String::from("refused")));
    assert!(ledger
        .installation("old", record.kind, &record.name)
        .unwrap()
        .is_some());
    assert_eq!(
        ledger
            .installation("new", record.kind, &record.name)
            .unwrap(),
        None
    );
    assert_eq!(ledger.project_at("new").unwrap(), None);
    assert_eq!(ledger.history(100).unwrap().len(), logged, "nothing logged");
}

/// The revision a mother draws stands for a commit; a test that pins a
/// source's JSON by it must see it written as it was given.
#[test]
fn a_source_s_revision_serializes_as_the_commit_it_names() {
    let revision = RevisionMother::random();
    let source = Source {
        revision: Some(revision.clone()),
        ..SourceMother::random()
    };
    let serialized: Value = serde_json::from_str(&json(&source)).unwrap();

    assert_eq!(serialized["revision"], Value::from(revision.as_str()));
}

#[test]
fn a_source_s_url_serializes_as_the_redaction_leaves_it() {
    for _ in 0..DRAWS {
        let url = random::pick(&[UrlMother::clean(), UrlMother::credentialed().typed]);
        let source = SourceMother::create(&url, None);
        let serialized: Value = serde_json::from_str(&json(&source)).unwrap();

        assert_eq!(
            serialized["url"],
            Value::from(crate::url::redacted(&url).into_owned()),
            "{url}"
        );
    }
}
