//! Looking under a workspace, or under folders given, for what it could
//! manage.

use std::fs;
use std::path::{Path, PathBuf};

use mindweaver_core::scan::{Order, Reason, Request, DEFAULT_DEPTH, MAX_LISTED};
use mindweaver_core::{
    scan, Agent, Directories, Found, MindProject, ScanFailure, Share, WeaverWorkspace, MIND_DIR,
    SETTINGS_FILE, WEAVER_CONFIG, WEAVER_DIR,
};
use tempfile::TempDir;

/// A git repository, as far as a scan can tell: a directory with a `.git`.
fn git_repository(root: &Path) {
    fs::create_dir_all(root.join(".git")).unwrap();
}

fn routes(found: &[Found]) -> Vec<&str> {
    found.iter().map(|found| found.route.as_str()).collect()
}

fn paths(found: &[Found]) -> Vec<&Path> {
    found.iter().map(|found| found.path.as_path()).collect()
}

/// What a scan of `roots` to `depth` finds.
fn under(workspace: &WeaverWorkspace, roots: &[&Path], depth: usize) -> mindweaver_core::Scan {
    scan(
        workspace,
        &Request {
            roots: roots.iter().map(|root| root.to_path_buf()).collect(),
            depth,
            ..Request::default()
        },
    )
}

/// A workspace in `ws` beside the folders a test scans, so that what they
/// hold is found only by naming them.
fn workspace_beside(dir: &TempDir) -> WeaverWorkspace {
    WeaverWorkspace::init(dir.path().join("ws")).unwrap().0
}

#[test]
fn a_scan_finds_projects_and_repositories_and_says_which_are_linked() {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    for name in ["alpha", "beta"] {
        fs::create_dir(dir.path().join(name)).unwrap();
        MindProject::init(dir.path().join(name)).unwrap();
    }
    git_repository(&dir.path().join("gamma"));
    fs::create_dir(dir.path().join("plain")).unwrap();
    let alpha = MindProject::open(dir.path().join("alpha")).unwrap();
    workspace.link(&alpha).unwrap();

    let scan = scan(&workspace, &Request::default());

    assert_eq!(routes(&scan.found), vec!["alpha", "beta", "gamma"]);
    let flags: Vec<(bool, bool)> = scan.found.iter().map(|f| (f.mind, f.linked)).collect();
    assert_eq!(flags, vec![(true, true), (true, false), (false, false)]);
    assert_eq!(scan.roots, vec![dir.path().to_path_buf()]);
    assert_eq!(scan.depth, DEFAULT_DEPTH);
    assert!(scan.failures.is_empty());
    assert!(!scan.truncated);
}

#[test]
fn linked_is_decided_by_where_an_entry_points() {
    let dir = TempDir::new().unwrap();
    WeaverWorkspace::init(dir.path()).unwrap();
    fs::create_dir(dir.path().join("beta")).unwrap();
    MindProject::init(dir.path().join("beta")).unwrap();
    fs::write(
        dir.path().join(".mindweaver/weaver.toml"),
        "version = 2\nname = \"w\"\nprojects = [\"./beta\"]\n",
    )
    .unwrap();
    let workspace = WeaverWorkspace::open(dir.path()).unwrap();

    let scan = scan(&workspace, &Request::default());

    assert!(scan.found[0].linked, "{:?}", scan.found);
}

#[test]
fn a_repository_is_not_looked_inside() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    git_repository(&dir.path().join("mono"));
    git_repository(&dir.path().join("mono/vendor/lib"));

    assert_eq!(
        routes(&scan(&workspace, &Request::default()).found),
        vec!["mono"]
    );
}

#[test]
fn hidden_folders_and_build_output_are_not_entered() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    git_repository(&dir.path().join(".cache/repo"));
    git_repository(&dir.path().join("node_modules/package"));
    git_repository(&dir.path().join("kept"));

    assert_eq!(
        routes(&scan(&workspace, &Request::default()).found),
        vec!["kept"]
    );
}

#[test]
fn repositories_are_found_a_few_levels_down_and_no_further() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    git_repository(&dir.path().join("github.com/acme/skills"));
    git_repository(&dir.path().join("a/b/c/too-deep"));
    assert_eq!(
        DEFAULT_DEPTH, 3,
        "the fixture above is built for three levels"
    );

    assert_eq!(
        routes(&scan(&workspace, &Request::default()).found),
        vec!["github.com/acme/skills"]
    );
}

#[test]
fn the_workspace_root_is_a_candidate_too() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    MindProject::init(dir.path()).unwrap();
    git_repository(&dir.path().join("beside"));

    // A workspace can manage the project it sits in, and the root is still
    // where the other repositories are kept.
    assert_eq!(
        routes(&scan(&workspace, &Request::default()).found),
        vec![".", "beside"]
    );
}

#[test]
fn a_workspace_whose_root_is_not_there_yet_holds_nothing() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("nobody-made-this");
    let workspace = WeaverWorkspace::unmade(&home).unwrap();

    let scan = scan(&workspace, &Request::default());

    assert!(scan.found.is_empty());
    assert!(scan.failures.is_empty(), "{:?}", scan.failures);
    assert!(!home.exists());
}

#[test]
fn a_scan_looks_under_every_root_it_is_given_and_not_under_the_workspace() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    git_repository(&dir.path().join("ws/inside"));
    let one = dir.path().join("one");
    let two = dir.path().join("two");
    git_repository(&one.join("alpha"));
    fs::create_dir_all(two.join("acme/beta")).unwrap();
    MindProject::init(two.join("acme/beta")).unwrap();

    let scan = under(&workspace, &[&two, &one], DEFAULT_DEPTH);

    let mut found = paths(&scan.found);
    found.sort();
    assert_eq!(found, vec![one.join("alpha"), two.join("acme/beta")]);
    assert_eq!(scan.roots, vec![two, one], "in the order given");
    assert!(scan.failures.is_empty(), "{:?}", scan.failures);
}

#[test]
fn the_depth_a_scan_is_given_is_how_far_below_each_root_it_looks() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    let root = dir.path().join("work");
    git_repository(&root.join("top"));
    git_repository(&root.join("org/deep"));
    git_repository(&root.join("a/b/c/d/deeper"));

    let found = |depth| {
        let mut found: Vec<PathBuf> = paths(&under(&workspace, &[&root], depth).found)
            .into_iter()
            .map(|path| path.strip_prefix(&root).unwrap().to_path_buf())
            .collect();
        found.sort();
        found
    };

    assert!(found(0).is_empty());
    assert_eq!(found(1), vec![PathBuf::from("top")]);
    assert_eq!(
        found(2),
        vec![PathBuf::from("org/deep"), PathBuf::from("top")]
    );
    assert_eq!(found(5).len(), 3);
}

#[test]
fn a_root_is_a_candidate_and_is_looked_inside_even_when_it_is_a_repository() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    let root = dir.path().join("mono");
    git_repository(&root);
    git_repository(&root.join("vendor/lib"));

    let scan = under(&workspace, &[&root], DEFAULT_DEPTH);

    assert_eq!(
        paths(&scan.found),
        vec![root.clone(), root.join("vendor/lib")]
    );
}

#[test]
fn a_folder_two_roots_reach_is_found_once() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    let outer = dir.path().join("work");
    let inner = outer.join("acme");
    git_repository(&inner.join("api"));
    git_repository(&inner);

    let scan = under(
        &workspace,
        &[&outer, &inner, &outer.join("./")],
        DEFAULT_DEPTH,
    );

    let mut found = paths(&scan.found);
    found.sort();
    assert_eq!(found, vec![inner.clone(), inner.join("api")]);
    assert_eq!(scan.roots, vec![outer, inner], "each root once");
}

#[test]
fn a_root_named_that_is_not_there_is_a_failure_beside_what_the_others_hold() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    let missing = dir.path().join("wrok");
    let root = dir.path().join("work");
    git_repository(&root.join("alpha"));

    for depth in [0, DEFAULT_DEPTH] {
        let scan = under(&workspace, &[&missing, &root], depth);

        assert_eq!(scan.failures.len(), 1, "{:?}", scan.failures);
        assert_eq!(scan.failures[0].path(), missing);
        assert!(
            scan.failures[0].to_string().contains("cannot be listed"),
            "{}",
            scan.failures[0]
        );
        if depth > 0 {
            assert_eq!(paths(&scan.found), vec![root.join("alpha")]);
        }
    }
}

#[test]
fn below_a_root_hidden_folders_and_build_output_are_not_entered() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    // Named, so entered whatever it is called.
    let root = dir.path().join(".work");
    git_repository(&root.join(".cache/repo"));
    git_repository(&root.join("node_modules/package"));
    git_repository(&root.join("target/debug/build"));
    git_repository(&root.join("kept"));

    let scan = under(&workspace, &[&root], DEFAULT_DEPTH);

    assert_eq!(paths(&scan.found), vec![root.join("kept")]);
}

#[cfg(unix)]
#[test]
fn below_a_root_a_symlinked_folder_is_not_entered() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    let elsewhere = dir.path().join("elsewhere");
    git_repository(&elsewhere.join("repo"));
    let root = dir.path().join("work");
    git_repository(&root.join("kept"));
    std::os::unix::fs::symlink(&elsewhere, root.join("link")).unwrap();
    // A loop, which entering links would never leave.
    std::os::unix::fs::symlink(&root, root.join("kept-loop")).unwrap();

    let scan = under(&workspace, &[&root], 10);

    assert_eq!(paths(&scan.found), vec![root.join("kept")]);
    assert!(scan.failures.is_empty(), "{:?}", scan.failures);
}

#[test]
fn a_scan_that_runs_out_of_budget_says_so_and_still_sees_a_second_root() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    let crowded = dir.path().join("crowded");
    // The root and every folder in it but the last are listed, which is
    // the whole budget.
    for index in 0..MAX_LISTED {
        fs::create_dir_all(crowded.join(format!("d{index:04}"))).unwrap();
    }
    let other = dir.path().join("other");
    git_repository(&other.join("alpha"));

    let crowded_only = under(&workspace, &[&crowded], DEFAULT_DEPTH);
    let both = under(&workspace, &[&crowded, &other], DEFAULT_DEPTH);

    assert!(crowded_only.truncated);
    assert!(crowded_only.found.is_empty());
    // Breadth first across both roots: the second is looked under before
    // the first's folders use up what is left.
    assert!(both.truncated);
    assert_eq!(paths(&both.found), vec![other.join("alpha")]);
}

#[test]
fn a_scan_links_nothing() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    let root = dir.path().join("work");
    fs::create_dir_all(root.join("alpha")).unwrap();
    MindProject::init(root.join("alpha")).unwrap();
    git_repository(&root.join("beta"));
    let marker = dir.path().join("ws").join(WEAVER_DIR).join(WEAVER_CONFIG);
    let before = fs::read_to_string(&marker).unwrap();

    let scan = under(&workspace, &[&root], DEFAULT_DEPTH);

    assert_eq!(scan.found.len(), 2);
    assert!(scan.found.iter().all(|found| !found.linked));
    assert_eq!(fs::read_to_string(&marker).unwrap(), before);
    assert!(WeaverWorkspace::open(dir.path().join("ws"))
        .unwrap()
        .config()
        .projects
        .is_empty());
}

#[test]
fn a_route_is_spelled_from_where_the_scan_is_asked_to_spell_it() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    git_repository(&dir.path().join("work/acme/api"));
    fs::create_dir(dir.path().join("other")).unwrap();

    let spelled = |from: Option<PathBuf>| {
        let request = Request {
            from,
            ..Request::default()
        };
        scan(&workspace, &request).found[0].route.clone()
    };

    assert_eq!(spelled(None), "work/acme/api", "from the workspace root");
    assert_eq!(spelled(Some(dir.path().join("work"))), "acme/api");
    // Not `../work/acme/api`, which reads right from one directory only.
    let elsewhere = spelled(Some(dir.path().join("other")));
    assert_eq!(
        elsewhere,
        dir.path()
            .join("work")
            .join("acme")
            .join("api")
            .display()
            .to_string()
    );
}

#[test]
fn a_repository_outside_the_workspace_is_spelled_by_its_path() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    let root = dir.path().join("work");
    git_repository(&root.join("alpha"));

    let scan = under(&workspace, &[&root], DEFAULT_DEPTH);

    assert_eq!(
        routes(&scan.found),
        vec![root.join("alpha").display().to_string()]
    );
}

#[test]
fn a_project_marked_by_its_settings_is_found_with_its_id() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    fs::create_dir(dir.path().join("alpha")).unwrap();
    let (alpha, _) = MindProject::init_asked(
        dir.path().join("alpha"),
        &Agent::ALL,
        Share::Local,
        &Directories::default(),
    )
    .unwrap();
    // Marked by a `mind.toml`, which names no id.
    fs::create_dir(dir.path().join("older")).unwrap();
    MindProject::init(dir.path().join("older")).unwrap();
    git_repository(&dir.path().join("plain"));

    let scan = scan(&workspace, &Request::default());

    let ids: Vec<(&str, Option<&str>)> = scan
        .found
        .iter()
        .map(|found| {
            (
                found.route.as_str(),
                found.id.as_ref().map(|id| id.as_str()),
            )
        })
        .collect();
    let id = alpha.settings().unwrap().id.as_str();
    assert_eq!(
        ids,
        vec![("alpha", Some(id)), ("older", None), ("plain", None)]
    );
    assert!(scan.failures.is_empty(), "{:?}", scan.failures);
}

#[test]
fn a_project_whose_settings_will_not_open_is_found_without_an_id_and_said() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let settings = dir.path().join("alpha").join(MIND_DIR).join(SETTINGS_FILE);
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(&settings, "{ not toml\n").unwrap();
    git_repository(&dir.path().join("beta"));

    let scan = scan(&workspace, &Request::default());

    // Still a mind project, as its marker says, and the one beside it is
    // still found.
    assert_eq!(routes(&scan.found), vec!["alpha", "beta"]);
    assert!(scan.found[0].mind);
    assert_eq!(scan.found[0].id, None);
    assert_eq!(scan.failures.len(), 1, "{:?}", scan.failures);
    assert!(
        matches!(scan.failures[0], ScanFailure::Unread(_)),
        "{:?}",
        scan.failures
    );
    assert_eq!(scan.failures[0].path(), settings);
}

/// Each one found by its route, with its reason and the agent files it
/// holds.
fn reasons(found: &[Found]) -> Vec<(&str, Reason, Vec<&str>)> {
    found
        .iter()
        .map(|found| {
            (
                found.route.as_str(),
                found.reason,
                found.agent_files.iter().map(String::as_str).collect(),
            )
        })
        .collect()
}

#[test]
fn each_one_found_says_why_it_is_worth_linking() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    fs::create_dir(dir.path().join("current")).unwrap();
    MindProject::init_asked(
        dir.path().join("current"),
        &Agent::ALL,
        Share::Local,
        &Directories::default(),
    )
    .unwrap();
    fs::create_dir(dir.path().join("older")).unwrap();
    MindProject::init(dir.path().join("older")).unwrap();
    let unread = dir.path().join("unread").join(MIND_DIR).join(SETTINGS_FILE);
    fs::create_dir_all(unread.parent().unwrap()).unwrap();
    fs::write(&unread, "{ not toml\n").unwrap();
    let instructed = dir.path().join("instructed");
    git_repository(&instructed);
    fs::write(instructed.join("AGENTS.md"), "# Agents\n").unwrap();
    fs::create_dir(instructed.join(".claude")).unwrap();
    let configured = dir.path().join("configured");
    git_repository(&configured);
    fs::write(configured.join("opencode.json"), "{}\n").unwrap();
    git_repository(&dir.path().join("plain"));

    let scan = scan(&workspace, &Request::default());

    assert_eq!(
        reasons(&scan.found),
        vec![
            ("configured", Reason::Agents, vec!["opencode.json"]),
            ("current", Reason::Id, vec![]),
            ("instructed", Reason::Agents, vec![".claude", "AGENTS.md"]),
            ("older", Reason::Mind, vec![]),
            ("plain", Reason::Git, vec![]),
            ("unread", Reason::Mind, vec![]),
        ]
    );
    assert_eq!(scan.failures.len(), 1, "{:?}", scan.failures);
}

#[test]
fn every_name_an_agent_reads_at_the_root_is_an_agent_file() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    // A repository for each agent, named for it so they are found in report
    // order, holding every name that agent reads at the root.
    for agent in Agent::ALL {
        let repository = dir.path().join(agent.name());
        git_repository(&repository);
        for name in agent.reads_at_root() {
            fs::write(repository.join(name), "").unwrap();
        }
    }

    let scan = scan(&workspace, &Request::default());

    assert_eq!(scan.found.len(), Agent::ALL.len());
    for (found, agent) in scan.found.iter().zip(Agent::ALL) {
        let names: Vec<String> = agent
            .reads_at_root()
            .iter()
            .map(|name| name.display().to_string())
            .collect();
        assert_eq!(found.agent_files, names, "{agent}");
        assert_eq!(found.reason, Reason::Agents, "{agent}");
    }
}

#[test]
fn only_what_sits_at_a_repositorys_root_counts_as_an_agent_file() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let repository = dir.path().join("repo");
    git_repository(&repository);
    fs::create_dir_all(repository.join("docs/.claude")).unwrap();
    fs::write(repository.join("docs/AGENTS.md"), "").unwrap();
    // Not one of the names: `.claude.md` is not `.claude`.
    fs::write(repository.join(".claude.md"), "").unwrap();

    let scan = scan(&workspace, &Request::default());

    assert_eq!(reasons(&scan.found), vec![("repo", Reason::Git, vec![])]);
}

#[test]
fn agent_files_alone_do_not_make_a_folder_a_candidate() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let notes = dir.path().join("notes");
    fs::create_dir_all(notes.join(".claude")).unwrap();
    fs::write(notes.join("CLAUDE.md"), "").unwrap();
    git_repository(&notes.join("inside"));

    let scan = scan(&workspace, &Request::default());

    // Neither a project nor a repository, so still walked through.
    assert_eq!(routes(&scan.found), vec!["notes/inside"]);
}

#[test]
fn a_mind_project_lists_the_agent_files_it_holds_beside_its_reason() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let alpha = dir.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    MindProject::init_asked(&alpha, &Agent::ALL, Share::Local, &Directories::default()).unwrap();
    fs::create_dir_all(alpha.join(".agents/skills")).unwrap();
    fs::write(alpha.join("CLAUDE.md"), "").unwrap();

    let scan = scan(&workspace, &Request::default());

    assert_eq!(
        reasons(&scan.found),
        vec![("alpha", Reason::Id, vec![".agents", "CLAUDE.md"])]
    );
}

#[cfg(unix)]
#[test]
fn an_agent_file_that_is_a_link_counts_without_being_followed() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let repository = dir.path().join("repo");
    git_repository(&repository);
    // Pointing nowhere: what it is called is what counts.
    std::os::unix::fs::symlink("AGENTS.md", repository.join("CLAUDE.md")).unwrap();

    let scan = scan(&workspace, &Request::default());

    assert_eq!(
        reasons(&scan.found),
        vec![("repo", Reason::Agents, vec!["CLAUDE.md"])]
    );
    assert!(scan.failures.is_empty(), "{:?}", scan.failures);
}

/// What a suggestion under `roots` finds, or under the workspace root when
/// there are none.
fn suggested(workspace: &WeaverWorkspace, roots: &[&Path]) -> mindweaver_core::Scan {
    scan(
        workspace,
        &Request {
            roots: roots.iter().map(|root| root.to_path_buf()).collect(),
            order: Order::Suggested,
            ..Request::default()
        },
    )
}

/// A workspace whose root holds one of each: `zeta`, a mind project with an
/// id, and `alpha`, one marked by a `mind.toml`, both linked, then, not
/// linked, `yak`, a project with an id, `bee` and `ant`, two marked by a
/// `mind.toml`, `xylo`, a repository holding a `CLAUDE.md`, `cat`, one
/// holding `.agents`, and `dog`, a repository and nothing more. Named so
/// that no rank is their order by name.
fn one_of_each() -> (TempDir, WeaverWorkspace) {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    for name in ["zeta", "yak"] {
        fs::create_dir(dir.path().join(name)).unwrap();
        MindProject::init_asked(
            dir.path().join(name),
            &Agent::ALL,
            Share::Local,
            &Directories::default(),
        )
        .unwrap();
    }
    for name in ["alpha", "bee", "ant"] {
        fs::create_dir(dir.path().join(name)).unwrap();
        MindProject::init(dir.path().join(name)).unwrap();
    }
    git_repository(&dir.path().join("xylo"));
    fs::write(dir.path().join("xylo/CLAUDE.md"), "# Claude\n").unwrap();
    git_repository(&dir.path().join("cat"));
    fs::create_dir_all(dir.path().join("cat/.agents/skills")).unwrap();
    git_repository(&dir.path().join("dog"));
    for name in ["zeta", "alpha"] {
        let project = MindProject::open(dir.path().join(name)).unwrap();
        workspace.link(&project).unwrap();
    }
    (dir, workspace)
}

#[test]
fn a_suggestion_lists_what_is_not_linked_first_best_reason_first() {
    let (_dir, workspace) = one_of_each();

    let scan = suggested(&workspace, &[]);

    let ranked: Vec<(&str, bool, Reason)> = scan
        .found
        .iter()
        .map(|found| (found.route.as_str(), found.linked, found.reason))
        .collect();
    assert_eq!(
        ranked,
        vec![
            ("yak", false, Reason::Id),
            ("ant", false, Reason::Mind),
            ("bee", false, Reason::Mind),
            ("cat", false, Reason::Agents),
            ("xylo", false, Reason::Agents),
            ("dog", false, Reason::Git),
            // Linked, by route alone, whatever each holds.
            ("alpha", true, Reason::Mind),
            ("zeta", true, Reason::Id),
        ]
    );
    assert!(scan.failures.is_empty(), "{:?}", scan.failures);
}

#[test]
fn without_a_suggestion_a_scan_is_ordered_by_route() {
    let (_dir, workspace) = one_of_each();

    let scan = scan(&workspace, &Request::default());

    assert_eq!(
        routes(&scan.found),
        vec!["alpha", "ant", "bee", "cat", "dog", "xylo", "yak", "zeta"]
    );
}

#[test]
fn a_suggestion_ranks_across_every_root_it_is_given() {
    let dir = TempDir::new().unwrap();
    let workspace = workspace_beside(&dir);
    let one = dir.path().join("one");
    let two = dir.path().join("two");
    git_repository(&one.join("plain"));
    git_repository(&two.join("worked"));
    fs::write(two.join("worked/AGENTS.md"), "").unwrap();
    fs::create_dir_all(one.join("project")).unwrap();
    MindProject::init_asked(
        one.join("project"),
        &Agent::ALL,
        Share::Local,
        &Directories::default(),
    )
    .unwrap();

    let scan = suggested(&workspace, &[&one, &two]);

    assert_eq!(
        paths(&scan.found),
        vec![one.join("project"), two.join("worked"), one.join("plain")]
    );
}

#[test]
fn a_suggestion_links_nothing_and_writes_nothing() {
    let (dir, workspace) = one_of_each();
    let marker = dir.path().join(WEAVER_DIR).join(WEAVER_CONFIG);
    let before = fs::read_to_string(&marker).unwrap();
    let held = |name: &str| fs::read_dir(dir.path().join(name)).unwrap().count();
    let counts: Vec<usize> = ["yak", "bee", "xylo", "dog"].map(held).to_vec();

    let scan = suggested(&workspace, &[]);

    assert!(scan.found.iter().any(|found| !found.linked));
    assert_eq!(fs::read_to_string(&marker).unwrap(), before);
    assert_eq!(
        WeaverWorkspace::open(dir.path())
            .unwrap()
            .config()
            .projects
            .len(),
        2
    );
    assert!(!workspace.ledger_path().exists(), "no ledger made");
    // No `mind init` either: a repository is suggested, not made a project.
    assert!(!dir.path().join("dog").join(MIND_DIR).exists());
    assert!(!dir.path().join("xylo").join(MIND_DIR).exists());
    assert_eq!(["yak", "bee", "xylo", "dog"].map(held).to_vec(), counts);
}

#[test]
fn a_suggestion_does_not_make_the_workspace_it_reads() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let work = dir.path().join("work");
    git_repository(&work.join("alpha"));
    let workspace = WeaverWorkspace::unmade(&home).unwrap();

    let scan = suggested(&workspace, &[&work]);

    assert_eq!(paths(&scan.found), vec![work.join("alpha")]);
    assert!(!home.exists());
}
