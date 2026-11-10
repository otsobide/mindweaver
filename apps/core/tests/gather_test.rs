//! Gathering skills out of a real git repository, against a real workspace.

mod common;

use std::fs;
use std::sync::{Arc, Mutex};

use common::{commit, repository, skill, url, workspace, write_files};
use mindweaver_core::gather::{self, GatherError, Progress, Request, Warning, Watch};
use mindweaver_core::ledger::{Action, Ledger, SourceKind};
use mindweaver_core::value::SourceName;
use mindweaver_core::{Kind, MindProject, ValidationIssue, WeaverWorkspace};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn gathering_puts_a_repositorys_skills_on_the_workspace_shelf() {
    let source = repository(&[
        (
            "skills/commit-style/SKILL.md",
            &skill("commit-style", "How we commit"),
        ),
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("README.md", "not a skill"),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    assert_eq!(report.added.len(), 2, "{:?}", report.added);
    assert!(report.updated.is_empty() && report.unchanged.is_empty());
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(report.revision.is_some(), "a clone resolves a commit");

    let shelf = workspace.gathered_dir(Kind::Skill).join(&report.directory);
    assert!(shelf.join("commit-style").join("SKILL.md").is_file());
    assert!(shelf.join("deploy").join("SKILL.md").is_file());
    // Only the skills folder was harvested.
    assert!(!shelf.join("README.md").exists());
}

#[test]
fn a_skills_whole_directory_travels_with_it() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/deploy/scripts/run.sh", "echo hi\n"),
        ("skills/deploy/reference.md", "detail\n"),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let deploy = workspace
        .gathered_dir(Kind::Skill)
        .join(&report.directory)
        .join("deploy");
    assert!(deploy.join("scripts").join("run.sh").is_file());
    assert!(deploy.join("reference.md").is_file());
}

#[test]
fn gathering_the_same_repository_again_changes_nothing() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request = Request::git(url(&source));

    gather::gather(&workspace, &ledger, &request).unwrap();
    let again = gather::gather(&workspace, &ledger, &request).unwrap();

    assert_eq!(again.unchanged.len(), 1);
    assert!(again.added.is_empty() && again.updated.is_empty());
    // One shelf entry, not two: the second gather is the same source.
    assert_eq!(ledger.gathered().unwrap().len(), 1);
}

#[test]
fn a_skill_that_changed_upstream_is_reported_as_updated() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request = Request::git(url(&source));
    gather::gather(&workspace, &ledger, &request).unwrap();

    write_files(
        source.path(),
        &[(
            "skills/deploy/SKILL.md",
            &skill("deploy", "Ship it, carefully"),
        )],
    );
    commit(source.path());

    let again = gather::gather(&workspace, &ledger, &request).unwrap();

    assert_eq!(again.updated.len(), 1, "{again:?}");
    assert_eq!(
        ledger.gathered().unwrap()[0].summary.as_deref(),
        Some("Ship it, carefully")
    );
}

#[test]
fn a_file_the_source_deleted_does_not_survive_on_the_shelf() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/deploy/old.md", "leftover\n"),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request = Request::git(url(&source));
    gather::gather(&workspace, &ledger, &request).unwrap();

    fs::remove_file(source.path().join("skills/deploy/old.md")).unwrap();
    commit(source.path());
    let report = gather::gather(&workspace, &ledger, &request).unwrap();

    let deploy = workspace
        .gathered_dir(Kind::Skill)
        .join(&report.directory)
        .join("deploy");
    assert!(deploy.join("SKILL.md").is_file());
    assert!(!deploy.join("old.md").exists());
}

#[test]
fn two_repositories_offering_one_name_both_survive() {
    let first = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "From the first"))]);
    let second = repository(&[(
        "skills/deploy/SKILL.md",
        &skill("deploy", "From the second"),
    )]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let one = gather::gather(&workspace, &ledger, &Request::git(url(&first))).unwrap();
    let two = gather::gather(&workspace, &ledger, &Request::git(url(&second))).unwrap();

    assert_ne!(
        one.directory, two.directory,
        "each source gets its own shelf"
    );
    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 2);
    assert!(gathered.iter().all(|entry| entry.name == "deploy"));
    let summaries: Vec<Option<&str>> = gathered.iter().map(|g| g.summary.as_deref()).collect();
    assert!(summaries.contains(&Some("From the first")));
    assert!(summaries.contains(&Some("From the second")));
}

#[test]
fn one_unreadable_skill_does_not_cost_the_ones_beside_it() {
    let source = repository(&[
        ("skills/good/SKILL.md", &skill("good", "Fine")),
        ("skills/broken/SKILL.md", "no front matter at all\n"),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    assert_eq!(report.added.len(), 1);
    assert_eq!(report.added[0].name, "good");
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].to_string().contains("front matter"));
}

#[test]
fn a_skill_that_breaks_the_rules_is_gathered_with_a_warning() {
    // Lenient, as the Agent Skills client guide advises for skills written
    // for other agents: each problem is said, and nothing is left behind.
    let source = repository(&[
        ("skills/a--b/SKILL.md", &skill("a--b", "Doubled")),
        (
            "skills/nested/SKILL.md",
            "---\nname: nested\ndescription: Nested metadata\nmetadata:\n  owner:\n    team: ops\n---\n",
        ),
        ("skills/fine/SKILL.md", &skill("fine", "Nothing wrong")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let names: Vec<&str> = report.added.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["a--b", "fine", "nested"]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let shelf = workspace.gathered_dir(Kind::Skill).join(&report.directory);
    assert_eq!(
        report.warnings,
        vec![
            Warning {
                path: shelf.join("a--b").join("SKILL.md"),
                issue: ValidationIssue::NameNotKebabCase {
                    segment: "a--b".into()
                },
            },
            Warning {
                path: shelf.join("nested").join("SKILL.md"),
                issue: ValidationIssue::MetadataNotAString {
                    key: "owner".into()
                },
            },
        ]
    );
    // What is ignored is not rewritten: the shelf holds the file as written.
    // Line endings are left out of the comparison because the checkout, not
    // Mindweaver, decides them: on Windows git's autocrlf turns them into CRLF.
    let copied = fs::read_to_string(shelf.join("nested").join("SKILL.md")).unwrap();
    let copied = copied.replace("\r\n", "\n");
    assert!(copied.contains("  owner:\n    team: ops\n"), "{copied}");
}

#[test]
fn a_compatibility_written_as_a_list_is_gathered_with_a_warning() {
    // A list of agents is what an author writes when the specification's
    // prose string does not occur to them; the skill is still a skill.
    let source = repository(&[(
        "skills/deploy/SKILL.md",
        "---\nname: deploy\ndescription: Ship it\ncompatibility: [claude-code, codex]\n---\n",
    )]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let names: Vec<&str> = report.added.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["deploy"]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let shelf = workspace.gathered_dir(Kind::Skill).join(&report.directory);
    assert_eq!(
        report.warnings,
        vec![Warning {
            path: shelf.join("deploy").join("SKILL.md"),
            issue: ValidationIssue::CompatibilityNotAString,
        }]
    );
}

#[test]
fn a_directory_without_a_manifest_is_not_a_skill() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/shared/logo.png", "not a skill\n"),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    assert_eq!(report.added.len(), 1);
    assert!(report.failures.is_empty());
}

#[test]
fn another_folder_can_be_named_instead_of_skills() {
    let source = repository(&[("agents/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let request = Request::git(url(&source)).from_subdirectory("agents");
    let report = gather::gather(&workspace, &ledger, &request).unwrap();

    assert_eq!(report.added.len(), 1);
}

#[test]
fn a_folder_the_source_does_not_have_is_named_in_the_error() {
    let source = repository(&[("README.md", "nothing to gather\n")]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let error = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap_err();

    assert!(matches!(error, GatherError::NoSuchFolder { .. }), "{error}");
    assert!(error.to_string().contains("skills"));
}

#[test]
fn a_gather_that_finds_no_folder_leaves_the_revision_its_shelf_came_from() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    let before = ledger
        .source(SourceKind::Git, &url(&source), None, "skills")
        .unwrap();

    fs::remove_dir_all(source.path().join("skills")).unwrap();
    write_files(source.path(), &[("README.md", "moved elsewhere\n")]);
    commit(source.path());
    let error = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap_err();

    assert!(matches!(error, GatherError::NoSuchFolder { .. }), "{error}");
    let after = ledger
        .source(SourceKind::Git, &url(&source), None, "skills")
        .unwrap();
    assert_eq!(
        after.map(|source| source.revision),
        before.map(|source| source.revision),
        "the shelf still holds what that revision gave it"
    );
}

#[test]
fn rules_cannot_be_gathered_yet_and_say_so() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let mut request = Request::git(url(&source));
    request.kind = Kind::Rule;
    let error = gather::gather(&workspace, &ledger, &request).unwrap_err();

    assert!(
        matches!(error, GatherError::NotGatherable { .. }),
        "{error}"
    );
}

#[test]
fn the_log_records_a_gather_and_the_source_it_came_from() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let address = url(&source);

    gather::gather(&workspace, &ledger, &Request::git(address.clone())).unwrap();

    let history = ledger.history(10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].action, Action::Gather.slug());
    assert_eq!(history[0].target.as_deref(), Some(address.as_str()));
    assert_eq!(history[0].outcome, "ok");

    let recorded = ledger
        .source(SourceKind::Git, &address, None, "skills")
        .unwrap()
        .expect("the source was registered");
    assert!(recorded.revision.is_some());
}

#[test]
fn a_gather_that_fails_is_logged_as_a_failure() {
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let request = Request::git("/does/not/exist/anywhere");
    assert!(gather::gather(&workspace, &ledger, &request).is_err());

    let history = ledger.history(10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].outcome, "failed");
    assert!(history[0].detail.is_some());
}

#[test]
fn gathering_writes_nothing_into_a_mind_project() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    // A workspace that is also a project, which is the arrangement where a
    // gather leaking into `.mind` would be easiest to miss.
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let (project, _) = MindProject::init(dir.path()).unwrap();
    let ledger = workspace.ledger().unwrap();

    gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let skills = project.directory_for(Kind::Skill).unwrap();
    assert!(skills.is_dir(), "init made it");
    assert_eq!(
        fs::read_dir(&skills).unwrap().count(),
        0,
        "gathering fills the workspace shelf and stops there"
    );
}

#[test]
fn the_clone_is_kept_so_a_gather_can_be_looked_at_afterwards() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let clone = workspace.cache_dir().join(&report.directory);
    assert!(clone
        .join("skills")
        .join("deploy")
        .join("SKILL.md")
        .is_file());
}

#[test]
fn a_gather_reports_reading_every_folder_before_placing_any() {
    let source = repository(&[
        ("skills/broken/SKILL.md", "no front matter at all\n"),
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let (watch, seen) = recording();

    gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).watched_by(watch),
    )
    .unwrap();

    let seen = seen.lock().unwrap();
    // What `gix` counts while it fetches depends on how fast the clone is,
    // so only the steps every gather says are compared whole.
    let said: Vec<Progress> = seen
        .iter()
        .copied()
        .filter(|progress| {
            matches!(
                progress,
                Progress::Connecting
                    | Progress::CheckingOut { total: None, .. }
                    | Progress::Harvesting { .. }
                    | Progress::Placing { .. }
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            Progress::Connecting,
            Progress::CheckingOut {
                done: 0,
                total: None
            },
            Progress::Harvesting { done: 0, total: 2 },
            Progress::Harvesting { done: 1, total: 2 },
            Progress::Harvesting { done: 2, total: 2 },
            // The broken one was read, and is not placed.
            Progress::Placing { done: 0, total: 1 },
            Progress::Placing { done: 1, total: 1 },
        ]
    );
    // And what it does count is said while it fetches, never after.
    let reading = seen
        .iter()
        .position(|progress| matches!(progress, Progress::Harvesting { .. }))
        .unwrap();
    assert!(seen[reading..].iter().all(|progress| matches!(
        progress,
        Progress::Harvesting { .. } | Progress::Placing { .. }
    )));
}

#[test]
fn a_flag_set_before_checkout_leaves_no_shelf_change_and_no_ledger_rows() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let watch =
        cancelled_at(|progress| matches!(progress, Progress::CheckingOut { total: None, .. }));

    let error = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).watched_by(watch),
    )
    .unwrap_err();

    assert!(matches!(error, GatherError::Cancelled), "{error}");
    assert!(!workspace.gathered_dir(Kind::Skill).exists(), "no shelf");
    assert!(ledger.gathered().unwrap().is_empty());
    assert!(ledger.history(10).unwrap().is_empty(), "nothing logged");
    assert_eq!(
        ledger
            .source(SourceKind::Git, &url(&source), None, "skills")
            .unwrap(),
        None,
        "the source it registered is forgotten"
    );
    // And the clone it had begun is gone with it.
    assert_eq!(fs::read_dir(workspace.cache_dir()).unwrap().count(), 0);
}

#[test]
fn a_cancelled_gather_of_a_known_source_leaves_its_shelf_and_rows_as_they_were() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let first = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    let shelf = workspace.gathered_dir(Kind::Skill).join(&first.directory);
    let before = fs::read_to_string(shelf.join("deploy/SKILL.md")).unwrap();
    let recorded = ledger
        .source(SourceKind::Git, &url(&source), None, "skills")
        .unwrap();

    write_files(
        source.path(),
        &[
            (
                "skills/deploy/SKILL.md",
                &skill("deploy", "Ship it, carefully"),
            ),
            ("skills/review/SKILL.md", &skill("review", "Look first")),
        ],
    );
    commit(source.path());
    // As late as a cancel is honoured: every folder read, nothing placed.
    let watch = cancelled_at(
        |progress| matches!(progress, Progress::Harvesting { done, total } if done == total),
    );
    let error = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).watched_by(watch),
    )
    .unwrap_err();

    assert!(matches!(error, GatherError::Cancelled), "{error}");
    assert_eq!(
        fs::read_to_string(shelf.join("deploy/SKILL.md")).unwrap(),
        before
    );
    assert!(!shelf.join("review").exists());
    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 1);
    assert_eq!(gathered[0].summary.as_deref(), Some("Ship it"));
    assert_eq!(
        ledger
            .source(SourceKind::Git, &url(&source), None, "skills")
            .unwrap(),
        recorded,
        "the revision it was gathered at is the one it still has"
    );
    assert_eq!(ledger.history(10).unwrap().len(), 1, "the first gather");
}

#[test]
fn a_gather_cancelled_before_it_starts_fetches_nothing() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let (watch, seen) = recording();
    watch.cancel();

    let error = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).watched_by(watch),
    )
    .unwrap_err();

    assert!(matches!(error, GatherError::Cancelled), "{error}");
    assert!(seen.lock().unwrap().is_empty(), "not even a connection");
    assert!(!workspace.cache_dir().exists());
    assert!(ledger.history(10).unwrap().is_empty());
}

#[test]
fn a_cancel_once_placing_has_begun_comes_too_late_to_stop_it() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/review/SKILL.md", &skill("review", "Look first")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let watch = cancelled_at(|progress| matches!(progress, Progress::Placing { done: 0, .. }));

    let report = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).watched_by(watch),
    )
    .unwrap();

    // Half a shelf would be worse than a whole one.
    assert_eq!(report.added.len(), 2);
    assert_eq!(ledger.gathered().unwrap().len(), 2);
}

#[test]
fn a_cancel_stops_a_gather_at_every_step_said_to_be_cancellable_and_no_other() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/review/SKILL.md", &skill("review", "Look first")),
    ]);
    let (watch, seen) = recording();
    {
        let (_dir, workspace) = workspace();
        let ledger = workspace.ledger().unwrap();
        gather::gather(
            &workspace,
            &ledger,
            &Request::git(url(&source)).watched_by(watch),
        )
        .unwrap();
    }
    // Only the steps every gather says: `gix`'s counts depend on how fast
    // the clone is, so a cancel at one of them could not be made to land.
    let steps: Vec<Progress> = seen
        .lock()
        .unwrap()
        .iter()
        .copied()
        .filter(|progress| {
            !matches!(
                progress,
                Progress::Receiving { .. }
                    | Progress::Resolving { .. }
                    | Progress::CheckingOut { total: Some(_), .. }
            )
        })
        .collect();
    assert!(steps.iter().any(|step| !step.cancellable()), "{steps:?}");

    for step in steps {
        let (_dir, workspace) = workspace();
        let ledger = workspace.ledger().unwrap();
        let outcome = gather::gather(
            &workspace,
            &ledger,
            &Request::git(url(&source)).watched_by(cancelled_at(move |progress| progress == step)),
        );

        assert_eq!(
            matches!(outcome, Err(GatherError::Cancelled)),
            step.cancellable(),
            "cancelled at {step:?}: {outcome:?}"
        );
    }
}

#[test]
fn the_ledger_lands_beside_the_marker_file() {
    let (dir, workspace) = workspace();
    let ledger = Ledger::open(workspace.ledger_path()).unwrap();
    drop(ledger);

    assert!(dir
        .path()
        .join(".mindweaver")
        .join("mindweaver.db")
        .is_file());
}

#[test]
fn a_url_with_a_token_in_it_is_refused_and_the_token_is_kept_nowhere() {
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let error = gather::gather(
        &workspace,
        &ledger,
        &Request::git("https://me:ghp_t0ken@example.com/acme/skills"),
    )
    .unwrap_err();

    assert!(
        matches!(error, GatherError::CredentialsInUrl { .. }),
        "{error}"
    );
    assert_eq!(
        error.to_string(),
        "https://example.com/acme/skills: a password, a token or, over http(s), a user name in \
         the URL is refused, because gathering stores and shows the URL: gather it without one, \
         and configure a git credential helper that has it"
    );
    // Nothing was registered, and nothing was fetched into the cache.
    assert!(ledger
        .source(
            SourceKind::Git,
            "https://example.com/acme/skills",
            None,
            "skills"
        )
        .unwrap()
        .is_none());
    assert!(!workspace.cache_dir().exists());
    // The attempt is logged, as every failed gather is, under the URL it was
    // refused for.
    let history = ledger.history(10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(
        history[0].target.as_deref(),
        Some("https://example.com/acme/skills")
    );
    // And the token is nowhere in the database file, whatever reads it.
    let stored = fs::read(workspace.ledger_path()).unwrap();
    assert!(!stored.windows(9).any(|bytes| bytes == b"ghp_t0ken"));
}

#[test]
fn a_password_cut_by_a_slash_is_refused_too_and_kept_nowhere() {
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    // No parser reads this as a URL with a password in it, which is why it
    // has to be caught before anything parses it.
    let error = gather::gather(
        &workspace,
        &ledger,
        &Request::git("https://me:p/t0ken@example.invalid/acme/skills"),
    )
    .unwrap_err();

    assert!(
        matches!(error, GatherError::CredentialsInUrl { .. }),
        "{error}"
    );
    assert!(!error.to_string().contains("t0ken"), "{error}");
    let stored = fs::read(workspace.ledger_path()).unwrap();
    assert!(!stored.windows(5).any(|bytes| bytes == b"t0ken"));
    assert!(!workspace.cache_dir().exists());
}

/// Leave in `workspace` what a Mindweaver from before the refusal left after
/// gathering from `url` with a token in it: the URL in the source the clone
/// in `directory` belongs to, in the log, and in that clone's config and
/// reflog.
fn left_by_an_older_build(workspace: &WeaverWorkspace, directory: &str, url: &str) {
    let database = rusqlite::Connection::open(workspace.ledger_path()).unwrap();
    database
        .execute("UPDATE sources SET url = ?1", [url])
        .unwrap();
    database
        .execute(
            "INSERT INTO actions (at, action, target, outcome, detail)
             VALUES (0, 'gather', ?1, 'failed', ?2)",
            [url, &format!("{url}: cannot be fetched")],
        )
        .unwrap();

    let git = workspace.cache_dir().join(directory).join(".git");
    let config = fs::read_to_string(git.join("config")).unwrap();
    let config: String = config
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("url = ") {
                format!("\turl = {url}\n")
            } else {
                format!("{line}\n")
            }
        })
        .collect();
    fs::write(git.join("config"), config).unwrap();
    let mut reflog = fs::read_to_string(git.join("logs").join("HEAD")).unwrap();
    reflog.push_str(&format!(
        "0000 1111 Fixture <fixture@example.com> 0 +0000\tclone: from {url}\n"
    ));
    fs::write(git.join("logs").join("HEAD"), reflog).unwrap();
}

/// Every file under `root` that reads as text, joined, for asserting on.
fn text_under(root: &std::path::Path) -> String {
    let mut text = String::new();
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            text.push_str(&text_under(&path));
        } else if let Ok(contents) = fs::read_to_string(&path) {
            text.push_str(&contents);
        }
    }
    text
}

#[test]
fn what_an_older_build_stored_with_a_token_is_scrubbed_once_from_the_ledger_and_the_clone() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    left_by_an_older_build(
        &workspace,
        &report.directory,
        "https://me:ghp_t0ken@example.com/acme/skills",
    );

    // Opened the way a command that writes opens it.
    let (ledger, scrub) = workspace.ledger_to_write().unwrap();

    assert_eq!(scrub.sources.len(), 1, "{:?}", scrub.sources);
    assert_eq!(scrub.sources[0].url, "https://example.com/acme/skills");
    assert_eq!(scrub.actions, 1);
    assert!(scrub.failures.is_empty(), "{:?}", scrub.failures);

    let stored = fs::read(workspace.ledger_path()).unwrap();
    assert!(!stored.windows(9).any(|bytes| bytes == b"ghp_t0ken"));
    let clone = text_under(&workspace.cache_dir().join(&report.directory).join(".git"));
    assert!(!clone.contains("ghp_t0ken"));
    assert!(clone.contains("url = https://example.com/acme/skills\n"));
    // What was gathered is still there, under the URL without the token.
    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 1);
    assert_eq!(gathered[0].source.url, "https://example.com/acme/skills");

    // Said once: the second time there is nothing left to say.
    assert!(workspace.ledger_to_write().unwrap().1.is_empty());
}

#[test]
fn one_url_scrubbed_out_of_several_sources_is_told_about_once() {
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    drop(ledger);
    // Two branches and another folder of one repository, all gathered with
    // the same token: one credential, to be given to a helper once.
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute_batch(
            "INSERT INTO sources (kind, url, reference, subdirectory, directory, first_seen,
                                  last_seen, name, canonical_url, reference_key)
                 VALUES ('git', 'https://me:ghp_t0ken@example.com/acme/skills', 'main', 'skills',
                         'a', 0, 0, 'a', 'https://example.com/acme/skills', 'main'),
                        ('git', 'https://me:ghp_t0ken@example.com/acme/skills', 'dev', 'skills',
                         'b', 0, 0, 'b', 'https://example.com/acme/skills', 'dev'),
                        ('git', 'https://me:ghp_t0ken@example.com/acme/skills', 'main', 'more',
                         'c', 0, 0, 'c', 'https://example.com/acme/skills', 'main');",
        )
        .unwrap();

    let (ledger, scrub) = workspace.ledger_to_write().unwrap();

    let told: Vec<&str> = scrub
        .sources
        .iter()
        .map(|source| source.url.as_str())
        .collect();
    assert_eq!(told, ["https://example.com/acme/skills"]);
    assert_eq!(ledger.tainted_sources().unwrap(), []);
}

/// Fetches the repository at `from` with gix whatever URL it is asked for,
/// as a forge would answer each spelling of one repository's: what a test
/// needs to gather `https://github.com/...` with no network.
#[derive(Debug)]
struct Forge {
    from: String,
    asked: Mutex<Vec<String>>,
}

impl mindweaver_core::Remote for Forge {
    fn fetch(
        &self,
        url: &str,
        reference: Option<&str>,
        into: &std::path::Path,
        watch: &Watch,
    ) -> Result<mindweaver_core::port::Cloned, mindweaver_core::port::GitError> {
        self.asked.lock().unwrap().push(url.to_owned());
        mindweaver_core::adapter::gix::Gix.fetch(&self.from, reference, into, watch)
    }

    fn scrub(&self, clone: &std::path::Path) -> Result<bool, mindweaver_core::port::GitError> {
        mindweaver_core::adapter::gix::Gix.scrub(clone)
    }
}

#[test]
fn two_gathers_of_one_repository_by_two_spellings_update_one_source() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let forge = Arc::new(Forge {
        from: url(&source),
        asked: Mutex::new(Vec::new()),
    });
    let gathered = |typed: &str| {
        gather::gather(
            &workspace,
            &ledger,
            &Request::git(typed).through(Arc::clone(&forge) as Arc<dyn mindweaver_core::Remote>),
        )
        .unwrap()
    };

    let first = gathered("https://github.com/acme/skills");
    let second = gathered("git@github.com:Acme/skills.git");
    let third = gathered("acme/skills");

    // One source, one folder: the second gather found what the first took,
    // where the first put it, and so did the third.
    let sources = ledger.sources().unwrap();
    assert_eq!(sources.len(), 1, "{sources:?}");
    assert_eq!(sources[0].name, "acme-skills");
    assert_eq!(sources[0].url, "https://github.com/acme/skills");
    assert_eq!(second.directory, first.directory);
    assert_eq!(third.directory, first.directory);
    assert_eq!(first.added.len(), 1, "{first:?}");
    assert_eq!(second.unchanged.len(), 1, "{second:?}");
    assert_eq!(third.unchanged.len(), 1, "{third:?}");
    assert_eq!(ledger.gathered().unwrap().len(), 1);
    // Each fetched as it was typed, `owner/repo` as the GitHub repository
    // it names.
    assert_eq!(
        *forge.asked.lock().unwrap(),
        [
            "https://github.com/acme/skills",
            "git@github.com:Acme/skills.git",
            "https://github.com/acme/skills",
        ]
    );
}

// Unix's alone: a Windows path is no file URL with `file://` before it.
#[cfg(unix)]
#[test]
fn two_gathers_of_a_folder_by_its_path_and_its_file_url_update_one_source() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let path = url(&source);

    gather::gather(&workspace, &ledger, &Request::git(path.clone())).unwrap();
    let again = gather::gather(
        &workspace,
        &ledger,
        &Request::git(format!("file://{path}/")),
    )
    .unwrap();

    assert_eq!(ledger.sources().unwrap().len(), 1);
    assert_eq!(again.unchanged.len(), 1, "{again:?}");
    assert!(again.added.is_empty() && again.updated.is_empty());
}

#[test]
fn a_gather_names_its_source_as_asked_and_a_later_one_renames_it() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let first = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).named(Some("team-skills")),
    )
    .unwrap();
    let again = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    let renamed = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).named(Some("Team.Skills_2")),
    )
    .unwrap();

    assert_eq!(first.name, "team-skills");
    // A gather that names none keeps the name the source has.
    assert_eq!(again.name, "team-skills");
    assert_eq!(renamed.name, "Team.Skills_2");
    let sources = ledger.sources().unwrap();
    assert_eq!(sources.len(), 1, "{sources:?}");
    assert_eq!(sources[0].name, "Team.Skills_2");
    assert_eq!(renamed.directory, first.directory, "its folder stays");
}

#[test]
fn a_name_another_source_has_is_refused_before_anything_is_registered_or_fetched() {
    let one = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let other = repository(&[("skills/review/SKILL.md", &skill("review", "Look"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let named = |source: &TempDir| Request::git(url(source)).named(Some("skills"));
    gather::gather(&workspace, &ledger, &named(&one)).unwrap();

    let refused = gather::gather(&workspace, &ledger, &named(&other)).unwrap_err();
    // Nor may a source gathered before take it.
    gather::gather(&workspace, &ledger, &Request::git(url(&other))).unwrap();
    let renaming = gather::gather(&workspace, &ledger, &named(&other)).unwrap_err();

    for refused in [&refused, &renaming] {
        assert_eq!(refused.code(), "source_name_taken", "{refused}");
        assert_eq!(
            refused.to_string(),
            format!(
                "`skills` is the name of the source at {} already: gather this one under \
                 another `--name`",
                url(&one)
            )
        );
    }
    let names: Vec<(String, String)> = ledger
        .sources()
        .unwrap()
        .into_iter()
        .map(|source| (source.url.to_string(), source.name.to_string()))
        .collect();
    assert!(
        names.contains(&(url(&one), String::from("skills"))),
        "{names:?}"
    );
    assert_eq!(names.len(), 2, "{names:?}");
}

#[test]
fn a_name_no_source_can_take_is_refused_and_nothing_registered() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    for name in ["acme/skills", "workspace", "-skills", "", "..", "load:team"] {
        let refused = gather::gather(
            &workspace,
            &ledger,
            &Request::git(url(&source)).named(Some(name)),
        )
        .unwrap_err();

        assert_eq!(refused.code(), "source_name_invalid", "{name}");
    }
    assert!(ledger.sources().unwrap().is_empty());
    assert!(!workspace.cache_dir().exists());
}

#[test]
fn a_cancelled_gather_leaves_its_source_called_what_it_was() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let first = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    let watch = cancelled_at(|progress| matches!(progress, Progress::Connecting));

    let error = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source))
            .named(Some("renamed"))
            .watched_by(watch),
    )
    .unwrap_err();

    assert!(matches!(error, GatherError::Cancelled), "{error}");
    assert_eq!(ledger.sources().unwrap()[0].name, first.name);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A watch that keeps every step it is told, and where they are kept.
fn recording() -> (Watch, Arc<Mutex<Vec<Progress>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let watch = Watch::new().reporting_to({
        let seen = Arc::clone(&seen);
        move |progress| seen.lock().unwrap().push(progress)
    });
    (watch, seen)
}

/// A watch that cancels itself once it is told a step `at` matches.
fn cancelled_at(at: impl Fn(Progress) -> bool + Send + Sync + 'static) -> Watch {
    let watch = Watch::new();
    let cancel = watch.clone();
    watch.reporting_to(move |progress| {
        if at(progress) {
            cancel.cancel();
        }
    })
}

/// What a source to gather from serializes to is a contract: its kind as the
/// ledger files it, its URL without any credential, and the branch asked for.
/// A request is built before a URL with a credential in it is refused, so
/// the URL is taken out of what is printed whatever it holds.
#[test]
fn a_source_to_gather_from_serializes_without_the_credentials_its_url_carries() {
    let typed = Request::git("https://me:s3cret@github.com/acme/skills.git").at(Some("main"));

    let serialized = serde_json::to_string(&typed.source).unwrap();

    assert_eq!(
        serialized,
        r#"{"kind":"git","url":"https://github.com/acme/skills.git","reference":"main"}"#
    );
}

/// What a gather's report serializes to is a contract, as a status report's
/// is. What could not be taken, and what is wrong with what was, stay out:
/// a front end says them as its warnings, which in JSON are the envelope's.
#[test]
fn a_gather_report_serializes_what_changed_and_leaves_its_failures_to_the_warnings() {
    let harvested = |name: &str| gather::Harvested {
        name: name.to_owned().into(),
        directory: name.to_owned(),
        summary: Some(format!("About {name}")),
        path: std::path::PathBuf::from(format!(
            "/work/.mindweaver/skills/github.com-acme-skills/{name}"
        )),
    };
    let report = gather::Report {
        revision: Some("4f9c2a7".into()),
        directory: String::from("github.com-acme-skills"),
        added: vec![harvested("deploy")],
        updated: Vec::new(),
        unchanged: vec![harvested("review")],
        failures: vec![gather::Failure::UnusableName {
            path: std::path::PathBuf::from("/work/.mindweaver/cache/x"),
        }],
        warnings: vec![Warning {
            path: std::path::PathBuf::from("/work/SKILL.md"),
            issue: ValidationIssue::MetadataNotAMap,
        }],
        name: SourceName::parse("acme-skills").unwrap(),
    };

    let serialized = serde_json::to_string(&report).unwrap();

    assert_eq!(
        serialized,
        concat!(
            r#"{"revision":"4f9c2a7","directory":"github.com-acme-skills","#,
            r#""added":[{"name":"deploy","directory":"deploy","summary":"About deploy","#,
            r#""path":"/work/.mindweaver/skills/github.com-acme-skills/deploy"}],"#,
            r#""updated":[],"#,
            r#""unchanged":[{"name":"review","directory":"review","summary":"About review","#,
            r#""path":"/work/.mindweaver/skills/github.com-acme-skills/review"}],"#,
            r#""name":"acme-skills"}"#,
        )
    );
}
