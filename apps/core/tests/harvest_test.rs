//! Where in a repository a gather finds its skills (#101): every layout
//! `gh skill` and `npx skills` both read, a `SKILL.md` at the root, two
//! skills of one name in one source, what a shelf copy is taken without,
//! and a dry run, which writes nothing.

mod common;

use std::fs;
use std::path::Path;

use common::{commit, repository, skill, url, workspace, write_files};
use mindweaver_core::gather::{self, Failure, GatherError, Request};
use mindweaver_core::ledger::Ledger;
use mindweaver_core::mother::{random, GitRepositoryMother};
use mindweaver_core::{Kind, WeaverWorkspace, DEFAULT_SUBDIRECTORY};

/// What the shelf holds of a gather's source, folder by folder.
fn shelved(workspace: &WeaverWorkspace, directory: &str) -> Vec<String> {
    let shelf = workspace.gathered_dir(Kind::Skill).join(directory);
    let mut names: Vec<String> = match fs::read_dir(&shelf) {
        Ok(entries) => entries
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

/// The names a report says were added, in order.
fn added(report: &gather::Report) -> Vec<String> {
    report
        .added
        .iter()
        .map(|harvested| harvested.name.to_string())
        .collect()
}

/// Every file under `root`, by its route from there, and what it holds:
/// what a test compares to say nothing was written.
fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut found = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                let route = path.strip_prefix(root).unwrap().display().to_string();
                found.push((route, fs::read(&path).unwrap()));
            }
        }
    }
    found.sort();
    found
}

// ---------------------------------------------------------------------------
// Layouts
// ---------------------------------------------------------------------------

#[test]
fn every_layout_a_repository_keeps_its_skills_in_is_gathered() {
    for layout in GitRepositoryMother::layouts() {
        let source = GitRepositoryMother::with_skills_in(&layout, &["deploy", "review"]);
        let (_dir, workspace) = workspace();
        let ledger = workspace.ledger().unwrap();

        let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

        assert_eq!(added(&report), ["deploy", "review"], "{layout}");
        assert!(
            report.failures.is_empty(),
            "{layout}: {:?}",
            report.failures
        );
        let routes: Vec<&str> = report.added.iter().map(|one| one.route.as_str()).collect();
        let under = |name: &str| match layout.as_str() {
            "." => name.to_owned(),
            folder => format!("{folder}/{name}"),
        };
        assert_eq!(routes, [under("deploy"), under("review")], "{layout}");
        assert_eq!(
            shelved(&workspace, &report.directory),
            ["deploy", "review"],
            "{layout}"
        );
    }
}

#[test]
fn skills_kept_in_several_layouts_of_one_repository_are_all_gathered() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/team/review/SKILL.md", &skill("review", "Read it")),
        (".claude/skills/lint/SKILL.md", &skill("lint", "Tidy it")),
        (
            "plugins/docs/skills/pdf/SKILL.md",
            &skill("pdf", "Read PDFs"),
        ),
        ("notes/SKILL.md", &skill("notes", "Keep notes")),
        ("README.md", "not a skill\n"),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let mut taken: Vec<(String, String)> = report
        .added
        .iter()
        .map(|one| (one.name.to_string(), one.route.clone()))
        .collect();
    taken.sort();
    assert_eq!(
        taken,
        [
            ("deploy", "skills/deploy"),
            ("lint", ".claude/skills/lint"),
            ("notes", "notes"),
            ("pdf", "plugins/docs/skills/pdf"),
            ("review", "skills/team/review"),
        ]
        .map(|(name, route)| (name.to_owned(), route.to_owned()))
    );
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(ledger.gathered().unwrap().len(), 5);
}

/// What only one of the two tools reads, a hidden namespace other than
/// theirs or a skill four folders down where no layout goes, is not taken,
/// nor anything under `node_modules` or a hidden folder.
#[test]
fn a_skill_where_no_layout_looks_is_not_taken() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        (
            "skills/.experimental/early/SKILL.md",
            &skill("early", "Not yet"),
        ),
        ("docs/guide/example/SKILL.md", &skill("example", "Deep")),
        ("node_modules/pkg/SKILL.md", &skill("pkg", "Installed")),
        (".hidden/SKILL.md", &skill("hidden", "Hidden")),
        (
            "skills/node_modules/dep/SKILL.md",
            &skill("dep", "Installed too"),
        ),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    assert_eq!(added(&report), ["deploy"]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
}

#[test]
fn path_restricts_the_search_to_the_folder_it_names() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        (".claude/skills/lint/SKILL.md", &skill("lint", "Tidy it")),
        (
            ".claude/skills/review/SKILL.md",
            &skill("review", "Read it"),
        ),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let request = Request::git(url(&source)).from_subdirectory(".claude/skills/");
    let report = gather::gather(&workspace, &ledger, &request).unwrap();

    assert_eq!(added(&report), ["lint", "review"]);
    // Spelled as its route, so `.claude/skills/` and `.claude/skills` are
    // one source.
    let again = Request::git(url(&source)).from_subdirectory("./.claude/skills");
    gather::gather(&workspace, &ledger, &again).unwrap();
    let sources = ledger.sources().unwrap();
    assert_eq!(sources.len(), 1, "{sources:?}");
    assert_eq!(sources[0].subdirectory, ".claude/skills");
}

#[test]
fn a_repository_holding_no_skill_anywhere_says_where_it_looked() {
    let source = repository(&[
        ("README.md", "nothing to gather\n"),
        ("docs/deep/inside/SKILL.md", &skill("deep", "Too far down")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let error = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap_err();

    assert!(matches!(error, GatherError::NothingFound { .. }), "{error}");
    assert_eq!(error.code(), "gather_nothing_found");
    assert_eq!(
        error.to_string(),
        format!(
            "{}: no skill found in the repository: looked for a `SKILL.md` there, in each \
             folder there, and in each folder of `skills`, `skills/<namespace>`, \
             `.agents/skills`, `.claude/skills` and `plugins/<plugin>/skills`; name the folder \
             that holds them with `--path`",
            url(&source)
        )
    );
}

/// The revision a source names is the one its shelf holds, so a gather
/// that found nothing leaves it, and the shelf, as they were.
#[test]
fn a_source_whose_skills_went_keeps_the_revision_its_shelf_came_from() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    let before = ledger.sources().unwrap();

    fs::remove_dir_all(source.path().join("skills")).unwrap();
    write_files(source.path(), &[("README.md", "moved elsewhere\n")]);
    commit(source.path());
    let error = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap_err();

    assert_eq!(error.code(), "gather_nothing_found");
    let after = ledger.sources().unwrap();
    assert_eq!(after[0].revision, before[0].revision);
    assert_eq!(shelved(&workspace, &before[0].directory), ["deploy"]);
}

#[test]
fn a_folder_outside_the_repository_is_refused_before_anything_is_registered() {
    let source = GitRepositoryMother::with_skills(&["deploy"]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    for outside in ["../elsewhere", "skills/../../elsewhere", "/etc"] {
        let request = Request::git(url(&source)).from_subdirectory(outside);
        let error = gather::gather(&workspace, &ledger, &request).unwrap_err();

        assert_eq!(error.code(), "gather_path_outside", "{outside}: {error}");
        assert_eq!(
            error.to_string(),
            format!(
                "`{outside}` is not a folder inside the repository: name one by its route from \
                 the repository's root"
            )
        );
    }
    assert!(ledger.sources().unwrap().is_empty());
    assert!(!workspace.cache_dir().exists());
}

// ---------------------------------------------------------------------------
// A SKILL.md at the root
// ---------------------------------------------------------------------------

#[test]
fn a_skill_md_at_the_root_makes_the_repository_one_skill() {
    let name = random::kebab();
    let source = GitRepositoryMother::one_skill(&name);
    write_files(
        source.path(),
        &[("skills/other/SKILL.md", &skill("other", "Its own"))],
    );
    commit(source.path());
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    // One skill, the whole repository, whatever it holds further down.
    assert_eq!(added(&report), std::slice::from_ref(&name));
    assert_eq!(report.added[0].route, ".");
    assert_eq!(report.added[0].directory, name);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    // Filed under the name it declares, which the shelf's folder then is,
    // so nothing says the two disagree.
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let copy = workspace
        .gathered_dir(Kind::Skill)
        .join(&report.directory)
        .join(&name);
    assert!(copy.join("SKILL.md").is_file());
    assert!(copy.join("README.md").is_file());
    assert!(copy.join("scripts/run.sh").is_file());
    assert!(copy.join("examples/inner/SKILL.md").is_file());
    assert!(copy.join("skills/other/SKILL.md").is_file());
    // The clone's, not the skill's.
    assert!(!copy.join(".git").exists());

    let again = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    assert_eq!(again.unchanged.len(), 1, "{again:?}");
}

#[test]
fn a_skill_md_in_the_folder_path_names_is_that_folder_s_one_skill() {
    let source = repository(&[
        ("tools/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("tools/deploy/helper/SKILL.md", &skill("helper", "Inside")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let request = Request::git(url(&source)).from_subdirectory("tools/deploy");
    let report = gather::gather(&workspace, &ledger, &request).unwrap();

    assert_eq!(added(&report), ["deploy"]);
    assert_eq!(report.added[0].route, "tools/deploy");
    assert_eq!(shelved(&workspace, &report.directory), ["deploy"]);
}

#[test]
fn a_root_skill_declaring_no_folder_s_name_is_not_taken() {
    let source = repository(&[(
        "SKILL.md",
        "---\nname: a/b\ndescription: Not a folder's name\n---\n",
    )]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    assert!(report.is_empty());
    assert!(
        matches!(&report.failures[..], [Failure::Unfiled { name, .. }] if name == "a/b"),
        "{:?}",
        report.failures
    );
    assert_eq!(report.failures[0].code(), "gathered_root_unnamed");
}

// ---------------------------------------------------------------------------
// A name two skills of one source share
// ---------------------------------------------------------------------------

#[test]
fn two_skills_of_one_folder_name_in_one_source_are_neither_taken() {
    let source = repository(&[
        (
            "skills/ops/deploy/SKILL.md",
            &skill("deploy", "The ops one"),
        ),
        (
            "skills/web/deploy/SKILL.md",
            &skill("deploy", "The web one"),
        ),
        ("skills/review/SKILL.md", &skill("review", "Read it")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    assert_eq!(added(&report), ["review"]);
    let said: Vec<String> = report.failures.iter().map(ToString::to_string).collect();
    assert_eq!(
        said,
        [
            "skills/ops/deploy: not taken, since skills/web/deploy in the same source is called \
             `deploy` too; gather the one wanted with `--path`",
            "skills/web/deploy: not taken, since skills/ops/deploy in the same source is called \
             `deploy` too; gather the one wanted with `--path`",
        ]
    );
    assert!(report
        .failures
        .iter()
        .all(|failure| failure.code() == "gathered_name_clash"));
    assert_eq!(shelved(&workspace, &report.directory), ["review"]);
    let names: Vec<String> = ledger
        .gathered()
        .unwrap()
        .iter()
        .map(|one| one.name.to_string())
        .collect();
    assert_eq!(names, ["review"]);

    // `--path` picks one.
    let one = Request::git(url(&source)).from_subdirectory("skills/web");
    let picked = gather::gather(&workspace, &ledger, &one).unwrap();
    assert_eq!(added(&picked), ["deploy"]);
    assert_eq!(picked.added[0].summary.as_deref(), Some("The web one"));
}

#[test]
fn two_folders_declaring_one_name_clash_too_and_so_do_two_cases_of_one_folder() {
    let source = repository(&[
        ("skills/deploy-v1/SKILL.md", &skill("deploy", "The first")),
        (
            ".claude/skills/deploy-v2/SKILL.md",
            &skill("deploy", "The second"),
        ),
        ("skills/Lint/SKILL.md", &skill("lint", "Upper")),
        (".agents/skills/lint/SKILL.md", &skill("lint-too", "Lower")),
        ("skills/review/SKILL.md", &skill("review", "Read it")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    assert_eq!(added(&report), ["review"]);
    let clashing: Vec<(&str, String)> = report
        .failures
        .iter()
        .map(|failure| match failure {
            Failure::Clash { route, name, .. } => (route.as_str(), name.clone()),
            other => panic!("{other}"),
        })
        .collect();
    assert_eq!(
        clashing,
        [
            (".agents/skills/lint", String::from("lint")),
            (".claude/skills/deploy-v2", String::from("deploy")),
            ("skills/Lint", String::from("Lint")),
            ("skills/deploy-v1", String::from("deploy")),
        ]
    );
}

// ---------------------------------------------------------------------------
// What the shelf's copy is taken without
// ---------------------------------------------------------------------------

#[test]
fn a_skill_s_claude_plugin_is_left_off_the_shelf_and_said() {
    let source = repository(&[
        (
            "plugins/ops/skills/deploy/SKILL.md",
            &skill("deploy", "Ship it"),
        ),
        (
            "plugins/ops/skills/deploy/.claude-plugin/plugin.json",
            r#"{"name": "deploy", "hooks": "hooks.json"}"#,
        ),
        (
            "plugins/ops/skills/deploy/scripts/.claude-plugin/example.json",
            "{}",
        ),
        (
            "plugins/ops/.claude-plugin/plugin.json",
            r#"{"name": "ops"}"#,
        ),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    assert_eq!(added(&report), ["deploy"]);
    assert_eq!(report.left_out.len(), 1, "{:?}", report.left_out);
    assert_eq!(report.left_out[0].name, "deploy");
    assert!(report.left_out[0]
        .left
        .path
        .ends_with("deploy/.claude-plugin"));
    assert!(report.left_out[0].left.reason.contains("as a plugin"));
    let copy = &report.added[0].path;
    assert!(!copy.join(".claude-plugin").exists());
    // Only at the top: further down it is the skill's own file.
    assert!(copy.join("scripts/.claude-plugin/example.json").is_file());

    // Compared without it, so a second gather finds nothing moved.
    let again = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    assert_eq!(again.unchanged.len(), 1, "{again:?}");
}

// ---------------------------------------------------------------------------
// A source an older build gathered from `skills`
// ---------------------------------------------------------------------------

/// Before #101 a gather naming no folder took `skills/`, and registered the
/// source from it: the next gather naming none is of that source, not a
/// second one of the same repository.
#[test]
fn a_source_an_older_build_gathered_from_skills_is_the_whole_repository_s_from_now_on() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        (".claude/skills/lint/SKILL.md", &skill("lint", "Tidy it")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let older = Request::git(url(&source)).from_subdirectory("skills");
    let first = gather::gather(&workspace, &ledger, &older).unwrap();

    let report = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    assert_eq!(report.directory, first.directory);
    assert_eq!(report.name, first.name);
    assert_eq!(added(&report), ["lint"]);
    assert_eq!(report.unchanged.len(), 1);
    let sources = ledger.sources().unwrap();
    assert_eq!(sources.len(), 1, "{sources:?}");
    assert_eq!(sources[0].subdirectory, DEFAULT_SUBDIRECTORY);
}

#[test]
fn a_cancelled_gather_leaves_an_older_build_s_source_taken_from_skills() {
    let source = GitRepositoryMother::with_skills(&["deploy"]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let older = Request::git(url(&source)).from_subdirectory("skills");
    gather::gather(&workspace, &ledger, &older).unwrap();

    let watch = gather::Watch::new();
    let cancelling = watch.clone();
    let watch = watch.reporting_to(move |progress| {
        if matches!(progress, gather::Progress::Harvesting { .. }) {
            cancelling.cancel();
        }
    });
    let error = gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&source)).watched_by(watch),
    )
    .unwrap_err();

    assert!(matches!(error, GatherError::Cancelled), "{error}");
    let sources = ledger.sources().unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].subdirectory, "skills");
}

// ---------------------------------------------------------------------------
// A dry run
// ---------------------------------------------------------------------------

/// Everything a dry run could touch: the shelf, the cache and the ledger's
/// file, as they stand.
fn written(workspace: &WeaverWorkspace) -> Vec<(String, Vec<u8>)> {
    let mut all = snapshot(&workspace.gathered_dir(Kind::Skill));
    all.extend(snapshot(&workspace.cache_dir()));
    all.push((
        String::from("ledger"),
        fs::read(workspace.ledger_path()).unwrap_or_default(),
    ));
    all
}

fn reading(workspace: &WeaverWorkspace) -> Ledger {
    workspace.ledger_for_reading().unwrap()
}

#[test]
fn a_dry_run_says_what_would_be_taken_where_a_gather_would_put_it() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        (".claude/skills/lint/SKILL.md", &skill("lint", "Tidy it")),
    ]);
    let (_dir, workspace) = workspace();

    let report = gather::dry_run(
        &workspace,
        &reading(&workspace),
        &Request::git(url(&source)),
    )
    .unwrap();

    // By name, wherever each is in the source, as a gather reports them.
    assert_eq!(added(&report), ["deploy", "lint"]);
    let routes: Vec<&str> = report.added.iter().map(|one| one.route.as_str()).collect();
    assert_eq!(routes, ["skills/deploy", ".claude/skills/lint"]);
    assert!(report.revision.is_some());
    // What a gather would call it, and where it would put it.
    let ledger = workspace.ledger().unwrap();
    let gathered = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    assert_eq!(report.name, gathered.name);
    assert_eq!(report.directory, gathered.directory);
    assert_eq!(report.added[0].path, gathered.added[0].path);
}

#[test]
fn a_dry_run_leaves_the_shelf_the_cache_and_the_ledger_as_they_were() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    write_files(
        source.path(),
        &[
            (
                "skills/deploy/SKILL.md",
                &skill("deploy", "Ship it, carefully"),
            ),
            ("skills/review/SKILL.md", &skill("review", "Read it")),
        ],
    );
    commit(source.path());
    let before = written(&workspace);
    let history = ledger.history(10).unwrap().len();

    let report = gather::dry_run(
        &workspace,
        &reading(&workspace),
        &Request::git(url(&source)),
    )
    .unwrap();

    assert_eq!(added(&report), ["review"]);
    assert_eq!(report.updated.len(), 1);
    assert_eq!(
        report.updated[0].summary.as_deref(),
        Some("Ship it, carefully")
    );
    assert_eq!(written(&workspace), before);
    assert_eq!(ledger.history(10).unwrap().len(), history);
    // Nothing of its own clone left behind either.
    let scratch = fs::read_dir(workspace.scratch_dir())
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert_eq!(scratch, 0);
}

#[test]
fn a_dry_run_in_a_workspace_that_never_gathered_makes_no_ledger() {
    let source = GitRepositoryMother::with_skills(&["deploy"]);
    let (_dir, workspace) = workspace();

    let report = gather::dry_run(
        &workspace,
        &reading(&workspace),
        &Request::git(url(&source)),
    )
    .unwrap();

    assert_eq!(added(&report), ["deploy"]);
    assert!(!workspace.ledger_path().exists());
    assert!(!workspace.gathered_dir(Kind::Skill).exists());
    assert!(!workspace.cache_dir().exists());
}

/// What could not be read is named where it is in the source, since the
/// clone it was read from is gone; what is wrong with what would be taken,
/// where it would be put.
#[test]
fn a_dry_run_names_what_it_could_not_read_by_its_route_in_the_source() {
    let source = repository(&[
        ("skills/broken/SKILL.md", "no front matter at all\n"),
        ("skills/pdf--forms/SKILL.md", &skill("pdf--forms", "Forms")),
        ("skills/a/deploy/SKILL.md", &skill("deploy", "One")),
        ("skills/b/deploy/SKILL.md", &skill("deploy", "Two")),
    ]);
    let (_dir, workspace) = workspace();

    let report = gather::dry_run(
        &workspace,
        &reading(&workspace),
        &Request::git(url(&source)),
    )
    .unwrap();

    let paths: Vec<&Path> = report.failures.iter().map(Failure::path).collect();
    assert_eq!(
        paths,
        [
            Path::new("skills/a/deploy"),
            Path::new("skills/b/deploy"),
            Path::new("skills/broken/SKILL.md"),
        ]
    );
    assert!(report.failures[2]
        .to_string()
        .starts_with("skills/broken/SKILL.md: "));
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert_eq!(
        report.warnings[0].path,
        workspace
            .gathered_dir(Kind::Skill)
            .join(&report.directory)
            .join("pdf--forms/SKILL.md")
    );
    assert!(report.warnings[0].issue.to_string().contains("pdf--forms"));
}

#[test]
fn a_dry_run_of_a_known_source_under_a_new_name_says_the_name_and_renames_nothing() {
    let source = GitRepositoryMother::with_skills(&["deploy"]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let first = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let request = Request::git(url(&source)).named(Some("team"));
    let report = gather::dry_run(&workspace, &reading(&workspace), &request).unwrap();

    assert_eq!(report.name, "team");
    assert_eq!(report.unchanged.len(), 1);
    assert_eq!(ledger.sources().unwrap()[0].name, first.name);
}

#[test]
fn a_dry_run_is_refused_what_a_gather_is() {
    let source = GitRepositoryMother::with_skills(&["deploy"]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let readme = repository(&[("README.md", "nothing\n")]);

    let nothing = gather::dry_run(&workspace, &ledger, &Request::git(url(&readme))).unwrap_err();
    let outside = gather::dry_run(
        &workspace,
        &ledger,
        &Request::git(url(&source)).from_subdirectory("../x"),
    )
    .unwrap_err();
    let missing = gather::dry_run(
        &workspace,
        &ledger,
        &Request::git(url(&source)).from_subdirectory("agents"),
    )
    .unwrap_err();

    assert_eq!(nothing.code(), "gather_nothing_found");
    assert_eq!(outside.code(), "gather_path_outside");
    assert_eq!(missing.code(), "gather_no_such_folder");
    assert!(ledger.sources().unwrap().is_empty());
}
