//! What a front end watches to know a snapshot is out of date (#74), against
//! a real workspace on disk: every path a snapshot reads that somebody can
//! change, each named on its own, never a tree.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use mindweaver_core::doctor::Machine;
use mindweaver_core::resolve::{Reason, Resolved};
use mindweaver_core::view::Snapshot;
use mindweaver_core::watch::{self, Scope, Watched};
use mindweaver_core::{
    claude_rules, Agent, AgentHomes, Catalog, Directories, Kind, MindProject, Share,
    WeaverWorkspace, LEDGER_FILE, WEAVER_DIR,
};
use tempfile::TempDir;

use common::skill;

/// A workspace `hub` in a temporary folder, beside a `home` and a `team`:
///
/// - its library holds `review`, which keeps scripts in a folder of its own;
/// - it links `api`, marked by its settings for Claude and Codex, which
///   authors `deploy` and a rule filed under `team/`, holds a copy of
///   `review` in Claude's folder, and is a repository with everything a
///   repository has beside: `.git`, `node_modules`, `target`, its code;
/// - it links `gone`, whose directory has gone since;
/// - it loads `team`, which offers `notes`.
struct Fixture {
    dir: TempDir,
    hub: WeaverWorkspace,
    api: PathBuf,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = TempDir::new().unwrap();
        let (mut hub, _) = WeaverWorkspace::init(dir.path().join("hub")).unwrap();
        let library = hub.own().directory_for(Kind::Skill).unwrap();
        common::write_files(
            &library,
            &[
                ("review/SKILL.md", &skill("review", "Read it")),
                ("review/scripts/deep/check.sh", "true\n"),
            ],
        );

        common::write_files(
            &hub.weaver_dir().join("templates"),
            &[
                ("release/SKILL.md", &skill("release", "Cut one")),
                ("release/assets/notes.md", "Notes.\n"),
            ],
        );

        let directories = Directories::default()
            .with(Kind::Skill, "skills")
            .with(Kind::Rule, "rules");
        let (api, _) = MindProject::init_settings(
            hub.root().join("api"),
            &[Agent::Claude, Agent::Codex],
            Share::Shared,
            &directories,
        )
        .unwrap();
        hub.link(&api).unwrap();
        common::write_files(
            api.root(),
            &[
                ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
                ("skills/deploy/scripts/deep/run.sh", "true\n"),
                ("rules/team/style.md", "Name things well.\n"),
                (
                    ".claude/skills/review/SKILL.md",
                    &skill("review", "Read it"),
                ),
                ("AGENTS.md", "# Working here\n"),
                (".git/objects/aa/bb", "blob\n"),
                ("node_modules/left-pad/index.js", "module.exports = 0\n"),
                ("target/debug/api", "binary\n"),
                ("src/main.rs", "fn main() {}\n"),
                ("README.md", "# api\n"),
            ],
        );

        let (gone, _) = MindProject::init(hub.root().join("gone")).unwrap();
        hub.link(&gone).unwrap();
        fs::remove_dir_all(gone.root()).unwrap();

        let (team, _) = WeaverWorkspace::init(dir.path().join("team")).unwrap();
        let notes = team.own().directory_for(Kind::Skill).unwrap();
        common::write_files(&notes, &[("notes/SKILL.md", &skill("notes", "Take them"))]);
        hub.load(&team).unwrap();

        let hub = WeaverWorkspace::open(hub.root()).unwrap();
        let api = api.root().to_path_buf();
        Fixture { dir, hub, api }
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }

    fn team(&self) -> WeaverWorkspace {
        WeaverWorkspace::open(self.dir.path().join("team")).unwrap()
    }

    fn watched(&self) -> Vec<Watched> {
        watch::watched(&self.hub, Some(&self.home()), &AgentHomes::default())
    }
}

fn of(scope: &Scope, path: impl Into<PathBuf>) -> Watched {
    Watched {
        path: path.into(),
        scope: scope.clone(),
    }
}

#[test]
fn the_workspace_is_watched_by_its_marker_its_ledger_its_library_its_shelf_and_its_loads() {
    let fixture = Fixture::new();
    let hub = fixture.hub.root();
    let library = fixture.hub.own().directory_for(Kind::Skill).unwrap();
    let team = fixture.team();
    let offered = team.own().directory_for(Kind::Skill).unwrap();

    let watched = fixture.watched();

    let workspace = Scope::Workspace;
    for expected in [
        of(&workspace, hub.join(WEAVER_DIR).join("weaver.toml")),
        of(&workspace, hub.join(WEAVER_DIR).join(LEDGER_FILE)),
        of(&workspace, fixture.hub.gathered_dir(Kind::Skill)),
        of(&workspace, library.clone()),
        of(&workspace, library.join("review")),
        of(&workspace, hub.join(WEAVER_DIR).join("templates")),
        of(
            &workspace,
            hub.join(WEAVER_DIR).join("templates").join("release"),
        ),
        of(&workspace, team.root().join(WEAVER_DIR).join("weaver.toml")),
        of(&workspace, offered.join("notes")),
        // Not made: the default workspace's marker, for when it is.
        of(
            &workspace,
            fixture.home().join(WEAVER_DIR).join("weaver.toml"),
        ),
    ] {
        assert!(watched.contains(&expected), "{expected:?} in {watched:#?}");
    }
}

#[test]
fn a_project_is_watched_by_its_marker_what_its_agents_read_and_where_it_keeps_each_kind() {
    let fixture = Fixture::new();
    let api = &fixture.api;

    let watched = fixture.watched();

    let project = Scope::Project(String::from("api"));
    for expected in [
        of(&project, api.join(".mind").join("settings.toml")),
        of(&project, api.join("AGENTS.md")),
        // Named before they are there, for when they are.
        of(&project, api.join("CLAUDE.md")),
        of(&project, api.join(".mcp.json")),
        of(&project, api.join("opencode.json")),
        of(&project, api.join(".codex").join("config.toml")),
        of(&project, api.join("skills")),
        of(&project, api.join("skills").join("deploy")),
        of(&project, api.join("rules")),
        of(&project, api.join("rules").join("team")),
        of(&project, api.join(".claude").join("skills")),
        of(&project, api.join(".claude").join("skills").join("review")),
        of(&project, api.join(".agents").join("skills")),
        of(&project, api.join(claude_rules::folder())),
        of(&project, api.join(".mind").join("templates")),
    ] {
        assert!(watched.contains(&expected), "{expected:?} in {watched:#?}");
    }
    // An entry that opens no project is watched for its marker coming back.
    let gone = Scope::Project(String::from("gone"));
    let marker = fixture.hub.root().join("gone").join(".mind");
    assert!(watched.contains(&of(&gone, marker.join("settings.toml"))));
    assert!(watched.contains(&of(&gone, marker.join("mind.toml"))));
}

#[test]
fn what_doctor_reads_is_watched_the_agents_own_folders_and_the_project_s_scratch_among_it() {
    let fixture = Fixture::new();
    let api = &fixture.api;
    let homes = AgentHomes::defaults_under(&fixture.home());
    let user = homes.agents_dir.clone().unwrap().join("skills");
    common::write_files(&user, &[("tidy/SKILL.md", &skill("tidy", "Tidy it"))]);

    let watched = watch::watched(&fixture.hub, Some(&fixture.home()), &homes);

    let workspace = Scope::Workspace;
    let project = Scope::Project(String::from("api"));
    let opencode = homes.opencode_config_dir.clone().unwrap();
    for expected in [
        of(
            &workspace,
            homes.codex_home.clone().unwrap().join("config.toml"),
        ),
        of(&workspace, opencode.join("opencode.json")),
        of(&workspace, opencode.join("opencode.jsonc")),
        of(&workspace, opencode.join("config.json")),
        of(&workspace, opencode.join("skills")),
        of(
            &workspace,
            homes.claude_config_dir.clone().unwrap().join("CLAUDE.md"),
        ),
        of(
            &workspace,
            homes.claude_config_dir.clone().unwrap().join("skills"),
        ),
        of(&workspace, user.clone()),
        of(&workspace, user.join("tidy")),
        // Read by OpenCode, which `api` does not name, and written by no
        // install.
        of(&project, api.join(".opencode").join("skills")),
        of(&project, api.join(".mind").join("tmp")),
    ] {
        assert!(watched.contains(&expected), "{expected:?} in {watched:#?}");
    }
    // Each once, however many agents read it.
    assert_eq!(watched.iter().filter(|each| each.path == user).count(), 1);
}

#[test]
fn opencode_s_folders_in_the_home_are_watched_beside_those_its_variables_name() {
    let fixture = Fixture::new();
    let home = fixture.home();
    let opencode = home.join(".config").join("opencode");
    let homes = AgentHomes {
        claude_config_dir: Some(home.join("claude")),
        opencode_config_dir: Some(home.join("opencode")),
        claude_dir_in_home: Some(home.join(".claude")),
        opencode_dir_in_home: Some(opencode.clone()),
        ..AgentHomes::default()
    };

    let watched = watch::watched(&fixture.hub, Some(&home), &homes);

    for expected in [
        opencode.join("opencode.json"),
        opencode.join("skills"),
        home.join("opencode").join("opencode.json"),
        home.join("opencode").join("skills"),
        home.join(".claude").join("skills"),
        home.join("claude").join("skills"),
    ] {
        let expected = of(&Scope::Workspace, expected);
        assert!(watched.contains(&expected), "{expected:?} in {watched:#?}");
    }
}

#[test]
fn what_doctor_reads_above_a_project_is_watched_under_its_scope() {
    let fixture = Fixture::new();
    let api = &fixture.api;
    let hub = fixture.hub.root().to_path_buf();

    let watched = fixture.watched();

    let project = Scope::Project(String::from("api"));
    for expected in [
        // What keeps Claude from `AGENTS.md`, in the project and above it.
        api.join(".claude").join("CLAUDE.md"),
        hub.join("CLAUDE.md"),
        hub.join(".claude").join("CLAUDE.md"),
        hub.join("CLAUDE.local.md"),
        fixture.dir.path().join("CLAUDE.local.md"),
        // `api`'s `.git` is no repository, so Codex finds no root: a `.git`
        // made above would give it one.
        hub.join(".git"),
    ] {
        let expected = of(&project, expected);
        assert!(watched.contains(&expected), "{expected:?} in {watched:#?}");
    }
    assert!(
        !watched.iter().any(|each| each.path == api.join(".git")),
        "a .git that is there is never watched"
    );
}

#[test]
fn what_codex_reads_from_the_git_root_down_is_watched_and_no_git() {
    let dir = TempDir::new().unwrap();
    gix::init(dir.path()).unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let (project, _) = MindProject::init_settings(
        dir.path().join("app"),
        &[Agent::Codex],
        Share::Local,
        &Directories::default(),
    )
    .unwrap();
    workspace.link(&project).unwrap();
    let root = mindweaver_core::paths::canonical(dir.path()).unwrap();

    let watched = watch::watched(&workspace, None, &AgentHomes::default());

    let scope = Scope::Project(String::from("app"));
    for expected in [
        root.join("AGENTS.md"),
        root.join("AGENTS.override.md"),
        root.join("app").join("AGENTS.md"),
        root.join("app").join("AGENTS.override.md"),
    ] {
        let expected = of(&scope, expected);
        assert!(watched.contains(&expected), "{expected:?} in {watched:#?}");
    }
    assert!(
        !watched.iter().any(|each| each
            .path
            .components()
            .any(|part| part.as_os_str() == ".git")),
        "{watched:#?}"
    );
}

/// The acceptance's own: a folder is watched for what is directly in it, so
/// no folder named may be a root whose every file is somebody's code, and
/// none may sit deeper than discovery looks.
#[test]
fn no_path_named_covers_a_whole_tree() {
    let fixture = Fixture::new();
    let hub = fixture.hub.root().to_path_buf();
    let api = fixture.api.clone();
    let library = fixture.hub.own().directory_for(Kind::Skill).unwrap();

    let watched = fixture.watched();
    let paths: Vec<&Path> = watched.iter().map(|each| each.path.as_path()).collect();

    for whole in [
        fixture.dir.path().to_path_buf(),
        hub.clone(),
        hub.join(WEAVER_DIR),
        fixture.hub.cache_dir(),
        api.clone(),
        api.join(".mind"),
        api.join(".claude"),
        api.join("src"),
        api.join("skills").join("deploy").join("scripts"),
        library.join("review").join("scripts"),
        hub.join(WEAVER_DIR)
            .join("templates")
            .join("release")
            .join("assets"),
    ] {
        assert!(!paths.contains(&whole.as_path()), "{whole:?} is named");
    }
    for path in &paths {
        // A `.git` is named only while it is not there, for its being made,
        // as doctor says a project Codex finds no root for: never one that
        // is, nor anything in one.
        let unmade_git = path.file_name().is_some_and(|name| name == ".git")
            && fs::symlink_metadata(path).is_err()
            && !path
                .parent()
                .is_some_and(|folder| folder.components().any(|each| each.as_os_str() == ".git"));
        for part in [".git", "node_modules", "target", "scripts"] {
            assert!(
                (unmade_git && part == ".git")
                    || !path.components().any(|each| each.as_os_str() == part),
                "{path:?} is inside a {part}"
            );
        }
    }

    // Every folder named is one a holder keeps a kind in, or the shelf's or
    // the templates', and below a folder of skills, only each skill's own.
    let holders = [fixture.hub.own(), MindProject::open(&api).unwrap()];
    let mut roots: Vec<(PathBuf, Option<Kind>)> = Vec::new();
    for holder in holders.iter().chain([&fixture.team().own()]) {
        for kind in Kind::ALL {
            for folder in Catalog::folders(holder, kind) {
                roots.push((folder, Some(kind)));
            }
        }
    }
    roots.push((api.join(claude_rules::folder()), Some(Kind::Rule)));
    // And doctor's: every folder an agent reads skills from, and the
    // scratch folder an install stages in, whose stages it says.
    for agent in Agent::ALL {
        for folder in agent.reads(Kind::Skill) {
            roots.push((api.join(folder), Some(Kind::Skill)));
        }
    }
    roots.push((api.join(".mind").join("tmp"), None));
    for kind in Kind::ALL {
        roots.push((fixture.hub.gathered_dir(kind), None));
    }
    roots.push((hub.join(WEAVER_DIR).join("templates"), Some(Kind::Skill)));
    roots.push((api.join(".mind").join("templates"), Some(Kind::Skill)));
    roots.push((hub.join(".mind").join("templates"), Some(Kind::Skill)));
    for path in paths.iter().filter(|path| path.is_dir()) {
        let under = roots.iter().find(|(root, kind)| match kind {
            Some(Kind::Skill) => *path == root || path.parent() == Some(root.as_path()),
            Some(Kind::Rule) => path.starts_with(root),
            Some(Kind::Mcp) | None => *path == root,
        });
        assert!(under.is_some(), "{path:?} is no folder discovery looks in");
    }
}

/// A name, and the size and time of what it names: what a watch sees change.
type Look = (String, u64, Option<SystemTime>);

/// What each path named holds, as far as a watch would see a change to it:
/// a file's size and time, and for a folder, each name directly in it, with
/// the same of each.
fn seen(watched: &[Watched]) -> Vec<(PathBuf, Vec<Look>)> {
    let look = |path: &Path| {
        let metadata = fs::metadata(path).ok();
        (
            metadata.as_ref().map_or(0, fs::Metadata::len),
            metadata.and_then(|metadata| metadata.modified().ok()),
        )
    };
    watched
        .iter()
        .map(|each| {
            let mut found = vec![{
                let (len, modified) = look(&each.path);
                (String::new(), len, modified)
            }];
            if each.path.is_dir() {
                let mut inside: Vec<Look> = fs::read_dir(&each.path)
                    .unwrap()
                    .map(|entry| {
                        let entry = entry.unwrap();
                        let (len, modified) = look(&entry.path());
                        (
                            entry.file_name().to_string_lossy().into_owned(),
                            len,
                            modified,
                        )
                    })
                    .collect();
                inside.sort();
                found.extend(inside);
            }
            (each.path.clone(), found)
        })
        .collect()
}

/// A snapshot changing something watched would set off another snapshot,
/// and that one another, for as long as the panel is open. SQLite makes its
/// log and shared memory beside the ledger to read it and takes them away
/// as the last connection closes, which a watch on the ledger's folder
/// would see however quickly they go: so only the file is named, and with
/// nothing in the log, closing writes nothing to it.
#[test]
fn reading_a_snapshot_changes_nothing_watched() {
    let fixture = Fixture::new();
    // A ledger that is there, as a write leaves one, its log checkpointed.
    fixture.hub.ledger().unwrap();
    let watched = fixture.watched();
    let before = seen(&watched);

    let resolved = Resolved {
        workspace: WeaverWorkspace::open(fixture.hub.root()).unwrap(),
        reason: Reason::Explicit,
    };
    Snapshot::of(&resolved, Some(&fixture.home()), &Machine::default());

    assert_eq!(seen(&watched), before);
    let ledger = fixture.hub.root().join(WEAVER_DIR).join(LEDGER_FILE);
    let beside = ledger.parent().unwrap();
    assert!(watched.iter().any(|each| each.path == ledger));
    assert!(!watched.iter().any(|each| each.path == beside));
    for log in ["-wal", "-shm"] {
        let mut name = ledger.clone().into_os_string();
        name.push(log);
        assert!(!watched.iter().any(|each| each.path.as_os_str() == name));
    }
}

#[test]
fn a_project_linked_since_is_watched_once_the_list_is_asked_again() {
    let fixture = Fixture::new();
    let mut hub = WeaverWorkspace::open(fixture.hub.root()).unwrap();
    let before = watch::watched(&hub, None, &AgentHomes::default());
    let (web, _) = MindProject::init(fixture.dir.path().join("web")).unwrap();
    hub.link(&web).unwrap();

    let after = watch::watched(
        &WeaverWorkspace::open(hub.root()).unwrap(),
        None,
        &AgentHomes::default(),
    );

    let scope = Scope::Project(String::from("../web"));
    assert!(!before.iter().any(|each| each.scope == scope));
    let marker = web.root().join(".mind").join("mind.toml");
    assert!(after.contains(&of(&scope, marker)));
}
