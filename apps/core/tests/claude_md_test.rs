//! Whether Claude Code reads a project's `AGENTS.md`: each file that keeps it
//! from doing so, wherever above the project it is, and the user memory file,
//! which never does; and how Claude is to reach the block of rules there,
//! natively, through an import in the project's own `CLAUDE.md`, or not at
//! all.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use mindweaver_core::agents_md;
use mindweaver_core::claude_md::{
    self, ClaudeMd, ClaudeMdError, Fallback, Reach, Warning, Written,
};
use mindweaver_core::claude_rules::Mode;
use mindweaver_core::{
    Agent, AgentHomes, Artifact, Directories, MindProject, Share, WeaverWorkspace,
};
use tempfile::TempDir;

/// No agent's own folder, as a front end hands over when a test pins them.
const NO_HOMES: AgentHomes = AgentHomes {
    claude_config_dir: None,
    codex_home: None,
    opencode_config_dir: None,
    agents_dir: None,
    claude_dir_in_home: None,
    opencode_dir_in_home: None,
};

/// A folder `route` below `root`, made.
fn folder(root: &Path, route: &str) -> PathBuf {
    let path = root.join(route);
    fs::create_dir_all(&path).unwrap();
    path
}

/// `contents` at `route` below `root`, with the folders it needs.
fn write(root: &Path, route: &str, contents: &str) -> PathBuf {
    let path = root.join(route);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, contents).unwrap();
    path
}

fn reads(folder: &Path, homes: &AgentHomes) -> bool {
    claude_md::reads_agents_md(folder, homes).unwrap()
}

/// A temporary folder, held to have nothing above it that Claude would take
/// for a `CLAUDE.md`, or every test here would say so for the wrong reason.
fn clean() -> TempDir {
    let dir = TempDir::new().unwrap();
    assert!(
        reads(dir.path(), &NO_HOMES),
        "a CLAUDE.md, .claude/CLAUDE.md or CLAUDE.local.md above {} keeps these tests \
         from telling anything",
        dir.path().display()
    );
    dir
}

#[test]
fn claude_reads_agents_md_where_no_claude_md_is_found() {
    let dir = clean();
    let app = folder(dir.path(), "work/app");
    write(&app, "AGENTS.md", "# Working here\n");
    // None of these is a CLAUDE.md to Claude's check.
    write(&app, ".claude/AGENTS.md", "# Mine\n");
    write(&app, ".claude/rules/mindweaver/style.md", "Be brief.\n");
    write(
        &app,
        ".claude/skills/review/SKILL.md",
        "---\nname: review\n---\n",
    );
    write(&app, "docs/CLAUDE.md", "# Below\n");
    folder(&app, "CLAUDE");
    assert!(reads(&app, &NO_HOMES));
}

#[test]
fn each_name_in_the_project_keeps_claude_from_reading_it() {
    for name in ["CLAUDE.md", ".claude/CLAUDE.md", "CLAUDE.local.md"] {
        let dir = clean();
        let app = folder(dir.path(), "app");
        write(&app, name, "# Team\n");
        assert!(!reads(&app, &NO_HOMES), "{name}");
    }
}

#[test]
fn each_name_in_any_folder_above_keeps_claude_from_reading_it() {
    for name in ["CLAUDE.md", ".claude/CLAUDE.md", "CLAUDE.local.md"] {
        let dir = clean();
        // A stand-in for ~/Work, above the git root and the project below it.
        let work = folder(dir.path(), "Work");
        let repo = folder(&work, "repo");
        gix::init(&repo).unwrap();
        let app = folder(&repo, "packages/app");
        write(&work, name, "# Everything I work on\n");
        assert!(!reads(&app, &NO_HOMES), "{name}");
        assert!(!reads(&repo, &NO_HOMES), "{name}");
    }
}

#[cfg(unix)]
#[test]
fn a_file_found_through_a_link_counts_and_a_link_to_nothing_does_not() {
    let dir = clean();
    let app = folder(dir.path(), "app");
    let team = write(dir.path(), "team/CLAUDE.md", "# Team\n");
    std::os::unix::fs::symlink(&team, app.join("CLAUDE.md")).unwrap();
    assert!(!reads(&app, &NO_HOMES));

    let other = folder(dir.path(), "other");
    std::os::unix::fs::symlink(dir.path().join("gone.md"), other.join("CLAUDE.md")).unwrap();
    assert!(reads(&other, &NO_HOMES));
}

#[test]
fn the_user_memory_file_passed_in_never_counts() {
    let dir = clean();
    // A stand-in for the user's home, with the memory file where Claude keeps
    // it, and a project under that home, as most projects are.
    let home = folder(dir.path(), "home");
    write(&home, ".claude/CLAUDE.md", "# Everywhere\n");
    let app = folder(&home, "work/app");
    let homes = AgentHomes::defaults_under(&home);
    assert!(reads(&app, &homes));

    // Not passed in, it is the home's own `.claude/CLAUDE.md`, like any
    // other above the project.
    assert!(!reads(&app, &NO_HOMES));
    // And the memory file passed in leaves out only itself.
    write(&home, "CLAUDE.md", "# Home\n");
    assert!(!reads(&app, &homes));
}

#[cfg(unix)]
#[test]
fn the_user_memory_file_named_through_a_link_is_still_left_out() {
    let dir = clean();
    let home = folder(dir.path(), "home");
    write(&home, ".claude/CLAUDE.md", "# Everywhere\n");
    let app = folder(&home, "work/app");
    let linked = dir.path().join("config");
    std::os::unix::fs::symlink(home.join(".claude"), &linked).unwrap();
    let homes = AgentHomes {
        claude_config_dir: Some(linked),
        ..NO_HOMES
    };
    assert!(reads(&app, &homes));
}

#[test]
fn claude_moved_elsewhere_leaves_the_homes_own_file_counting() {
    let dir = clean();
    let home = folder(dir.path(), "home");
    write(&home, ".claude/CLAUDE.md", "# Not Claude's any more\n");
    write(dir.path(), "elsewhere/CLAUDE.md", "# Everywhere\n");
    let app = folder(&home, "work/app");
    let homes = AgentHomes {
        claude_config_dir: Some(dir.path().join("elsewhere")),
        ..NO_HOMES
    };
    assert!(!reads(&app, &homes));
}

#[test]
fn a_folder_that_is_not_there_cannot_be_told() {
    let dir = clean();
    let error = claude_md::reads_agents_md(&dir.path().join("gone"), &NO_HOMES).unwrap_err();
    assert!(error.to_string().contains("gone"), "{error}");
}

/// What a team's `CLAUDE.md` holds before Mindweaver writes in it.
const TEAM: &str = "# Working here\n\nRun `make test` before every push.\n";

/// A project at `app` in a git work tree, its settings sharing as `share`
/// says, in a folder with nothing above it in Claude's way.
struct Fixture {
    dir: TempDir,
    project: MindProject,
}

fn fixture(share: Share) -> Fixture {
    let dir = clean();
    gix::init(dir.path()).unwrap();
    let root = folder(dir.path(), "app");
    let (project, _) =
        MindProject::init_settings(&root, &Agent::ALL, share, &Directories::default()).unwrap();
    Fixture { dir, project }
}

impl Fixture {
    fn root(&self) -> &Path {
        self.project.root()
    }

    fn file(&self, name: &str) -> PathBuf {
        self.root().join(name)
    }

    /// Whatever is in the work tree, put in the index: git tracks it.
    fn track(&self) {
        common::indexed(self.dir.path());
    }

    fn reach(&self, block_written: bool) -> Reach {
        claude_md::reach(&self.project, &NO_HOMES, block_written).unwrap()
    }

    fn render(&self, block_written: bool) -> claude_md::Rendered {
        claude_md::render(&self.project, &NO_HOMES, block_written).unwrap()
    }
}

/// The three names Claude's check looks for in a folder.
const NAMES: [&str; 3] = ["CLAUDE.md", ".claude/CLAUDE.md", "CLAUDE.local.md"];

/// The reach a local project without `edit_tracked` is to have, holding the
/// files `present` names, tracked by git or not: the import in the first of
/// its own `CLAUDE.md` and `.claude/CLAUDE.md` while git does not track it,
/// and otherwise the rules folder, said for that file; with neither, the
/// rules folder for a `CLAUDE.local.md`, and `AGENTS.md` read natively
/// without one.
fn expected(fixture: &Fixture, present: &[&str], tracked: bool) -> Reach {
    let own = ["CLAUDE.md", ".claude/CLAUDE.md"]
        .into_iter()
        .find(|name| present.contains(name));
    match own {
        Some(name) if tracked => Reach::Fallback(Fallback::Tracked(fixture.file(name))),
        Some(name) => Reach::Import(fixture.file(name)),
        None if present.contains(&"CLAUDE.local.md") => {
            Reach::Fallback(Fallback::LocalOnly(fixture.file("CLAUDE.local.md")))
        }
        None => Reach::Native,
    }
}

/// Every combination of the three names, as the names present.
fn combinations() -> Vec<Vec<&'static str>> {
    (0..1 << NAMES.len())
        .map(|mask| {
            NAMES
                .iter()
                .enumerate()
                .filter(|(bit, _)| mask & (1 << bit) != 0)
                .map(|(_, name)| *name)
                .collect()
        })
        .collect()
}

#[test]
fn each_combination_of_claudes_files_tracked_or_not_chooses_its_reach() {
    for present in combinations() {
        for tracked in [false, true] {
            let fixture = fixture(Share::Local);
            for name in &present {
                write(fixture.root(), name, TEAM);
            }
            if tracked {
                fixture.track();
            }
            let reach = fixture.reach(true);
            assert_eq!(
                reach,
                expected(&fixture, &present, tracked),
                "{present:?}, tracked: {tracked}"
            );
            let mode = if matches!(reach, Reach::Fallback(_)) {
                Mode::Fallback
            } else {
                Mode::Scoped
            };
            assert_eq!(reach.mode(), mode);
        }
    }
}

#[test]
fn a_tracked_claude_md_takes_the_import_in_a_shared_project_or_with_edit_tracked() {
    for present in combinations() {
        for edit_tracked in [false, true] {
            let share = if edit_tracked {
                Share::Local
            } else {
                Share::Shared
            };
            let mut fixture = fixture(share);
            if edit_tracked {
                fixture
                    .project
                    .change_settings(|settings| settings.edit_tracked = true)
                    .unwrap();
            }
            for name in &present {
                write(fixture.root(), name, TEAM);
            }
            fixture.track();
            assert_eq!(
                fixture.reach(true),
                expected(&fixture, &present, false),
                "{present:?}, edit_tracked: {edit_tracked}"
            );
        }
    }
}

#[test]
fn a_tracked_claude_md_gives_way_to_a_dot_claude_one_git_does_not_track() {
    let fixture = fixture(Share::Local);
    write(fixture.root(), "CLAUDE.md", TEAM);
    fixture.track();
    write(fixture.root(), ".claude/CLAUDE.md", TEAM);
    assert_eq!(
        fixture.reach(true),
        Reach::Import(fixture.file(".claude/CLAUDE.md"))
    );
}

#[test]
fn a_project_a_mind_toml_marks_was_committed_by_design_and_takes_the_import() {
    let dir = clean();
    gix::init(dir.path()).unwrap();
    let root = folder(dir.path(), "app");
    let (project, _) = MindProject::init(&root).unwrap();
    write(&root, "CLAUDE.md", TEAM);
    common::indexed(dir.path());
    assert_eq!(
        claude_md::reach(&project, &NO_HOMES, true).unwrap(),
        Reach::Import(root.join("CLAUDE.md"))
    );
}

#[test]
fn no_block_written_is_the_fallback_whatever_the_files_say() {
    for present in combinations() {
        let fixture = fixture(Share::Shared);
        for name in &present {
            write(fixture.root(), name, TEAM);
        }
        assert_eq!(
            fixture.reach(false),
            Reach::Fallback(Fallback::NoBlock),
            "{present:?}"
        );
        assert_eq!(fixture.reach(false).mode(), Mode::Fallback);
    }
}

#[test]
fn a_claude_md_above_the_git_root_gives_the_fallback() {
    let dir = clean();
    // A stand-in for ~/Work, holding what its owner wants every agent to know.
    let work = write(dir.path(), "Work/CLAUDE.md", TEAM);
    let repo = folder(dir.path(), "Work/repo");
    gix::init(&repo).unwrap();
    let (project, _) =
        MindProject::init_settings(&repo, &Agent::ALL, Share::Shared, &Directories::default())
            .unwrap();
    let reach = claude_md::reach(&project, &NO_HOMES, true).unwrap();
    assert_eq!(reach, Reach::Fallback(Fallback::Above(resolved(&work))));
    assert_eq!(reach.mode(), Mode::Fallback);
}

#[test]
fn a_project_nested_under_a_repository_root_claude_md_gives_the_fallback() {
    let dir = clean();
    gix::init(dir.path()).unwrap();
    let team = write(dir.path(), "CLAUDE.md", TEAM);
    let app = folder(dir.path(), "packages/app");
    let (project, _) =
        MindProject::init_settings(&app, &Agent::ALL, Share::Shared, &Directories::default())
            .unwrap();
    assert_eq!(
        claude_md::reach(&project, &NO_HOMES, true).unwrap(),
        Reach::Fallback(Fallback::Above(resolved(&team)))
    );
}

#[test]
fn a_claude_local_md_nearer_than_one_above_is_the_reason_given() {
    let fixture = fixture(Share::Shared);
    write(fixture.dir.path(), "CLAUDE.md", TEAM);
    write(fixture.root(), "CLAUDE.local.md", TEAM);
    assert_eq!(
        fixture.reach(true),
        Reach::Fallback(Fallback::LocalOnly(fixture.file("CLAUDE.local.md")))
    );
}

#[test]
fn a_project_with_a_claude_md_of_its_own_takes_the_import_whatever_is_above() {
    let fixture = fixture(Share::Shared);
    write(fixture.dir.path(), "CLAUDE.md", TEAM);
    write(fixture.root(), "CLAUDE.md", TEAM);
    assert_eq!(
        fixture.reach(true),
        Reach::Import(fixture.file("CLAUDE.md"))
    );
}

#[test]
fn the_user_memory_file_passed_in_never_counts_for_the_reach() {
    let dir = clean();
    let home = folder(dir.path(), "home");
    write(&home, ".claude/CLAUDE.md", TEAM);
    let app = folder(&home, "work/app");
    let (project, _) =
        MindProject::init_settings(&app, &Agent::ALL, Share::Shared, &Directories::default())
            .unwrap();
    let homes = AgentHomes::defaults_under(&home);
    assert_eq!(
        claude_md::reach(&project, &homes, true).unwrap(),
        Reach::Native
    );
}

#[test]
fn the_user_memory_file_in_a_project_at_the_home_is_never_its_claude_md() {
    let dir = clean();
    let home = folder(dir.path(), "home");
    write(&home, ".claude/CLAUDE.md", TEAM);
    let (project, _) =
        MindProject::init_settings(&home, &Agent::ALL, Share::Shared, &Directories::default())
            .unwrap();
    let homes = AgentHomes::defaults_under(&home);
    assert_eq!(
        claude_md::reach(&project, &homes, true).unwrap(),
        Reach::Native
    );
}

#[test]
fn an_import_already_in_a_tracked_file_is_how_claude_reaches_the_block() {
    let fixture = fixture(Share::Local);
    let imported =
        format!("<!-- mindweaver:begin -->\n@AGENTS.md\n<!-- mindweaver:end -->\n\n{TEAM}");
    write(fixture.root(), ".claude/CLAUDE.md", TEAM);
    write(fixture.root(), "CLAUDE.md", &imported);
    fixture.track();
    // Committed with the import in it: Claude reads it there, so its rules
    // folder must not give them a second time.
    assert_eq!(
        fixture.reach(true),
        Reach::Import(fixture.file("CLAUDE.md"))
    );
}

#[test]
fn an_import_in_the_second_file_keeps_it_there() {
    let fixture = fixture(Share::Shared);
    let imported =
        format!("<!-- mindweaver:begin -->\n@../AGENTS.md\n<!-- mindweaver:end -->\n\n{TEAM}");
    write(fixture.root(), "CLAUDE.md", TEAM);
    write(fixture.root(), ".claude/CLAUDE.md", &imported);
    assert_eq!(
        fixture.reach(true),
        Reach::Import(fixture.file(".claude/CLAUDE.md"))
    );
}

/// A team's `CLAUDE.md` sharing `AGENTS.md` with Claude, as Claude's docs
/// show it: the import on a line of its own, above the team's own text.
const SHARING: &str = "@AGENTS.md\n\n# Working here\n\nRun `make test` before every push.\n";

#[test]
fn a_tracked_claude_md_that_imports_agents_md_itself_is_how_claude_reaches_the_block() {
    let fixture = fixture(Share::Local);
    write(fixture.root(), "CLAUDE.md", SHARING);
    fixture.track();
    // Claude reads the block through the team's own line, so its rules
    // folder must not give the rules about every file a second time.
    let reach = fixture.reach(true);
    assert_eq!(reach, Reach::Imported(fixture.file("CLAUDE.md")));
    assert_eq!(reach.mode(), Mode::Scoped);

    for block_written in [true, false] {
        let rendered = fixture.render(block_written);
        assert_eq!(rendered.files[0].written, Written::Unchanged);
        assert!(rendered.warnings.is_empty());
        assert_eq!(
            fs::read_to_string(fixture.file("CLAUDE.md")).unwrap(),
            SHARING
        );
    }
}

#[test]
fn a_dot_claude_md_that_imports_agents_md_itself_is_how_claude_reaches_the_block() {
    let fixture = fixture(Share::Local);
    write(fixture.root(), "CLAUDE.md", TEAM);
    let sharing = SHARING.replace("@AGENTS.md", "@../AGENTS.md");
    write(fixture.root(), ".claude/CLAUDE.md", &sharing);
    fixture.track();
    assert_eq!(
        fixture.reach(true),
        Reach::Imported(fixture.file(".claude/CLAUDE.md"))
    );
}

#[test]
fn a_claude_md_that_imports_agents_md_itself_is_never_given_a_second_import() {
    let fixture = fixture(Share::Shared);
    write(fixture.root(), "CLAUDE.md", SHARING);
    let rendered = fixture.render(true);
    assert_eq!(rendered.reach, Reach::Imported(fixture.file("CLAUDE.md")));
    assert_eq!(rendered.files[0].written, Written::Unchanged);
    assert_eq!(
        fs::read_to_string(fixture.file("CLAUDE.md")).unwrap(),
        SHARING
    );
}

#[test]
fn mindweavers_import_beside_one_somebody_wrote_is_taken_out() {
    let fixture = fixture(Share::Shared);
    // What an earlier render made of the team's file, and their own import
    // in the other file.
    write(
        fixture.root(),
        "CLAUDE.md",
        &format!("<!-- mindweaver:begin -->\n@AGENTS.md\n<!-- mindweaver:end -->\n\n{SHARING}"),
    );
    let sharing = SHARING.replace("@AGENTS.md", "@../AGENTS.md");
    write(fixture.root(), ".claude/CLAUDE.md", &sharing);
    let rendered = fixture.render(true);
    assert_eq!(rendered.reach, Reach::Imported(fixture.file("CLAUDE.md")));
    assert_eq!(rendered.files[0].written, Written::Removed);
    assert_eq!(rendered.files[1].written, Written::Unchanged);
    assert_eq!(
        fs::read_to_string(fixture.file("CLAUDE.md")).unwrap(),
        SHARING
    );
    assert_eq!(
        fs::read_to_string(fixture.file(".claude/CLAUDE.md")).unwrap(),
        sharing
    );
}

#[test]
fn a_claude_local_md_that_imports_agents_md_is_how_claude_reaches_the_block() {
    // Alone, beside a tracked CLAUDE.md that may not take the import, and
    // below a CLAUDE.md above the project.
    for beside in [None, Some("CLAUDE.md"), Some("../CLAUDE.md")] {
        let fixture = fixture(Share::Local);
        if let Some(name) = beside {
            write(fixture.root(), name, TEAM);
        }
        write(fixture.root(), "CLAUDE.local.md", SHARING);
        fixture.track();
        let reach = fixture.reach(true);
        assert_eq!(
            reach,
            Reach::Imported(fixture.file("CLAUDE.local.md")),
            "{beside:?}"
        );
        assert_eq!(reach.mode(), Mode::Scoped);
        fixture.render(true);
        assert_eq!(
            fs::read_to_string(fixture.file("CLAUDE.local.md")).unwrap(),
            SHARING
        );
        if let Some(name) = beside {
            assert_eq!(fs::read_to_string(fixture.file(name)).unwrap(), TEAM);
        }
    }
}

#[test]
fn a_claude_md_that_may_take_the_import_takes_it_before_a_claude_local_md_does() {
    // A person's own file reaches only them: a teammate cloning a shared
    // project needs the import in the file they share.
    let fixture = fixture(Share::Shared);
    write(fixture.root(), "CLAUDE.md", TEAM);
    write(fixture.root(), "CLAUDE.local.md", SHARING);
    assert_eq!(
        fixture.reach(true),
        Reach::Import(fixture.file("CLAUDE.md"))
    );
}

#[test]
fn an_import_quoted_in_a_code_fence_is_not_how_claude_reaches_the_block() {
    let fixture = fixture(Share::Local);
    let quoted = format!("{TEAM}\n```markdown\n@AGENTS.md\n```\n");
    write(fixture.root(), "CLAUDE.md", &quoted);
    fixture.track();
    assert_eq!(
        fixture.reach(true),
        Reach::Fallback(Fallback::Tracked(fixture.file("CLAUDE.md")))
    );
}

#[test]
fn a_malformed_block_in_the_projects_claude_md_is_refused() {
    let fixture = fixture(Share::Shared);
    write(
        fixture.root(),
        "CLAUDE.md",
        "<!-- mindweaver:begin -->\n@AGENTS.md\n",
    );
    let error = claude_md::reach(&fixture.project, &NO_HOMES, true).unwrap_err();
    assert!(matches!(error, ClaudeMdError::Malformed { .. }), "{error}");
}

#[cfg(unix)]
#[test]
fn a_claude_md_that_is_the_agents_md_reads_the_block_as_it_is() {
    for (link, target) in [("CLAUDE.md", "AGENTS.md"), ("AGENTS.md", "CLAUDE.md")] {
        let fixture = fixture(Share::Local);
        write(fixture.root(), target, TEAM);
        std::os::unix::fs::symlink(target, fixture.file(link)).unwrap();
        fixture.track();
        let reach = fixture.reach(true);
        assert_eq!(reach, Reach::Same(fixture.file("CLAUDE.md")), "{link}");
        assert_eq!(reach.mode(), Mode::Scoped);
    }
}

#[cfg(unix)]
#[test]
fn a_dot_claude_md_that_is_the_agents_md_wins_over_a_claude_md_to_import_into() {
    let fixture = fixture(Share::Shared);
    write(fixture.root(), "AGENTS.md", TEAM);
    write(fixture.root(), "CLAUDE.md", TEAM);
    folder(fixture.root(), ".claude");
    std::os::unix::fs::symlink("../AGENTS.md", fixture.file(".claude/CLAUDE.md")).unwrap();
    assert_eq!(
        fixture.reach(true),
        Reach::Same(fixture.file(".claude/CLAUDE.md"))
    );
}

#[cfg(unix)]
#[test]
fn a_claude_md_linked_to_another_file_in_the_project_is_never_imported_into() {
    let fixture = fixture(Share::Shared);
    let docs = write(fixture.root(), "docs/claude.md", TEAM);
    std::os::unix::fs::symlink("docs/claude.md", fixture.file("CLAUDE.md")).unwrap();
    assert_eq!(
        fixture.reach(true),
        Reach::Fallback(Fallback::Linked {
            file: fixture.file("CLAUDE.md"),
            target: resolved(&docs),
        })
    );
}

#[cfg(unix)]
#[test]
fn a_dot_claude_linked_out_of_the_project_is_never_imported_into() {
    let fixture = fixture(Share::Shared);
    let elsewhere = write(fixture.dir.path(), "config/claude/CLAUDE.md", TEAM);
    std::os::unix::fs::symlink(
        fixture.dir.path().join("config/claude"),
        fixture.file(".claude"),
    )
    .unwrap();
    assert_eq!(
        fixture.reach(true),
        Reach::Fallback(Fallback::Linked {
            file: fixture.file(".claude/CLAUDE.md"),
            target: resolved(&elsewhere),
        })
    );
}

/// `path` resolved, as the walk up from a project spells what it finds.
fn resolved(path: &Path) -> PathBuf {
    mindweaver_core::paths::canonical(path).unwrap()
}

/// The line that imports `AGENTS.md` into the file at `name`.
fn import_for(name: &str) -> &'static str {
    match name {
        "CLAUDE.md" => "@AGENTS.md",
        ".claude/CLAUDE.md" => "@../AGENTS.md",
        _ => unreachable!("no import goes in {name}"),
    }
}

/// What the file at `name` holds before Mindweaver writes in it: the team's
/// text and its name, so no two are alike, in CRLF for one of them.
fn original(name: &str) -> String {
    let text = format!("{TEAM}\nThis is {name}.\n");
    if name == ".claude/CLAUDE.md" {
        text.replace('\n', "\r\n")
    } else {
        text
    }
}

#[test]
fn each_combination_changes_only_the_file_the_reach_names_and_nothing_outside_its_markers() {
    for present in combinations() {
        for tracked in [false, true] {
            let fixture = fixture(Share::Local);
            for name in &present {
                write(fixture.root(), name, &original(name));
            }
            if tracked {
                fixture.track();
            }
            let context = format!("{present:?}, tracked: {tracked}");

            let rendered = fixture.render(true);
            let reach = expected(&fixture, &present, tracked);
            assert_eq!(rendered.reach, reach, "{context}");
            for name in &present {
                let file = fixture.file(name);
                let now = fs::read(&file).unwrap();
                if reach == Reach::Import(file.clone()) {
                    let mut text = ClaudeMd::parse(&now).unwrap();
                    assert!(text.imports(import_for(name)), "{context}: {name}");
                    text.drop_import();
                    assert_eq!(text.to_bytes(), original(name).as_bytes(), "{context}");
                } else {
                    assert_eq!(now, original(name).as_bytes(), "{context}: {name}");
                }
            }
            let written: Vec<(PathBuf, Written)> = rendered
                .files
                .iter()
                .map(|file| (file.path.clone(), file.written))
                .collect();
            let own: Vec<(PathBuf, Written)> = ["CLAUDE.md", ".claude/CLAUDE.md"]
                .into_iter()
                .filter(|name| present.contains(name))
                .map(|name| {
                    let file = fixture.file(name);
                    let added = reach == Reach::Import(file.clone());
                    let written = if added {
                        Written::Added
                    } else {
                        Written::Unchanged
                    };
                    (file, written)
                })
                .collect();
            assert_eq!(written, own, "{context}");
            assert!(rendered.warnings.is_empty(), "{context}");

            // No block any more: the import goes, and every file is as it was.
            let rendered = fixture.render(false);
            assert_eq!(rendered.reach, Reach::Fallback(Fallback::NoBlock));
            for name in &present {
                let now = fs::read(fixture.file(name)).unwrap();
                assert_eq!(now, original(name).as_bytes(), "{context}: {name}");
            }
        }
    }
}

#[test]
fn a_render_that_changes_nothing_writes_nothing() {
    let fixture = fixture(Share::Shared);
    write(fixture.root(), "CLAUDE.md", TEAM);
    fixture.render(true);
    let once = fs::read(fixture.file("CLAUDE.md")).unwrap();
    let again = fixture.render(true);
    assert_eq!(again.files[0].written, Written::Unchanged);
    assert_eq!(fs::read(fixture.file("CLAUDE.md")).unwrap(), once);
}

#[test]
fn a_dot_claude_md_imports_the_agents_md_in_the_folder_above_it() {
    let fixture = fixture(Share::Shared);
    write(fixture.root(), ".claude/CLAUDE.md", TEAM);
    fixture.render(true);
    assert_eq!(
        fs::read_to_string(fixture.file(".claude/CLAUDE.md")).unwrap(),
        format!("<!-- mindweaver:begin -->\n@../AGENTS.md\n<!-- mindweaver:end -->\n\n{TEAM}")
    );
}

/// A workspace linking one project below it, with the team's `CLAUDE.md`,
/// and a library of rules to render from.
struct Rules {
    _dir: TempDir,
    workspace: WeaverWorkspace,
    project: MindProject,
    library: PathBuf,
}

impl Rules {
    fn new() -> Self {
        let dir = clean();
        gix::init(dir.path()).unwrap();
        let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
        let root = folder(dir.path(), "app");
        let (project, _) =
            MindProject::init_settings(&root, &Agent::ALL, Share::Local, &Directories::default())
                .unwrap();
        workspace.link(&project).unwrap();
        write(&root, "CLAUDE.md", TEAM);
        Rules {
            library: dir.path().join("library"),
            _dir: dir,
            workspace,
            project,
        }
    }

    fn rule(&self, route: &str) -> Artifact {
        let file = write(&self.library, &format!("{route}.md"), "Be brief.\n");
        Artifact::rule(&file, route.to_owned(), &self.library).unwrap()
    }

    /// The block rendered for `rules`, then the import as Claude is to
    /// reach it, as a caller does once a rule is installed or uninstalled.
    fn render(&self, rules: &[&Artifact]) -> claude_md::Rendered {
        let block = agents_md::render(
            &self.workspace,
            &self.project,
            rules.iter().copied(),
            false,
            &NO_HOMES,
        )
        .unwrap();
        claude_md::render(&self.project, &NO_HOMES, !block.routes.is_empty()).unwrap()
    }

    fn claude_md(&self) -> String {
        fs::read_to_string(self.project.root().join("CLAUDE.md")).unwrap()
    }
}

#[test]
fn the_import_is_added_once_however_many_rules_and_is_gone_after_the_last() {
    let fixture = Rules::new();
    let rules = [
        fixture.rule("commit-style"),
        fixture.rule("git/no-force-push"),
        fixture.rule("review/tone"),
    ];
    for count in 1..=rules.len() {
        let given: Vec<&Artifact> = rules[..count].iter().collect();
        let rendered = fixture.render(&given);
        assert_eq!(
            rendered.reach,
            Reach::Import(fixture.project.root().join("CLAUDE.md"))
        );
        let text = fixture.claude_md();
        assert_eq!(text.matches("@AGENTS.md").count(), 1, "{count} rules");
        assert_eq!(text.matches(agents_md::BEGIN).count(), 1, "{count} rules");
        assert!(text.ends_with(TEAM));
    }
    // One rule uninstalled after another, the import staying till the last.
    for count in (0..rules.len()).rev() {
        let given: Vec<&Artifact> = rules[..count].iter().collect();
        fixture.render(&given);
        let imported = fixture.claude_md().contains("@AGENTS.md");
        assert_eq!(imported, count > 0, "{count} rules left");
    }
    assert_eq!(fixture.claude_md(), TEAM);
}

#[test]
fn nothing_above_the_project_is_edited_whatever_the_reach() {
    let dir = clean();
    // A stand-in for ~/Work above the git root, and the repository's own
    // CLAUDE.md at its root, with the project nested below both.
    let work = write(dir.path(), "Work/CLAUDE.md", TEAM);
    let repo = folder(dir.path(), "Work/repo");
    gix::init(&repo).unwrap();
    let team = write(&repo, "CLAUDE.md", TEAM);
    let app = folder(&repo, "packages/app");
    let (project, _) =
        MindProject::init_settings(&app, &Agent::ALL, Share::Shared, &Directories::default())
            .unwrap();
    for block_written in [true, false] {
        let rendered = claude_md::render(&project, &NO_HOMES, block_written).unwrap();
        assert!(rendered.files.is_empty());
        assert_eq!(fs::read_to_string(&work).unwrap(), TEAM);
        assert_eq!(fs::read_to_string(&team).unwrap(), TEAM);
    }
    assert_eq!(
        claude_md::render(&project, &NO_HOMES, true).unwrap().reach,
        Reach::Fallback(Fallback::Above(resolved(&team)))
    );
    assert!(!app.join("CLAUDE.md").exists());
    assert!(!app.join("CLAUDE.local.md").exists());
}

#[test]
fn no_claude_md_is_ever_made() {
    let fixture = fixture(Share::Shared);
    for block_written in [true, false] {
        let rendered = fixture.render(block_written);
        assert!(rendered.files.is_empty());
    }
    for name in NAMES {
        assert!(!fixture.file(name).exists(), "{name}");
    }

    let fixture = self::fixture(Share::Shared);
    write(fixture.root(), "CLAUDE.local.md", TEAM);
    let rendered = fixture.render(true);
    assert!(rendered.files.is_empty());
    assert!(!fixture.file("CLAUDE.md").exists());
    assert_eq!(
        fs::read_to_string(fixture.file("CLAUDE.local.md")).unwrap(),
        TEAM
    );
}

#[test]
fn an_import_no_longer_wanted_in_a_tracked_file_is_left_and_said() {
    let mut fixture = fixture(Share::Shared);
    write(fixture.root(), "CLAUDE.md", TEAM);
    fixture.render(true);
    let imported = fs::read(fixture.file("CLAUDE.md")).unwrap();
    // Committed with the import in it, then the project turned local.
    fixture.track();
    fixture
        .project
        .change_settings(|settings| settings.share = Share::Local)
        .unwrap();

    // Claude still reads it there, so it is still how Claude reaches the block.
    let rendered = fixture.render(true);
    assert_eq!(rendered.reach, Reach::Import(fixture.file("CLAUDE.md")));
    assert!(rendered.warnings.is_empty());

    let rendered = fixture.render(false);
    assert_eq!(
        rendered.warnings,
        [Warning::Tracked(fixture.file("CLAUDE.md"))]
    );
    assert_eq!(rendered.files[0].written, Written::Unchanged);
    assert_eq!(fs::read(fixture.file("CLAUDE.md")).unwrap(), imported);
}

#[test]
fn a_malformed_file_refuses_the_render_with_nothing_written() {
    let fixture = fixture(Share::Shared);
    // The file read first holds the import, which a render with no block
    // takes out: one that wrote each file as it went would have taken it
    // out before meeting the malformed one.
    let imported =
        format!("<!-- mindweaver:begin -->\n@AGENTS.md\n<!-- mindweaver:end -->\n\n{TEAM}");
    write(fixture.root(), "CLAUDE.md", &imported);
    write(
        fixture.root(),
        ".claude/CLAUDE.md",
        &format!("<!-- mindweaver:begin -->\n@../AGENTS.md\n\n{TEAM}"),
    );
    for block_written in [false, true] {
        let error = claude_md::render(&fixture.project, &NO_HOMES, block_written).unwrap_err();
        assert!(matches!(error, ClaudeMdError::Malformed { .. }), "{error}");
        assert_eq!(
            fs::read_to_string(fixture.file("CLAUDE.md")).unwrap(),
            imported
        );
    }
}

#[cfg(unix)]
#[test]
fn the_agents_md_itself_is_never_imported_into_and_an_import_beside_it_goes() {
    let fixture = fixture(Share::Shared);
    write(fixture.root(), "AGENTS.md", TEAM);
    std::os::unix::fs::symlink("AGENTS.md", fixture.file("CLAUDE.md")).unwrap();
    let imported =
        format!("<!-- mindweaver:begin -->\n@../AGENTS.md\n<!-- mindweaver:end -->\n\n{TEAM}");
    write(fixture.root(), ".claude/CLAUDE.md", &imported);

    let rendered = fixture.render(true);
    assert_eq!(rendered.reach, Reach::Same(fixture.file("CLAUDE.md")));
    assert_eq!(fs::read_to_string(fixture.file("AGENTS.md")).unwrap(), TEAM);
    assert_eq!(
        fs::read_to_string(fixture.file(".claude/CLAUDE.md")).unwrap(),
        TEAM
    );
    assert_eq!(rendered.files.len(), 1);
    assert_eq!(rendered.files[0].written, Written::Removed);
}

#[cfg(unix)]
#[test]
fn a_link_is_never_written_through() {
    let fixture = fixture(Share::Shared);
    let imported =
        format!("<!-- mindweaver:begin -->\n@AGENTS.md\n<!-- mindweaver:end -->\n\n{TEAM}");
    let docs = write(fixture.root(), "docs/claude.md", &imported);
    std::os::unix::fs::symlink("docs/claude.md", fixture.file("CLAUDE.md")).unwrap();
    for block_written in [true, false] {
        let rendered = fixture.render(block_written);
        assert!(rendered.files.is_empty());
        assert_eq!(fs::read_to_string(&docs).unwrap(), imported);
    }
}
