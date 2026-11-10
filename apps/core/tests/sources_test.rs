//! The sources a workspace gathers from, once gathered (#100): each listed
//! with what the shelf holds of it, renamed, gathered again and forgotten,
//! against a real workspace and real repositories.

mod common;

use std::fs;
use std::sync::Arc;

use common::{commit, repository, skill, url, workspace};
use mindweaver_core::gather::{self, GatherError, Request, Watch};
use mindweaver_core::ledger::{Action, FromSource, Ledger};
use mindweaver_core::sources::{self, Again, SourceError};
use mindweaver_core::value::SourceName;
use mindweaver_core::view::{KindCount, ShelfSource};
use mindweaver_core::{install, AgentHomes, Clock, Kind, MindProject, WeaverWorkspace};
use tempfile::TempDir;

/// A clock stopped at one moment, so what a listing says of when is known.
#[derive(Debug)]
struct Stopped(i64);

impl Clock for Stopped {
    fn now(&self) -> i64 {
        self.0
    }
}

/// A workspace whose ledger is stamped at `at`, beside the folder it is in.
fn stamped(at: i64) -> (TempDir, WeaverWorkspace, Ledger) {
    let (dir, workspace) = workspace();
    let ledger = workspace
        .ledger()
        .unwrap()
        .with_clock(Arc::new(Stopped(at)));
    (dir, workspace, ledger)
}

/// `deploy` and `review` in one repository.
fn two_skills() -> TempDir {
    repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/review/SKILL.md", &skill("review", "Read a change")),
    ])
}

/// `source` gathered into `workspace`, and what it is called.
fn gathered(workspace: &WeaverWorkspace, ledger: &Ledger, source: &TempDir) -> SourceName {
    gather::gather(workspace, ledger, &Request::git(url(source)))
        .unwrap()
        .name
}

fn name(text: &str) -> SourceName {
    SourceName::parse(text).unwrap()
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

#[test]
fn each_source_is_listed_with_what_the_shelf_holds_of_it_and_when_it_was_gathered() {
    let (_dir, workspace, ledger) = stamped(1_790_000_000);
    let source = two_skills();
    let called = gathered(&workspace, &ledger, &source);

    let listed = ShelfSource::read(&ledger).unwrap();

    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].source.name, called);
    assert_eq!(
        listed[0].shelved,
        [KindCount {
            kind: Kind::Skill,
            count: 2
        }]
    );
    assert_eq!(listed[0].gathered_at, Some(1_790_000_000));
    assert_eq!(listed[0].origin, listed[0].source.origin());
}

#[test]
fn a_source_whose_gather_took_nothing_is_listed_with_nothing_on_the_shelf_and_no_time() {
    let (_dir, workspace, ledger) = stamped(1_790_000_000);
    let empty = repository(&[("README.md", "no skills here\n")]);
    let refused = gather::gather(&workspace, &ledger, &Request::git(url(&empty))).unwrap_err();
    assert!(
        matches!(refused, GatherError::NothingFound { .. }),
        "{refused}"
    );

    let listed = ShelfSource::read(&ledger).unwrap();

    assert_eq!(listed.len(), 1, "{listed:?}");
    assert!(listed[0].shelved.is_empty(), "{listed:?}");
    assert_eq!(listed[0].gathered_at, None);
}

// ---------------------------------------------------------------------------
// Choosing and renaming
// ---------------------------------------------------------------------------

#[test]
fn no_name_chooses_every_source_and_names_choose_each_once_in_the_order_given() {
    let (_dir, workspace, ledger) = stamped(1);
    let (first, second) = (two_skills(), two_skills());
    let one = gathered(&workspace, &ledger, &first);
    let other = gathered(&workspace, &ledger, &second);

    let every = sources::chosen(&ledger, &[]).unwrap();
    let named = sources::chosen(
        &ledger,
        &[other.to_string(), one.to_string(), other.to_string()],
    )
    .unwrap();

    let names = |chosen: &[mindweaver_core::ledger::Source]| -> Vec<SourceName> {
        chosen.iter().map(|source| source.name.clone()).collect()
    };
    assert_eq!(
        names(&every),
        names(&ledger.sources().unwrap()),
        "every source, as the ledger lists them"
    );
    assert_eq!(names(&named), [other, one]);
}

#[test]
fn a_name_no_source_has_is_refused_before_anything_is_chosen() {
    let (_dir, workspace, ledger) = stamped(1);
    let called = gathered(&workspace, &ledger, &two_skills());

    for typed in ["nobody", "not a name", "-odd", ""] {
        let refused =
            sources::chosen(&ledger, &[called.to_string(), typed.to_owned()]).unwrap_err();

        assert_eq!(refused.code(), "source_unknown", "{typed}");
        assert!(
            refused.to_string().contains(&format!("`{typed}`")),
            "{refused}"
        );
    }
}

#[test]
fn a_renamed_source_is_found_by_its_new_name_and_its_url_alone() {
    let (_dir, workspace, ledger) = stamped(1);
    let called = gathered(&workspace, &ledger, &two_skills());

    let renamed = sources::rename(&workspace, &ledger, called.as_str(), "team-skills").unwrap();

    assert_eq!(renamed.was, called);
    assert_eq!(renamed.source.name, name("team-skills"));
    assert_eq!(ledger.source_named(&called).unwrap(), None);
    let found = ledger.source_named(&name("team-skills")).unwrap().unwrap();
    assert_eq!(found.id, renamed.source.id);
    assert_eq!(found.origin(), renamed.source.origin());
}

#[test]
fn renaming_a_source_to_its_own_name_changes_nothing() {
    let (_dir, workspace, ledger) = stamped(1);
    let called = gathered(&workspace, &ledger, &two_skills());

    let renamed = sources::rename(&workspace, &ledger, called.as_str(), called.as_str()).unwrap();

    assert_eq!(renamed.was, called);
    assert_eq!(renamed.source.name, called);
}

#[test]
fn a_name_another_source_has_or_none_can_have_is_refused_by_the_gathers_codes() {
    let (_dir, workspace, ledger) = stamped(1);
    let one = gathered(&workspace, &ledger, &two_skills());
    let other = gathered(&workspace, &ledger, &two_skills());

    let taken = sources::rename(&workspace, &ledger, one.as_str(), other.as_str()).unwrap_err();
    let invalid = sources::rename(&workspace, &ledger, one.as_str(), "no/slash").unwrap_err();
    let unknown = sources::rename(&workspace, &ledger, "nobody", "anybody").unwrap_err();

    assert_eq!(taken.code(), "source_name_taken", "{taken}");
    assert_eq!(invalid.code(), "source_name_invalid", "{invalid}");
    assert_eq!(unknown.code(), "source_unknown", "{unknown}");
    assert_eq!(ledger.source_named(&one).unwrap().unwrap().name, one);
}

// ---------------------------------------------------------------------------
// Gathering again
// ---------------------------------------------------------------------------

#[test]
fn gathering_sources_again_says_what_each_added_updated_and_removed() {
    let (_dir, workspace, ledger) = stamped(1);
    let source = two_skills();
    gathered(&workspace, &ledger, &source);
    fs::remove_dir_all(source.path().join("skills/review")).unwrap();
    common::write_files(
        source.path(),
        &[
            ("skills/deploy/SKILL.md", &skill("deploy", "Ship it, now")),
            ("skills/lint/SKILL.md", &skill("lint", "Tidy it")),
        ],
    );
    commit(source.path());

    let again = again_as_recorded(&workspace, &ledger);

    assert_eq!(again.len(), 1);
    let Again::Gathered(report) = &again[0].again else {
        panic!("{:?}", again[0].again);
    };
    let names = |harvested: &[gather::Harvested]| -> Vec<String> {
        harvested.iter().map(|one| one.name.to_string()).collect()
    };
    assert_eq!(names(&report.added), ["lint"]);
    assert_eq!(names(&report.updated), ["deploy"]);
    assert_eq!(names(&report.removed), ["review"]);
    assert_eq!(ledger.sources().unwrap().len(), 1, "the same source");
}

#[test]
fn a_pinned_source_is_gathered_again_at_its_pin() {
    let (_dir, workspace, ledger) = stamped(1);
    let source = two_skills();
    let pin = gix::open(source.path())
        .unwrap()
        .head_id()
        .unwrap()
        .to_string();
    gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).pinned(Some(pin.clone())),
    )
    .unwrap();
    fs::remove_dir_all(source.path().join("skills/review")).unwrap();
    commit(source.path());

    let again = again_as_recorded(&workspace, &ledger);

    let Again::Gathered(report) = &again[0].again else {
        panic!("{:?}", again[0].again);
    };
    assert!(report.removed.is_empty(), "{report:?}");
    assert_eq!(report.unchanged.len(), 2, "{report:?}");
    assert_eq!(report.revision.as_deref(), Some(pin.as_str()));
}

#[test]
fn a_source_that_fails_is_said_and_the_next_is_gathered_all_the_same() {
    let (_dir, workspace, ledger) = stamped(1);
    let (first, second) = (two_skills(), two_skills());
    gathered(&workspace, &ledger, &first);
    gathered(&workspace, &ledger, &second);
    let sources = ledger.sources().unwrap();
    let failing = sources[0].id;

    let again = sources::update(&workspace, &ledger, sources, |source, kind| {
        if source.id == failing {
            Err(GatherError::TokenNotHttps {
                url: source.url.to_string(),
            })
        } else {
            Ok(Request::again(source, kind))
        }
    })
    .unwrap();

    assert!(
        matches!(
            again[0].again,
            Again::Failed(GatherError::TokenNotHttps { .. })
        ),
        "{:?}",
        again[0].again
    );
    assert!(
        matches!(again[1].again, Again::Gathered(_)),
        "{:?}",
        again[1].again
    );
}

#[test]
fn a_cancelled_source_stops_the_update_and_those_after_it_are_not_tried() {
    let (_dir, workspace, ledger) = stamped(1);
    let (first, second) = (two_skills(), two_skills());
    gathered(&workspace, &ledger, &first);
    gathered(&workspace, &ledger, &second);
    let watch = Watch::new();
    watch.cancel();

    let again = sources::update(
        &workspace,
        &ledger,
        ledger.sources().unwrap(),
        |source, kind| Ok(Request::again(source, kind).watched_by(watch.clone())),
    )
    .unwrap();

    assert!(
        matches!(again[0].again, Again::Failed(GatherError::Cancelled)),
        "{:?}",
        again[0].again
    );
    assert!(
        matches!(again[1].again, Again::Unreached),
        "{:?}",
        again[1].again
    );
}

/// Every source of `ledger` gathered again, each kind of it as the ledger
/// records it.
fn again_as_recorded(workspace: &WeaverWorkspace, ledger: &Ledger) -> Vec<sources::Regathered> {
    sources::update(
        workspace,
        ledger,
        ledger.sources().unwrap(),
        |source, kind| Ok(Request::again(source, kind)),
    )
    .unwrap()
}

#[test]
fn a_source_of_rules_is_gathered_again_as_rules_from_its_folder() {
    let (_dir, workspace, ledger) = stamped(1);
    let source = repository(&[
        ("rules/git/commits.md", "# Commits\n\nOne step at a time.\n"),
        ("rules/style.md", "# Style\n\nKeep it short.\n"),
    ]);
    gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source))
            .taking(Kind::Rule)
            .from_subdirectory("rules"),
    )
    .unwrap();
    fs::remove_file(source.path().join("rules/git/commits.md")).unwrap();
    commit(source.path());

    let again = again_as_recorded(&workspace, &ledger);

    assert_eq!(again.len(), 1, "{again:?}");
    assert_eq!(again[0].kind, Kind::Rule);
    let Again::Gathered(report) = &again[0].again else {
        panic!("{:?}", again[0].again);
    };
    let removed: Vec<&str> = report
        .removed
        .iter()
        .map(|gone| gone.name.as_str())
        .collect();
    assert_eq!(removed, ["git/commits"]);
    let shelf = workspace.gathered_dir(Kind::Rule).join(&report.directory);
    assert!(!shelf.join("git").exists(), "no folder of nothing is left");
    assert!(shelf.join("style.md").is_file());
}

// ---------------------------------------------------------------------------
// Forgetting
// ---------------------------------------------------------------------------

/// A workspace that gathered [`two_skills`] and installed `deploy` from it
/// into `collapse`, and what the source is called.
fn installed_from_a_source() -> (TempDir, TempDir, WeaverWorkspace, Ledger, SourceName) {
    let source = two_skills();
    let (dir, mut workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let called = gathered(&workspace, &ledger, &source);
    let (project, _) = MindProject::init(dir.path().join("collapse")).unwrap();
    workspace.link(&project).unwrap();
    let candidate =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();
    (source, dir, workspace, ledger, called)
}

#[test]
fn a_removal_says_what_it_would_take_away_and_leave_orphaned_and_takes_nothing() {
    let (_source, _dir, workspace, ledger, called) = installed_from_a_source();
    let found = ledger.source_named(&called).unwrap().unwrap();

    let removal = sources::removal(&workspace, &ledger, called.as_str()).unwrap();

    let clone = workspace.cache_dir().join(&found.directory);
    let shelf = workspace.gathered_dir(Kind::Skill).join(&found.directory);
    assert_eq!(removal.source, found);
    assert_eq!(removal.clone.as_ref(), Some(&clone));
    assert_eq!(removal.shelves, [(Kind::Skill, shelf.clone())]);
    assert_eq!(
        removal.shelved,
        [KindCount {
            kind: Kind::Skill,
            count: 2
        }]
    );
    assert_eq!(
        removal.installed,
        [FromSource {
            project: String::from("collapse"),
            kind: Kind::Skill,
            name: "deploy".into(),
        }]
    );
    assert!(clone.is_dir() && shelf.is_dir());
    assert_eq!(ledger.gathered().unwrap().len(), 2);
}

#[test]
fn removing_a_source_takes_its_folders_and_rows_and_leaves_its_installs_recorded() {
    let (_source, _dir, workspace, ledger, called) = installed_from_a_source();
    let found = ledger.source_named(&called).unwrap().unwrap();
    let before = ledger
        .installation("collapse", Kind::Skill, "deploy")
        .unwrap()
        .unwrap();

    let removed = sources::remove(&workspace, &ledger, called.as_str()).unwrap();

    assert_eq!(removed.installed.len(), 1, "{removed:?}");
    assert!(!workspace.cache_dir().join(&found.directory).exists());
    assert!(!workspace
        .gathered_dir(Kind::Skill)
        .join(&found.directory)
        .exists());
    assert!(ledger.sources().unwrap().is_empty());
    assert!(ledger.gathered().unwrap().is_empty());
    let after = ledger
        .installation("collapse", Kind::Skill, "deploy")
        .unwrap()
        .expect("the installation is still recorded");
    assert_eq!(after.source_id, None);
    assert_eq!(after.origin, before.origin);
    assert_eq!(after.targets, before.targets);
    let logged = &ledger.history(1).unwrap()[0];
    assert_eq!(logged.action, Action::Forget.slug());
    assert_eq!(logged.detail.as_deref(), Some(called.as_str()));
}

#[test]
fn a_source_no_one_has_called_that_is_neither_planned_nor_removed() {
    let (_source, _dir, workspace, ledger, _called) = installed_from_a_source();

    for refused in [
        sources::removal(&workspace, &ledger, "nobody").unwrap_err(),
        sources::remove(&workspace, &ledger, "nobody").unwrap_err(),
    ] {
        assert!(matches!(refused, SourceError::Unknown { .. }), "{refused}");
    }
    assert_eq!(ledger.sources().unwrap().len(), 1);
}

#[cfg(unix)]
#[test]
fn a_shelf_folder_that_is_a_link_is_refused_and_nothing_is_removed() {
    let (_source, dir, workspace, ledger, called) = installed_from_a_source();
    let found = ledger.source_named(&called).unwrap().unwrap();
    let shelf = workspace.gathered_dir(Kind::Skill).join(&found.directory);
    let elsewhere = dir.path().join("elsewhere");
    fs::rename(&shelf, &elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &shelf).unwrap();

    let refused = sources::remove(&workspace, &ledger, called.as_str()).unwrap_err();

    assert_eq!(refused.code(), "source_folder_linked", "{refused}");
    assert!(workspace.cache_dir().join(&found.directory).is_dir());
    assert!(elsewhere.join("deploy").join("SKILL.md").is_file());
    assert_eq!(ledger.sources().unwrap().len(), 1);
}
