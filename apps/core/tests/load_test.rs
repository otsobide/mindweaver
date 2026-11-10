//! Loading another workspace: its own artifacts offered to this one's
//! projects, read live from where it is.

use std::fs;
use std::path::{Path, PathBuf};

mod common;

use mindweaver_core::install::{self, InstallError, Installed, Offer, Standing};
use mindweaver_core::ledger::Ledger;
use mindweaver_core::AgentHomes;
use mindweaver_core::{
    resolve_except, Inputs, Intent, Kind, MindProject, Registration, ResolveError, WeaverWorkspace,
    WorkspaceError, WEAVER_CONFIG, WEAVER_DIR,
};
use tempfile::TempDir;

/// Under one directory: `target`, a workspace linking the project
/// `target/collapse`, and `team`, a workspace with skills of its own.
fn target_and_team(
    team_skills: &[&str],
) -> (TempDir, WeaverWorkspace, MindProject, WeaverWorkspace) {
    let dir = TempDir::new().unwrap();
    let (mut target, _) = WeaverWorkspace::init(dir.path().join("target")).unwrap();
    let (project, _) = MindProject::init(dir.path().join("target/collapse")).unwrap();
    target.link(&project).unwrap();
    let (team, _) = WeaverWorkspace::init(dir.path().join("team")).unwrap();
    for name in team_skills {
        mindweaver_core::create(
            &team.own(),
            Kind::Skill,
            name,
            &format!("The team's {name}"),
        )
        .unwrap();
    }
    (dir, target, project, team)
}

fn marker(workspace: &WeaverWorkspace) -> String {
    fs::read_to_string(workspace.root().join(WEAVER_DIR).join(WEAVER_CONFIG)).unwrap()
}

fn installed_at(project: &MindProject, name: &str) -> PathBuf {
    project.directory_for(Kind::Skill).unwrap().join(name)
}

// ---------------------------------------------------------------------------
// The marker
// ---------------------------------------------------------------------------

#[test]
fn load_records_the_workspace_as_a_route_and_keeps_the_comments() {
    let (_dir, mut target, _, team) = target_and_team(&[]);

    let (entry, registration) = target.load(&team).unwrap();

    assert_eq!(entry, Path::new("../team"));
    assert_eq!(registration, Registration::Added);
    let text = marker(&target);
    assert!(text.contains("loaded = [\"../team\"]"), "{text}");
    assert!(text.contains("# Mindweaver workspace."), "comments kept");
    assert!(target.is_loaded(team.root()));
}

#[test]
fn loading_twice_changes_nothing_and_says_so() {
    let (_dir, mut target, _, team) = target_and_team(&[]);
    target.load(&team).unwrap();
    let before = marker(&target);

    let (entry, registration) = target.load(&team).unwrap();

    assert_eq!(registration, Registration::AlreadyRegistered);
    assert_eq!(entry, Path::new("../team"));
    assert_eq!(marker(&target), before);
}

#[test]
fn a_workspace_cannot_load_itself() {
    let (_dir, mut target, _, _) = target_and_team(&[]);
    let same = WeaverWorkspace::open(target.root()).unwrap();

    let error = target.load(&same).unwrap_err();

    assert!(
        matches!(error, WorkspaceError::LoadsItself { .. }),
        "{error}"
    );
}

#[test]
fn loading_several_loads_none_when_one_is_the_workspace_itself() {
    let (_dir, mut target, _, team) = target_and_team(&[]);
    let same = WeaverWorkspace::open(target.root()).unwrap();
    let before = marker(&target);

    let error = target.load_all(&[team, same]).unwrap_err();

    assert!(
        matches!(error, WorkspaceError::LoadsItself { .. }),
        "{error}"
    );
    assert_eq!(marker(&target), before, "half a load happened");
}

#[test]
fn a_marker_from_before_loading_existed_gains_the_key_when_it_is_first_used() {
    let (_dir, target, _, team) = target_and_team(&[]);
    let path = target.root().join(WEAVER_DIR).join(WEAVER_CONFIG);
    let old = marker(&target).replace("loaded = []\n", "");
    fs::write(&path, &old).unwrap();
    let mut target = WeaverWorkspace::open(target.root()).unwrap();
    assert!(
        target.loads().is_empty(),
        "an old marker reads as loading nothing"
    );

    target.load(&team).unwrap();

    assert!(marker(&target).contains("loaded = [\"../team\"]"));
}

#[test]
fn unload_drops_the_entry_and_refuses_what_was_never_loaded() {
    let (dir, mut target, _, team) = target_and_team(&[]);
    target.load(&team).unwrap();

    let removed = target.unload(team.root()).unwrap();
    assert_eq!(removed, vec![PathBuf::from("../team")]);
    assert!(!target.is_loaded(team.root()));

    let error = target.unload(&dir.path().join("elsewhere")).unwrap_err();
    assert!(matches!(error, WorkspaceError::NotLoaded { .. }), "{error}");
}

#[test]
fn a_loaded_workspace_that_has_gone_is_reported_and_can_still_be_unloaded() {
    let (_dir, mut target, _, team) = target_and_team(&["deploy"]);
    target.load(&team).unwrap();
    let root = team.root().to_path_buf();
    fs::remove_dir_all(&root).unwrap();

    let loads = target.loads();
    assert_eq!(loads.len(), 1);
    assert!(loads[0].workspace.is_err());
    let loads = install::offered_by_loads(&target);
    assert!(loads.offers.is_empty());
    assert_eq!(loads.failures.len(), 1);

    assert_eq!(target.unload(&root).unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// Which workspace a load goes into
// ---------------------------------------------------------------------------

/// The workspace a load or an unload run in `start` goes into, passing over
/// `except`, with nothing named or activated and the default in `home`.
fn into(
    start: &Path,
    except: &[PathBuf],
    home: Option<&Path>,
    intent: Intent,
) -> Result<Option<WeaverWorkspace>, ResolveError> {
    let inputs = Inputs {
        flag: None,
        activated: None,
        start,
        home,
        intent,
    };
    Ok(resolve_except(&inputs, except)?.map(|resolved| resolved.workspace))
}

#[test]
fn from_inside_the_workspace_being_loaded_the_walk_carries_on_above_it() {
    let dir = TempDir::new().unwrap();
    WeaverWorkspace::init(dir.path()).unwrap();
    let (team, _) = WeaverWorkspace::init(dir.path().join("team")).unwrap();
    let inside = team.root().join("skills");
    fs::create_dir_all(&inside).unwrap();

    let found = into(&inside, &[team.root().to_path_buf()], None, Intent::Write)
        .unwrap()
        .unwrap();

    assert_eq!(
        found.root().canonicalize().unwrap(),
        dir.path().canonicalize().unwrap()
    );
}

#[test]
fn with_nothing_else_above_the_load_goes_into_the_default_workspace() {
    let home = TempDir::new().unwrap();
    let dir = TempDir::new().unwrap();
    let (team, _) = WeaverWorkspace::init(dir.path().join("team")).unwrap();

    let found = into(
        team.root(),
        &[team.root().to_path_buf()],
        Some(home.path()),
        Intent::Write,
    )
    .unwrap()
    .unwrap();

    assert!(found.is_default(Some(home.path())));
}

#[test]
fn the_default_workspace_is_not_offered_as_somewhere_to_load_itself() {
    let home = TempDir::new().unwrap();
    WeaverWorkspace::init(home.path()).unwrap();

    let error = into(
        home.path(),
        &[home.path().to_path_buf()],
        Some(home.path()),
        Intent::Write,
    )
    .unwrap_err();

    assert!(
        matches!(
            error,
            ResolveError::Workspace(WorkspaceError::LoadsItself { .. })
        ),
        "{error}"
    );
}

// ---------------------------------------------------------------------------
// What a load offers, and installing it
// ---------------------------------------------------------------------------

#[test]
fn a_loaded_workspaces_own_skills_are_offered_after_the_workspaces_own() {
    let (_dir, mut target, project, team) = target_and_team(&["deploy"]);
    mindweaver_core::create(&target.own(), Kind::Skill, "commit-style", "Ours").unwrap();
    target.load(&team).unwrap();
    let ledger = Ledger::in_memory().unwrap();

    let candidates = install::survey(&target, &ledger, &project, Kind::Skill).unwrap();

    let offered: Vec<(&str, String)> = candidates
        .iter()
        .map(|candidate| (candidate.name().as_str(), candidate.origin().into_string()))
        .collect();
    assert_eq!(
        offered,
        [
            ("commit-style", String::from("workspace")),
            ("deploy", String::from("load:../team")),
        ]
    );
}

#[test]
fn installing_a_loaded_skill_copies_it_and_a_later_edit_there_updates_it() {
    let (_dir, mut target, project, team) = target_and_team(&["deploy"]);
    target.load(&team).unwrap();
    let ledger = Ledger::in_memory().unwrap();

    let candidate =
        install::offered(&target, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    let first = install::install(
        &target,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap()
    .installed;
    assert!(matches!(first, Installed::Added { .. }), "{first:?}");
    assert!(installed_at(&project, "deploy").join("SKILL.md").is_file());

    // Read live: an edit in the loaded workspace is what installs next, with
    // no load in between.
    let source = team
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy/SKILL.md");
    let edited = fs::read_to_string(&source).unwrap() + "\nNew steps.\n";
    fs::write(&source, edited).unwrap();
    let candidate =
        install::offered(&target, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    // The copy is what was installed, and the loaded workspace offers
    // something else by now.
    assert_eq!(candidate.standing, Standing::Outdated);
    let second = install::install(
        &target,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap()
    .installed;

    assert!(matches!(second, Installed::Updated { .. }), "{second:?}");
    let copied = fs::read_to_string(installed_at(&project, "deploy").join("SKILL.md")).unwrap();
    assert!(copied.contains("New steps"), "{copied}");
}

#[test]
fn from_names_a_loaded_workspace_by_its_origin_or_its_name() {
    let (_dir, mut target, project, team) = target_and_team(&["deploy"]);
    target.load(&team).unwrap();
    let ledger = Ledger::in_memory().unwrap();

    for from in ["load:../team", "team"] {
        let candidate = install::offered(
            &target,
            &ledger,
            &project,
            Kind::Skill,
            "deploy",
            Some(from),
        )
        .unwrap_or_else(|error| panic!("--from {from}: {error}"));
        assert!(matches!(candidate.offer, Offer::Loaded { .. }));
    }
    let error = install::offered(
        &target,
        &ledger,
        &project,
        Kind::Skill,
        "deploy",
        Some("workspace"),
    )
    .unwrap_err();
    assert!(matches!(error, InstallError::NotOffered { .. }), "{error}");
}

#[test]
fn a_name_a_source_and_a_loaded_workspace_are_both_called_names_neither_and_their_origins_each() {
    let (_dir, mut target, project, team) = target_and_team(&["deploy"]);
    target.load(&team).unwrap();
    let repo = common::repository(&[(
        "skills/deploy/SKILL.md",
        &common::skill("deploy", "Ship it"),
    )]);
    let url = common::url(&repo);
    let ledger = target.ledger().unwrap();
    mindweaver_core::gather::gather(
        &target,
        &ledger,
        &mindweaver_core::gather::Request::git(url.clone()).named(Some("team")),
    )
    .unwrap();
    let from = |typed: &str| {
        install::offered(
            &target,
            &ledger,
            &project,
            Kind::Skill,
            "deploy",
            Some(typed),
        )
    };

    // `--from team` named the loaded workspace before sources had names,
    // so it is not the source's now, unsaid.
    let twice = from("team").unwrap_err();
    assert!(
        matches!(twice, InstallError::OfferedTwice { .. }),
        "{twice}"
    );
    assert_eq!(
        twice.to_string(),
        format!(
            "`deploy` is offered by more than one place (load:../team, {url}), so say which \
             with --from"
        )
    );
    assert!(matches!(
        from("load:../team").unwrap().offer,
        Offer::Loaded { .. }
    ));
    assert!(matches!(from(&url).unwrap().offer, Offer::Shelf(_)));
}

#[test]
fn a_sources_name_that_is_a_path_to_a_loaded_workspace_names_neither() {
    let (dir, mut target, project, team) = target_and_team(&["deploy"]);
    // Called something else, so only the path is `team`.
    let config = team.root().join(WEAVER_DIR).join(WEAVER_CONFIG);
    let marker = fs::read_to_string(&config).unwrap();
    fs::write(
        &config,
        marker.replace("name = \"team\"", "name = \"crew\""),
    )
    .unwrap();
    target
        .load(&WeaverWorkspace::open(team.root()).unwrap())
        .unwrap();
    let repo = common::repository(&[(
        "skills/deploy/SKILL.md",
        &common::skill("deploy", "Ship it"),
    )]);
    let url = common::url(&repo);
    let ledger = target.ledger().unwrap();
    mindweaver_core::gather::gather(
        &target,
        &ledger,
        &mindweaver_core::gather::Request::git(url.clone()).named(Some("team")),
    )
    .unwrap();
    let typed = |from: &str, base: &Path| {
        install::offered_typed(
            &target,
            &ledger,
            &project,
            &mindweaver_core::Reference::parse("deploy"),
            Some(from),
            base,
        )
    };

    // From beside `team`, where it is the path given to load.
    let twice = typed("team", dir.path()).unwrap_err();
    assert!(
        matches!(twice, InstallError::OfferedTwice { .. }),
        "{twice}"
    );
    // From anywhere else it is only the source's name.
    let named = typed("team", project.root()).unwrap();
    assert!(matches!(named.offer, Offer::Shelf(_)));
}

#[test]
fn a_loaded_workspace_under_the_target_reads_as_a_path_not_as_the_workspaces_own() {
    let dir = TempDir::new().unwrap();
    let (mut target, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let (project, _) = MindProject::init(dir.path().join("collapse")).unwrap();
    target.link(&project).unwrap();
    // A workspace in a folder called `workspace`: its entry is the word the
    // workspace's own go by.
    let (inner, _) = WeaverWorkspace::init(dir.path().join("workspace")).unwrap();
    mindweaver_core::create(&inner.own(), Kind::Skill, "deploy", "Theirs").unwrap();
    target.load(&inner).unwrap();
    let ledger = Ledger::in_memory().unwrap();

    let candidates = install::survey(&target, &ledger, &project, Kind::Skill).unwrap();

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].origin(), "load:workspace");
    assert!(!candidates[0].comes_from("workspace"));
}

#[test]
fn a_loaded_workspace_is_not_followed_into_what_it_loads() {
    let (dir, mut target, project, mut team) = target_and_team(&[]);
    let (further, _) = WeaverWorkspace::init(dir.path().join("further")).unwrap();
    mindweaver_core::create(&further.own(), Kind::Skill, "deploy", "Far").unwrap();
    team.load(&further).unwrap();
    target.load(&team).unwrap();
    // And the other way round too, which would loop if loads were followed.
    let mut further = WeaverWorkspace::open(further.root()).unwrap();
    further.load(&target).unwrap();
    let ledger = Ledger::in_memory().unwrap();

    let candidates = install::survey(&target, &ledger, &project, Kind::Skill).unwrap();

    assert!(candidates.is_empty(), "{candidates:?}");
}

#[test]
fn a_loaded_skill_whose_folder_went_away_says_to_unload_it() {
    let (_dir, mut target, project, team) = target_and_team(&["deploy"]);
    target.load(&team).unwrap();
    let ledger = Ledger::in_memory().unwrap();
    let candidate =
        install::offered(&target, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    fs::remove_dir_all(
        team.own()
            .directory_for(Kind::Skill)
            .unwrap()
            .join("deploy"),
    )
    .unwrap();

    let error = install::install(
        &target,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert!(matches!(error, InstallError::Gone { .. }), "{error}");
    // By its root, which `mindweaver unload` reads the same from anywhere.
    let hint = format!("mindweaver unload {}", team.root().display());
    assert!(error.to_string().contains(&hint), "{error}");
}

#[test]
fn a_loaded_workspace_that_is_the_project_itself_is_never_copied_onto_itself() {
    // A skills repository that is both a workspace and a project it links,
    // keeping both in one folder: loading it into a workspace that also links
    // that project offers each skill into the very folder it is in.
    let dir = TempDir::new().unwrap();
    let (mut target, _) = WeaverWorkspace::init(dir.path().join("target")).unwrap();
    let repo = dir.path().join("repo");
    let (mut shared, _) = WeaverWorkspace::init_with(&repo, &common::beside()).unwrap();
    let (project, _) = MindProject::init(&repo).unwrap();
    shared.link(&project).unwrap();
    mindweaver_core::create(&project, Kind::Skill, "deploy", "Ship it").unwrap();
    target.link(&project).unwrap();
    target.load(&shared).unwrap();
    let ledger = Ledger::in_memory().unwrap();
    let mut candidate = install::offered(
        &target,
        &ledger,
        &project,
        Kind::Skill,
        "deploy",
        Some("load:../repo"),
    )
    .unwrap();
    // As if a record said it were ours, the case that could otherwise delete
    // the loaded workspace's own files on uninstall.
    candidate.standing = Standing::Current;

    let outcome = install::install(
        &target,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap()
    .installed;

    assert!(
        matches!(outcome, Installed::Unchanged { .. }),
        "{outcome:?}"
    );
    assert!(installed_at(&project, "deploy").join("SKILL.md").is_file());
    let entry = "../repo";
    assert!(
        ledger.installations(entry, Kind::Skill).unwrap().is_empty(),
        "nothing recorded"
    );
}

// ---------------------------------------------------------------------------
// What a loaded workspace must never be able to do
// ---------------------------------------------------------------------------

/// Write a skill by hand into a workspace's own skills, declaring `name`.
fn write_declaring(workspace: &WeaverWorkspace, folder: &str, name: &str) {
    let dir = workspace
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join(folder);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Crafted\n---\n\nSteps.\n"),
    )
    .unwrap();
}

#[test]
fn a_skill_declaring_a_name_that_is_not_one_folder_is_not_offered_and_is_said() {
    let (_dir, mut target, project, team) = target_and_team(&["deploy"]);
    for (folder, name) in [
        ("up", ".."),
        ("here", "."),
        ("deep", "a/b"),
        ("root", "/tmp/victim"),
    ] {
        write_declaring(&team, folder, name);
    }
    target.load(&team).unwrap();
    let ledger = Ledger::in_memory().unwrap();

    let candidates = install::survey(&target, &ledger, &project, Kind::Skill).unwrap();
    let names: Vec<&str> = candidates
        .iter()
        .map(|candidate| candidate.name().as_str())
        .collect();
    assert_eq!(names, ["deploy"]);

    let failures: Vec<String> = install::offered_by_loads(&target)
        .failures
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(failures.len(), 4, "{failures:?}");
    assert!(failures
        .iter()
        .all(|failure| failure.contains("not one folder name")));
}

#[test]
fn install_and_uninstall_refuse_a_name_that_is_not_one_folder_whatever_offers_it() {
    let (_dir, mut target, project, team) = target_and_team(&["deploy"]);
    target.load(&team).unwrap();
    let ledger = Ledger::in_memory().unwrap();
    let mut candidate =
        install::offered(&target, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    // As though it had been offered anyway.
    if let Offer::Loaded { name, .. } = &mut candidate.offer {
        *name = "..".into();
    }

    let installing = install::install(
        &target,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap_err();
    let removing = install::uninstall(
        &target,
        &ledger,
        &project,
        Kind::Skill,
        "..",
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert!(
        matches!(installing, InstallError::UnsafeName { .. }),
        "{installing}"
    );
    assert!(
        matches!(removing, InstallError::UnsafeName { .. }),
        "{removing}"
    );
    assert!(
        project.root().join(".mind").is_dir(),
        "the project is still there"
    );
}

#[test]
fn a_loaded_workspace_inside_the_folder_it_would_install_into_is_not_deleted() {
    let dir = TempDir::new().unwrap();
    let (mut target, _) = WeaverWorkspace::init(dir.path().join("target")).unwrap();
    let (project, _) = MindProject::init(dir.path().join("target/app")).unwrap();
    target.link(&project).unwrap();
    // A workspace sitting where the project would put `deploy`.
    let nested = project.directory_for(Kind::Skill).unwrap().join("deploy");
    let (inner, _) = WeaverWorkspace::init(&nested).unwrap();
    mindweaver_core::create(&inner.own(), Kind::Skill, "deploy", "Nested").unwrap();
    target.load(&inner).unwrap();
    let ledger = Ledger::in_memory().unwrap();
    let mut candidate =
        install::offered(&target, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    candidate.standing = Standing::Absent;

    let error = install::install(
        &target,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert!(matches!(error, InstallError::Overlaps { .. }), "{error}");
    assert!(
        nested.join(".mindweaver").join("weaver.toml").is_file(),
        "left whole"
    );
    assert!(inner
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy/SKILL.md")
        .is_file());
}

#[test]
fn a_folder_started_by_hand_without_a_manifest_is_somebody_elses() {
    let (_dir, mut target, project, team) = target_and_team(&["deploy"]);
    target.load(&team).unwrap();
    let notes = installed_at(&project, "deploy").join("NOTES.md");
    fs::create_dir_all(notes.parent().unwrap()).unwrap();
    fs::write(&notes, "my draft").unwrap();
    let ledger = Ledger::in_memory().unwrap();

    let candidate =
        install::offered(&target, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    assert_eq!(candidate.standing, Standing::Foreign);
    let outcome = install::install(
        &target,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap()
    .installed;

    assert!(matches!(outcome, Installed::Foreign { .. }), "{outcome:?}");
    assert_eq!(fs::read_to_string(&notes).unwrap(), "my draft");
}

#[cfg(unix)]
#[test]
fn a_link_out_of_the_loaded_workspace_is_refused_not_copied() {
    let (dir, mut target, project, team) = target_and_team(&["deploy"]);
    let secret = dir.path().join("private.env");
    fs::write(&secret, "SECRET=hunter2").unwrap();
    let skill = team
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy");
    std::os::unix::fs::symlink(&secret, skill.join("config.env")).unwrap();
    // A link that stays inside the workspace is fine, and is copied as a file.
    fs::write(team.root().join("shared.md"), "shared").unwrap();
    std::os::unix::fs::symlink(team.root().join("shared.md"), skill.join("shared.md")).unwrap();
    target.load(&team).unwrap();
    let ledger = Ledger::in_memory().unwrap();
    let candidate =
        install::offered(&target, &ledger, &project, Kind::Skill, "deploy", None).unwrap();

    let error = install::install(
        &target,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert!(matches!(error, InstallError::Write { .. }), "{error}");
    assert!(error.to_string().contains("outside"), "{error}");
    assert!(!installed_at(&project, "deploy").join("config.env").exists());

    fs::remove_file(skill.join("config.env")).unwrap();
    install::install(
        &target,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();
    let copied = installed_at(&project, "deploy").join("shared.md");
    assert!(copied.is_file() && !copied.is_symlink());
}

#[test]
fn two_spellings_of_one_loaded_workspace_offer_its_skills_once() {
    let (_dir, target, project, _team) = target_and_team(&["deploy"]);
    let path = target.root().join(WEAVER_DIR).join(WEAVER_CONFIG);
    let text = marker(&target).replace("loaded = []", "loaded = [\"../team\", \"./../team\"]");
    fs::write(&path, text).unwrap();
    let target = WeaverWorkspace::open(target.root()).unwrap();
    let ledger = Ledger::in_memory().unwrap();

    let candidates = install::survey(&target, &ledger, &project, Kind::Skill).unwrap();

    assert_eq!(candidates.len(), 1, "{candidates:?}");
    install::offered(
        &target,
        &ledger,
        &project,
        Kind::Skill,
        "deploy",
        Some("team"),
    )
    .unwrap();
}

#[cfg(unix)]
#[test]
fn unloading_two_spellings_of_one_workspace_is_one_unload() {
    let (dir, mut target, _, team) = target_and_team(&[]);
    target.load(&team).unwrap();
    let link = dir.path().join("team-link");
    std::os::unix::fs::symlink(team.root(), &link).unwrap();

    let removed = target
        .unload_all(&[team.root().to_path_buf(), link])
        .unwrap();

    assert_eq!(removed, vec![PathBuf::from("../team")]);
    assert!(target.loads().is_empty());
}

#[test]
fn unload_all_changes_nothing_when_one_was_never_loaded() {
    let (dir, mut target, _, team) = target_and_team(&[]);
    target.load(&team).unwrap();
    let before = marker(&target);

    let error = target
        .unload_all(&[team.root().to_path_buf(), dir.path().join("nowhere")])
        .unwrap_err();

    assert!(matches!(error, WorkspaceError::NotLoaded { .. }), "{error}");
    assert_eq!(marker(&target), before);
}

#[test]
fn something_gathered_and_loaded_from_one_directory_has_two_origins() {
    let repo = common::repository(&[(
        "skills/deploy/SKILL.md",
        &common::skill("deploy", "Ship it"),
    )]);
    WeaverWorkspace::init_with(repo.path(), &common::beside()).unwrap();
    let (_dir, mut target, project, _) = target_and_team(&[]);
    let ledger = target.ledger().unwrap();
    let url = common::url(&repo);
    mindweaver_core::gather::gather(
        &target,
        &ledger,
        &mindweaver_core::gather::Request::git(url.clone()),
    )
    .unwrap();
    target
        .load(&WeaverWorkspace::open(repo.path()).unwrap())
        .unwrap();

    let candidates = install::survey(&target, &ledger, &project, Kind::Skill).unwrap();
    let origins: Vec<String> = candidates
        .iter()
        .map(|candidate| candidate.origin().into_string())
        .collect();
    assert_eq!(origins.len(), 2);
    assert_ne!(origins[0], origins[1], "{origins:?}");

    for origin in &origins {
        install::offered(
            &target,
            &ledger,
            &project,
            Kind::Skill,
            "deploy",
            Some(origin),
        )
        .unwrap_or_else(|error| panic!("--from {origin}: {error}"));
    }
}

#[test]
fn from_takes_the_path_given_to_load_from_wherever_it_is_typed() {
    let (dir, mut target, project, team) = target_and_team(&["deploy"]);
    target.load(&team).unwrap();
    let ledger = Ledger::in_memory().unwrap();

    for (base, typed) in [
        (project.root().to_path_buf(), String::from("../../team")),
        (dir.path().to_path_buf(), String::from("team/")),
        (dir.path().to_path_buf(), team.root().display().to_string()),
    ] {
        let from = install::resolve_from(&target, &base, &typed);
        assert_eq!(from, "load:../team", "{typed} from {}", base.display());
        install::offered(
            &target,
            &ledger,
            &project,
            Kind::Skill,
            "deploy",
            Some(&from),
        )
        .unwrap();
    }
}

#[test]
fn looking_for_somewhere_to_unload_from_never_makes_the_default() {
    let home = TempDir::new().unwrap();
    let dir = TempDir::new().unwrap();

    // A read, as unloading resolves it: what it finds at home is a stand-in
    // with nothing loaded, never the workspace made to say so.
    let found = into(dir.path(), &[], Some(home.path()), Intent::Read)
        .unwrap()
        .unwrap();

    assert!(!found.is_made());
    assert!(!home.path().join(WEAVER_DIR).exists());
}

#[test]
fn what_a_loaded_workspace_keeps_in_a_projects_folder_is_never_overwritten_or_removed() {
    // `lib` is a project the target links and installs into, and then becomes
    // a workspace of its own keeping its skills in that same folder, which the
    // target loads: a record from before the load says the folder is ours.
    let dir = TempDir::new().unwrap();
    let (mut target, _) = WeaverWorkspace::init(dir.path().join("target")).unwrap();
    mindweaver_core::create(&target.own(), Kind::Skill, "x", "Ours").unwrap();
    let (project, _) = MindProject::init(dir.path().join("lib")).unwrap();
    target.link(&project).unwrap();
    let ledger = Ledger::in_memory().unwrap();
    let ours = install::offered(&target, &ledger, &project, Kind::Skill, "x", None).unwrap();
    install::install(&target, &ledger, &project, &ours, &AgentHomes::default()).unwrap();
    let (lib, _) = WeaverWorkspace::init_with(project.root(), &common::beside()).unwrap();
    let own = installed_at(&project, "x").join("SKILL.md");
    let edited = fs::read_to_string(&own).unwrap() + "\nlib's own edit\n";
    fs::write(&own, &edited).unwrap();
    target.load(&lib).unwrap();

    let again = install::offered(
        &target,
        &ledger,
        &project,
        Kind::Skill,
        "x",
        Some("workspace"),
    )
    .unwrap();
    let installing =
        install::install(&target, &ledger, &project, &again, &AgentHomes::default()).unwrap_err();
    let removing = install::uninstall(
        &target,
        &ledger,
        &project,
        Kind::Skill,
        "x",
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert!(
        matches!(installing, InstallError::KeptByALoad { .. }),
        "{installing}"
    );
    assert!(
        matches!(removing, InstallError::KeptByALoad { .. }),
        "{removing}"
    );
    assert_eq!(fs::read_to_string(&own).unwrap(), edited);
}

// ---------------------------------------------------------------------------
// What the second review found in the first round of fixes
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn a_refused_update_leaves_the_installed_copy_as_it_was() {
    let (dir, mut target, project, team) = target_and_team(&["deploy"]);
    target.load(&team).unwrap();
    let ledger = Ledger::in_memory().unwrap();
    let candidate =
        install::offered(&target, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    install::install(
        &target,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();
    let installed = installed_at(&project, "deploy");
    let before = fs::read_to_string(installed.join("SKILL.md")).unwrap();
    // The teammate adds a link out of their workspace, and edits the skill.
    let skill = team
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy");
    fs::write(dir.path().join("private.env"), "SECRET").unwrap();
    std::os::unix::fs::symlink(dir.path().join("private.env"), skill.join("aaa.env")).unwrap();
    fs::write(skill.join("SKILL.md"), before.clone() + "\nMore.\n").unwrap();
    let candidate =
        install::offered(&target, &ledger, &project, Kind::Skill, "deploy", None).unwrap();

    install::install(
        &target,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert_eq!(
        fs::read_to_string(installed.join("SKILL.md")).unwrap(),
        before
    );
    assert!(!installed.join("aaa.env").exists());
    let leftovers: Vec<_> = fs::read_dir(project.directory_for(Kind::Skill).unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(leftovers, ["deploy"], "nothing staged is left behind");
}

#[cfg(unix)]
#[test]
fn a_refused_first_install_leaves_nothing_so_it_can_be_tried_again() {
    let (dir, mut target, project, team) = target_and_team(&["deploy"]);
    let skill = team
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy");
    fs::write(dir.path().join("private.env"), "SECRET").unwrap();
    let link = skill.join("config.env");
    std::os::unix::fs::symlink(dir.path().join("private.env"), &link).unwrap();
    target.load(&team).unwrap();
    let ledger = Ledger::in_memory().unwrap();
    let candidate =
        install::offered(&target, &ledger, &project, Kind::Skill, "deploy", None).unwrap();

    install::install(
        &target,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap_err();
    assert!(!installed_at(&project, "deploy").exists());

    fs::remove_file(&link).unwrap();
    let candidate =
        install::offered(&target, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    assert_eq!(candidate.standing, Standing::Absent);
    install::install(
        &target,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();
}

#[cfg(unix)]
#[test]
fn a_link_inside_the_skill_itself_is_followed_wherever_the_skills_folder_lives() {
    // The workspace's own skills folder is a link to somewhere outside it; a
    // link between two files of one skill still stays inside that skill.
    let dir = TempDir::new().unwrap();
    let shared = dir.path().join("shared/skills/deploy");
    fs::create_dir_all(&shared).unwrap();
    fs::write(
        shared.join("SKILL.md"),
        "---\nname: deploy\ndescription: Own\n---\n",
    )
    .unwrap();
    fs::write(shared.join("v2.md"), "v2").unwrap();
    std::os::unix::fs::symlink("v2.md", shared.join("latest.md")).unwrap();
    let (mut target, _) = WeaverWorkspace::init(dir.path().join("target")).unwrap();
    let own = target.own().directory_for(Kind::Skill).unwrap();
    fs::create_dir_all(own.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(dir.path().join("shared/skills"), own).unwrap();
    let (project, _) = MindProject::init(dir.path().join("target/app")).unwrap();
    target.link(&project).unwrap();
    let ledger = Ledger::in_memory().unwrap();
    let candidate =
        install::offered(&target, &ledger, &project, Kind::Skill, "deploy", None).unwrap();

    install::install(
        &target,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();

    let copied = installed_at(&project, "deploy").join("latest.md");
    assert_eq!(fs::read_to_string(copied).unwrap(), "v2");
}

#[cfg(unix)]
#[test]
fn directories_linking_to_each_other_are_refused_not_copied_forever() {
    let (_dir, mut target, project, team) = target_and_team(&["deploy"]);
    let skill = team
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy");
    fs::create_dir_all(skill.join("a")).unwrap();
    fs::create_dir_all(skill.join("b")).unwrap();
    std::os::unix::fs::symlink("../b", skill.join("a/tob")).unwrap();
    std::os::unix::fs::symlink("../a", skill.join("b/toa")).unwrap();
    target.load(&team).unwrap();
    let ledger = Ledger::in_memory().unwrap();
    let candidate =
        install::offered(&target, &ledger, &project, Kind::Skill, "deploy", None).unwrap();

    let error = install::install(
        &target,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert!(error.to_string().contains("links back into"), "{error}");
    assert!(!installed_at(&project, "deploy").exists());
}

#[test]
fn nothing_is_installed_into_the_folder_a_loaded_workspace_keeps_its_own_in() {
    // A project that is also a loaded workspace, sharing its skills folder:
    // an install there would become the loaded workspace's own at once, and
    // could never be updated or removed again, so none is made.
    let dir = TempDir::new().unwrap();
    let (mut target, _) = WeaverWorkspace::init(dir.path().join("target")).unwrap();
    mindweaver_core::create(&target.own(), Kind::Skill, "lint", "Ours").unwrap();
    let (project, _) = MindProject::init(dir.path().join("target/team")).unwrap();
    let (team, _) = WeaverWorkspace::init_with(project.root(), &common::beside()).unwrap();
    target.link(&project).unwrap();
    target.load(&team).unwrap();
    let ledger = Ledger::in_memory().unwrap();
    let candidate =
        install::offered(&target, &ledger, &project, Kind::Skill, "lint", None).unwrap();

    let error = install::install(
        &target,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert!(matches!(error, InstallError::KeptByALoad { .. }), "{error}");
    assert!(error.to_string().contains("mindweaver unload"), "{error}");
    assert!(!installed_at(&project, "lint").exists());
}

#[test]
fn the_word_workspace_and_a_loaded_origin_are_never_taken_for_paths() {
    let (dir, mut target, _, _) = target_and_team(&[]);
    let (inner, _) = WeaverWorkspace::init(target.root().join("workspace")).unwrap();
    target.load(&inner).unwrap();

    assert_eq!(
        install::resolve_from(&target, target.root(), "workspace"),
        "workspace"
    );
    assert_eq!(
        install::resolve_from(&target, dir.path(), "load:elsewhere"),
        "load:elsewhere"
    );
    assert_eq!(
        install::resolve_from(&target, target.root(), "./workspace"),
        "load:workspace"
    );
}
