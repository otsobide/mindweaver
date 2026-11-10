//! Unit tests for `workspace.rs`, kept beside it rather than inside it.

use super::*;

use crate::agent::Agent;
use crate::settings::Share;

#[test]
fn templates_round_trip_through_the_parser() {
    let mind: MindConfig =
        toml::from_str(&mind_template("collapse", &Directories::default())).unwrap();
    // Not the newest a project's marker holds, which is a settings file's.
    assert_eq!(mind.version, MIND_TOML_VERSION);
    assert_eq!(mind.name, "collapse");
    // Spelled out rather than left to the default, so the file answers
    // where a project's skills go without anyone reading the source.
    assert_eq!(mind.directories.get(Kind::Skill), Some(Path::new("skills")));
    assert_eq!(mind.directories.get(Kind::Rule), Some(Path::new("rules")));

    let weaver: WeaverConfig = toml::from_str(&weaver_template(
        "projects",
        &Directories::default(),
        Names::Mindweaver,
    ))
    .unwrap();
    assert_eq!(weaver.version, WEAVER_FORMAT_VERSION);
    assert_eq!(weaver.name, "projects");
    assert!(weaver.projects.is_empty());
    // A workspace's own default is its library, not a project's.
    assert_eq!(
        weaver.directories.get(Kind::Skill),
        Some(Path::new(".mindweaver/library/skills"))
    );
    assert_eq!(
        weaver.directories.get(Kind::Rule),
        Some(Path::new(".mindweaver/library/rules"))
    );
}

#[test]
fn a_name_needing_quotes_still_round_trips() {
    let awkward = "quote\" and \\ backslash";
    let mind: MindConfig =
        toml::from_str(&mind_template(awkward, &Directories::default())).unwrap();
    assert_eq!(mind.name, awkward);
}

#[test]
fn a_name_holding_control_characters_still_round_trips() {
    // A basic string takes no control character as it is but a tab: each of
    // these, written unescaped, makes a marker the parser refuses.
    let awkward = "nul\u{0} esc\u{1b} cr\r del\u{7f} c1\u{85} nl\n tab\t";
    let mind: MindConfig =
        toml::from_str(&mind_template(awkward, &Directories::default())).unwrap();
    assert_eq!(mind.name, awkward);
    let weaver: WeaverConfig = toml::from_str(&weaver_template(
        awkward,
        &Directories::default(),
        Names::Mindweaver,
    ))
    .unwrap();
    assert_eq!(weaver.name, awkward);
}

/// What `mindweaver init PATH` reaches with a typed argument: a directory whose
/// name holds a control character is made a workspace that opens again.
/// Unix only, as Windows takes no control character in a file name.
#[cfg(unix)]
#[test]
fn a_workspace_whose_folder_name_holds_a_control_character_opens_again() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("cr\r esc\u{1b}");
    WeaverWorkspace::init(&root).unwrap();
    assert_eq!(
        WeaverWorkspace::open(&root).unwrap().name(),
        "cr\r esc\u{1b}"
    );
}

/// Rewrite only the `version` line of the marker at `path`, TOML or JSON,
/// rather than writing a two-line stand-in, so what is read back is the
/// marker that was written with nothing changed but its version.
fn with_version(path: &Path, version: u32) {
    let text = fs::read_to_string(path).unwrap();
    let (prefix, line) = if path
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        ("  \"version\": ", format!("  \"version\": {version},"))
    } else {
        ("version = ", format!("version = {version}"))
    };
    assert!(
        text.lines().any(|line| line.starts_with(prefix)),
        "{} has no version line",
        path.display()
    );
    let rewritten: String = text
        .lines()
        .map(|written| {
            if written.starts_with(prefix) {
                format!("{line}\n")
            } else {
                format!("{written}\n")
            }
        })
        .collect();
    fs::write(path, rewritten).unwrap();
}

/// A project marked by settings, written at `root` as a new project's are:
/// a `settings.toml`, at the newest version a project's marker has.
fn settings_project(root: &Path, directories: Directories) -> PathBuf {
    let mind = root.join(MIND_DIR);
    fs::create_dir_all(&mind).unwrap();
    let path = mind.join(SETTINGS_FILE);
    let settings = Settings::new("both", Agent::ALL.to_vec(), Share::Local, directories);
    fs::write(&path, settings.text().unwrap()).unwrap();
    path
}

/// A directory that is both a mind project, marked by settings at the newest
/// version a project's marker has, and a weaver workspace, so each marker
/// can be made newer while the other is read beside it.
fn both_markers() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    let mind = settings_project(dir.path(), Directories::default());
    WeaverWorkspace::init(dir.path()).unwrap();
    let weaver = dir.path().join(WEAVER_DIR).join(WEAVER_CONFIG);
    (dir, mind, weaver)
}

#[test]
fn a_refusal_names_the_marker_that_is_newer() {
    let (dir, mind, weaver) = both_markers();
    with_version(&mind, MIND_FORMAT_VERSION + 1);
    with_version(&weaver, WEAVER_FORMAT_VERSION + 1);

    let project = MindProject::open(dir.path()).unwrap_err();
    assert!(
        matches!(
            project,
            WorkspaceError::Version {
                marker: Marker::Project,
                ..
            }
        ),
        "{project}"
    );
    assert!(project.to_string().contains("project marker"), "{project}");

    let workspace = WeaverWorkspace::open(dir.path()).unwrap_err();
    assert!(
        matches!(
            workspace,
            WorkspaceError::Version {
                marker: Marker::Workspace,
                ..
            }
        ),
        "{workspace}"
    );
    assert!(
        workspace.to_string().contains("workspace marker"),
        "{workspace}"
    );
}

// Written against each marker's own constant, so that a check holding one
// marker to the other's version fails: the second does, both ways round, now
// that a project's marker reads up to 5 and a workspace's up to 2.

#[test]
fn a_newer_project_marker_is_refused_while_a_current_workspace_marker_is_read() {
    let (dir, mind, weaver) = both_markers();
    with_version(&mind, MIND_FORMAT_VERSION + 1);
    with_version(&weaver, WEAVER_FORMAT_VERSION);

    let error = MindProject::open(dir.path()).unwrap_err();
    assert!(
        matches!(
            error,
            WorkspaceError::Version { marker: Marker::Project, found, .. }
                if found == MIND_FORMAT_VERSION + 1
        ),
        "{error}"
    );
    let workspace = WeaverWorkspace::open(dir.path()).unwrap();
    assert_eq!(workspace.config().version, WEAVER_FORMAT_VERSION);
}

#[test]
fn a_newer_workspace_marker_is_refused_while_a_current_project_marker_is_read() {
    let (dir, mind, weaver) = both_markers();
    with_version(&mind, MIND_FORMAT_VERSION);
    with_version(&weaver, WEAVER_FORMAT_VERSION + 1);

    let error = WeaverWorkspace::open(dir.path()).unwrap_err();
    assert!(
        matches!(
            error,
            WorkspaceError::Version { marker: Marker::Workspace, found, .. }
                if found == WEAVER_FORMAT_VERSION + 1
        ),
        "{error}"
    );
    let project = MindProject::open(dir.path()).unwrap();
    assert_eq!(project.config().version(), MIND_FORMAT_VERSION);
}

#[test]
fn a_mind_toml_claiming_the_settings_version_is_refused_rather_than_guessed_at() {
    let dir = tempfile::TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();
    let mind = dir.path().join(MIND_DIR).join(MIND_CONFIG);
    with_version(&mind, SETTINGS_VERSION);

    let error = MindProject::open(dir.path()).unwrap_err();

    assert!(
        matches!(&error, WorkspaceError::Malformed { path, .. } if *path == mind),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("version 3 is written as settings.json"),
        "{error}"
    );

    with_version(&mind, SETTINGS_TOML_VERSION);
    let error = MindProject::open(dir.path()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("version 5 is written as settings.toml"),
        "{error}"
    );
}

#[test]
fn each_version_is_written_in_one_file() {
    for (version, file) in [
        (1, MarkerFile::MindToml),
        (MIND_TOML_VERSION, MarkerFile::MindToml),
        (SETTINGS_VERSION, MarkerFile::SettingsJson),
        (TARGETS_VERSION, MarkerFile::SettingsJson),
        (SETTINGS_TOML_VERSION, MarkerFile::SettingsToml),
    ] {
        assert_eq!(MarkerFile::of_version(version), file, "{version}");
    }
    assert_eq!(
        MarkerFile::of_version(MIND_FORMAT_VERSION),
        MarkerFile::SettingsToml
    );
}

/// The names in `dir`, sorted.
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_replacement_builds_in_a_temporary_of_its_own() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("weaver.toml");
    fs::write(&file, "old").unwrap();
    // The one name every writer used to build in. Somebody's now, or another
    // writer's: either way not this one's to write into or take away.
    let shared = dir.path().join("weaver.toml.tmp");
    fs::write(&shared, "somebody's").unwrap();

    replace_file(&file, "new").unwrap();

    assert_eq!(fs::read_to_string(&file).unwrap(), "new");
    assert_eq!(fs::read_to_string(&shared).unwrap(), "somebody's");
    assert_eq!(names_in(dir.path()), ["weaver.toml", "weaver.toml.tmp"]);
}

#[cfg(unix)]
#[test]
fn an_unresolved_replacement_puts_a_file_where_a_link_was_and_leaves_its_target() {
    let dir = tempfile::TempDir::new().unwrap();
    let target = dir.path().join("elsewhere.json");
    fs::write(&target, "theirs").unwrap();
    let link = dir.path().join("settings.json");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    replace_bytes_unresolved(&link, b"new").unwrap();

    assert!(fs::symlink_metadata(&link).unwrap().is_file());
    assert_eq!(fs::read_to_string(&link).unwrap(), "new");
    assert_eq!(fs::read_to_string(&target).unwrap(), "theirs");
}

#[test]
fn a_replacement_that_cannot_be_put_in_place_leaves_no_temporary_behind() {
    let dir = tempfile::TempDir::new().unwrap();
    // A folder where the file should be: the temporary is written whole, and
    // then no rename can put a file in its place.
    let file = dir.path().join("weaver.toml");
    fs::create_dir(&file).unwrap();
    fs::write(file.join("inside"), "kept").unwrap();

    let (path, _) = replace_bytes(&file, b"new").unwrap_err();

    assert_eq!(path, fs::canonicalize(&file).unwrap());
    assert_eq!(names_in(dir.path()), ["weaver.toml"]);
    assert_eq!(fs::read_to_string(file.join("inside")).unwrap(), "kept");
}

/// What a version 1 build wrote, and what `init` writes as version 2.
const V1: &str = include_str!("../tests/fixtures/mind-v1.toml");
const V2: &str = include_str!("../tests/fixtures/mind-v2.toml");

/// A project at a new directory, marked by `marker` as its `mind.toml`.
fn toml_project(marker: &str) -> (tempfile::TempDir, MindProject) {
    let dir = tempfile::TempDir::new().unwrap();
    fs::create_dir(dir.path().join(MIND_DIR)).unwrap();
    fs::write(dir.path().join(MIND_DIR).join(MIND_CONFIG), marker).unwrap();
    let project = MindProject::open(dir.path()).unwrap();
    (dir, project)
}

#[test]
fn init_still_writes_the_version_2_marker_byte_for_byte() {
    assert_eq!(mind_template("collapse", &Directories::default()), V2);
}

#[test]
fn a_version_1_marker_still_keeps_every_kind_inside_mind() {
    let (dir, project) = toml_project(V1);

    assert_eq!(project.config().version(), 1);
    assert_eq!(project.settings(), None);
    for kind in [Kind::Skill, Kind::Rule] {
        assert_eq!(
            project.directory_for(kind),
            Some(dir.path().join(MIND_DIR).join(kind.folder())),
            "{kind}"
        );
    }
    // Every kind it knew: one added since was kept nowhere.
    assert_eq!(project.directory_for(Kind::Mcp), None);
}

#[test]
fn a_version_2_marker_still_keeps_each_kind_where_it_says_or_beside_the_code() {
    let (dir, project) = toml_project(V2);
    assert_eq!(project.config().version(), 2);
    assert_eq!(
        project.directory_for(Kind::Skill),
        Some(dir.path().join("skills"))
    );
    assert_eq!(
        project.directory_for(Kind::Rule),
        Some(dir.path().join("rules"))
    );

    // One that names a folder of its own, and leaves the other kind out.
    let named = V2.replace(
        "skills = \"skills\"\nrules = \"rules\"\n",
        "skills = \".claude/skills\"\n",
    );
    let (dir, project) = toml_project(&named);
    assert_eq!(
        project.directory_for(Kind::Skill),
        Some(dir.path().join(".claude/skills"))
    );
    assert_eq!(
        project.directory_for(Kind::Rule),
        Some(dir.path().join("rules"))
    );
}

#[test]
fn with_defaults_names_every_kind_and_keeps_what_was_named() {
    let named = Directories::default()
        .with(Kind::Skill, ".claude/skills")
        .with_key("servers", "mcp");

    let filled = named.with_defaults();

    assert_eq!(
        filled.entries(),
        [
            ("skills", Path::new(".claude/skills")),
            ("rules", Path::new("rules")),
            ("servers", Path::new("mcp")),
        ]
    );
    assert_eq!(
        Directories::default().with_defaults().entries(),
        [
            ("skills", Path::new("skills")),
            ("rules", Path::new("rules"))
        ]
    );
}

#[test]
fn a_project_marked_by_settings_without_directories_has_no_folder_for_any_kind() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = settings_project(dir.path(), Directories::default());
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replace("\n[directories]\n", "")).unwrap();

    let project = MindProject::open(dir.path()).unwrap();

    assert!(!fs::read_to_string(&path).unwrap().contains("[directories]"));
    for kind in Kind::ALL {
        assert_eq!(project.directory_for(kind), None, "{kind}");
        let error = project.required_directory(kind).unwrap_err();
        assert!(
            matches!(&error, WorkspaceError::NoDirectory { kind: refused, .. } if *refused == kind),
            "{error}"
        );
    }
}

#[test]
fn a_project_marked_by_settings_keeps_each_kind_it_names_where_it_says() {
    let dir = tempfile::TempDir::new().unwrap();
    settings_project(
        dir.path(),
        Directories::default().with(Kind::Skill, "docs/skills"),
    );

    let project = MindProject::open(dir.path()).unwrap();

    assert_eq!(project.config().version(), SETTINGS_TOML_VERSION);
    assert_eq!(project.marker_file(), MarkerFile::SettingsToml);
    assert_eq!(project.name(), "both");
    assert_eq!(
        project.directory_for(Kind::Skill),
        Some(dir.path().join("docs/skills"))
    );
    assert_eq!(project.directory_for(Kind::Rule), None);
}

/// `list = <array>` with every entry pointing at `entry` dropped, as the
/// marker would read afterwards.
fn dropping(array: &str, entry: &str) -> String {
    let mut document: toml_edit::DocumentMut = format!("list = {array}\n").parse().unwrap();
    let root = Path::new("/w");
    let removed = remove_pointing_at(
        document["list"].as_array_mut().unwrap(),
        root,
        &root.join(entry),
    );
    assert_eq!(removed, [PathBuf::from(entry)], "{array}");
    document
        .to_string()
        .strip_prefix("list = ")
        .unwrap()
        .strip_suffix('\n')
        .unwrap()
        .to_owned()
}

#[test]
fn an_entry_moving_into_a_dropped_ones_place_takes_its_layout() {
    assert_eq!(dropping(r#"["a", "b"]"#, "a"), r#"["b"]"#);
    assert_eq!(dropping(r#"[ "a", "b" ]"#, "a"), r#"[ "b" ]"#);
    assert_eq!(dropping(r#"["a", "b", "c"]"#, "b"), r#"["a", "c"]"#);
    assert_eq!(
        dropping("[\n    \"a\",\n    \"b\",\n]", "a"),
        "[\n    \"b\",\n]"
    );
    assert_eq!(dropping("[\n    \"a\", \"b\",\n]", "a"), "[\n    \"b\",\n]");
    assert_eq!(dropping("[\"a\",\n    \"b\"]", "a"), "[\n    \"b\"]");
}

#[test]
fn dropping_an_entry_takes_its_comments_and_leaves_everyone_elses() {
    let above = "[\n    # a\n    \"a\",\n    # b\n    \"b\",\n    # c\n    \"c\",\n]";
    assert_eq!(
        dropping(above, "b"),
        "[\n    # a\n    \"a\",\n    # c\n    \"c\",\n]"
    );
    assert_eq!(
        dropping(above, "a"),
        "[\n    # b\n    \"b\",\n    # c\n    \"c\",\n]"
    );

    let beside = "[ # the list\n    \"a\", # a\n    \"b\", # b\n    \"c\", # c\n]";
    assert_eq!(
        dropping(beside, "b"),
        "[ # the list\n    \"a\", # a\n    \"c\", # c\n]"
    );
    assert_eq!(
        dropping(beside, "a"),
        "[ # the list\n    \"b\", # b\n    \"c\", # c\n]"
    );
}

/// A project read from `config` at `root`, without touching the disk.
fn project_at(root: &str, config: ProjectConfig) -> MindProject {
    MindProject {
        root: PathBuf::from(root),
        config,
        leftovers: Vec::new(),
    }
}

/// What a member serializes to is a contract, as a status report's is: the
/// entry and where it points, then what the project there says of itself,
/// or why there is none, by its code and message.
#[test]
fn a_member_serializes_its_entry_and_the_project_it_opens() {
    let mut settings = Settings::new(
        "api",
        vec![Agent::Claude, Agent::Codex],
        Share::Local,
        Directories::default(),
    );
    settings.id = ProjectId::parse("0123456789abcdef0123456789abcdef").unwrap();
    let older = MindConfig {
        version: MIND_TOML_VERSION,
        name: String::from("old"),
        directories: Directories::default(),
    };
    let gone = || WorkspaceError::NotAProject {
        path: PathBuf::from("/gone"),
    };
    let members = [
        Member {
            entry: PathBuf::from("api"),
            root: PathBuf::from("/work/api"),
            project: Ok(project_at("/work/api", ProjectConfig::Settings(settings))),
        },
        Member {
            entry: PathBuf::from("old"),
            root: PathBuf::from("/work/old"),
            project: Ok(project_at("/work/old", ProjectConfig::Toml(older))),
        },
        Member {
            entry: PathBuf::from("../gone"),
            root: PathBuf::from("/gone"),
            project: Err(gone()),
        },
    ];

    let serialized = serde_json::to_string(&members).unwrap();

    assert_eq!(
        serialized,
        [
            r#"[{"entry":"api","root":"/work/api","name":"api","marker":"settings.toml","#,
            r#""id":"0123456789abcdef0123456789abcdef","agents":["claude","codex"],"#,
            r#""share":"local","error":null},"#,
            r#"{"entry":"old","root":"/work/old","name":"old","marker":"mind.toml","#,
            r#""id":null,"agents":[],"share":null,"error":null},"#,
            r#"{"entry":"../gone","root":"/gone","name":null,"marker":null,"#,
            r#""id":null,"agents":[],"share":null,"#,
            &format!(
                r#""error":{{"code":"not_a_project","message":"{}"}}}}]"#,
                gone()
            ),
        ]
        .concat()
    );
}

/// The same for a workspace one loads: the entry, where it points, and the
/// workspace's name, or why there is none.
#[test]
fn a_loaded_workspace_serializes_its_entry_and_its_name() {
    let team = WeaverWorkspace {
        root: PathBuf::from("/team"),
        config: WeaverConfig {
            version: WEAVER_FORMAT_VERSION,
            name: String::from("team"),
            projects: Vec::new(),
            loaded: Vec::new(),
            known: Vec::new(),
            finders: None,
            directories: Directories::default(),
        },
        made: true,
        names: Names::Mindweaver,
        leftover: None,
    };
    let gone = || WorkspaceError::NotAWorkspace {
        path: PathBuf::from("/gone"),
    };
    let loads = [
        Loaded {
            entry: PathBuf::from("../team"),
            root: PathBuf::from("/team"),
            workspace: Ok(team),
        },
        Loaded {
            entry: PathBuf::from("../gone"),
            root: PathBuf::from("/gone"),
            workspace: Err(gone()),
        },
    ];

    let serialized = serde_json::to_string(&loads).unwrap();

    assert_eq!(
        serialized,
        [
            r#"[{"entry":"../team","root":"/team","name":"team","error":null},"#,
            r#"{"entry":"../gone","root":"/gone","name":null,"#,
            &format!(
                r#""error":{{"code":"not_a_workspace","message":"{}"}}}}]"#,
                gone()
            ),
        ]
        .concat()
    );
}

#[test]
fn a_share_mode_is_serialized_by_its_name() {
    assert_eq!(
        serde_json::to_string(&Share::ALL).unwrap(),
        r#"["local","shared"]"#
    );
}

#[test]
fn a_share_mode_is_read_back_from_its_name_and_no_other_spelling() {
    let read: Vec<Share> = serde_json::from_str(r#"["local","shared"]"#).unwrap();

    assert_eq!(read, Share::ALL);
    let error = serde_json::from_str::<Share>(r#""Local""#).unwrap_err();
    assert_eq!(
        error.to_string(),
        "unknown share mode `Local`; expected local or shared"
    );
}

/// Inside `.mindweaver`, apart from every folder a workspace keeps what it
/// holds in: its library, its shelf and its cache; and the route a line run
/// from the root names it by leads there.
#[test]
fn a_workspaces_scratch_folder_is_its_own_inside_mindweaver() {
    let dir = tempfile::TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();

    let scratch = workspace.scratch_dir();

    assert_eq!(scratch, dir.path().join(WEAVER_DIR).join(SCRATCH_DIR));
    assert_eq!(workspace.scratch_route(), ".mindweaver/tmp");
    assert_eq!(workspace.root().join(workspace.scratch_route()), scratch);
    let own = workspace.own();
    let mut kept: Vec<PathBuf> = Kind::ALL
        .into_iter()
        .flat_map(|kind| {
            [
                own.directory_for(kind).unwrap(),
                workspace.gathered_dir(kind),
            ]
        })
        .collect();
    kept.push(workspace.cache_dir());
    for folder in kept {
        assert!(
            !folder.starts_with(&scratch) && !scratch.starts_with(&folder),
            "{}",
            folder.display()
        );
    }
    assert!(
        !scratch.exists(),
        "nothing makes it until something is put in it"
    );
}

#[test]
fn a_workspace_a_mindflayer_build_made_is_read_under_its_own_names() {
    let (dir, made) = crate::mother::MindflayerMother::empty();

    let opened = WeaverWorkspace::open(dir.path()).unwrap();

    assert_eq!(opened, made);
    assert_eq!(opened.names(), Names::Mindflayer);
    assert_eq!(opened.weaver_dir(), dir.path().join(FLAYER_DIR));
    assert_eq!(
        opened.config_path(),
        dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG)
    );
    assert_eq!(
        opened.ledger_path(),
        dir.path().join(FLAYER_DIR).join("mindflayer.db")
    );
    assert_eq!(opened.scratch_route(), ".mindflayer/tmp");
    assert_eq!(opened.leftover(), None);
    // Its own in its library, inside the folder it is kept in, as its
    // marker says and a kind it names nothing for is sent.
    assert_eq!(
        opened.config().directories.get(Kind::Skill),
        Some(Path::new(".mindflayer/library/skills"))
    );
    assert_eq!(
        opened.own().directory_for(Kind::Rule),
        Some(dir.path().join(".mindflayer/library/rules"))
    );
}

#[test]
fn init_takes_a_workspace_a_mindflayer_build_made_for_the_one_there() {
    let (dir, made) = crate::mother::MindflayerMother::empty();

    let (workspace, initialized) = WeaverWorkspace::init(dir.path()).unwrap();

    assert_eq!(initialized, Initialization::AlreadyInitialized);
    assert_eq!(workspace, made);
    assert!(!dir.path().join(WEAVER_DIR).exists());
}

#[test]
fn where_both_are_the_new_workspace_is_read_and_the_old_is_left_over() {
    let dir = tempfile::TempDir::new().unwrap();
    crate::mother::MindflayerMother::create(dir.path());
    let (made, _) = WeaverWorkspace::init_under(
        &dir.path().join("elsewhere"),
        &Directories::default(),
        Names::Mindweaver,
    )
    .unwrap();
    fs::rename(made.weaver_dir(), dir.path().join(WEAVER_DIR)).unwrap();

    let opened = WeaverWorkspace::open(dir.path()).unwrap();

    assert_eq!(opened.names(), Names::Mindweaver);
    assert_eq!(
        opened.leftover(),
        Some(dir.path().join(FLAYER_DIR).as_path())
    );
    assert!(WeaverWorkspace::is_marked(dir.path()));
    assert_eq!(
        WeaverWorkspace::names_at(dir.path()),
        Some(Names::Mindweaver)
    );
}

#[test]
fn no_install_lands_in_a_workspace_folder_under_either_names() {
    for folder in [
        ".mindweaver/skills",
        ".mindflayer/skills",
        "a/.MindFlayer/b",
    ] {
        assert_eq!(
            misplaced_target(Kind::Skill, Path::new(folder)),
            Some(Misplaced::InsideWorkspace),
            "{folder}"
        );
    }
}

#[test]
fn the_shelf_and_cache_are_gathering_s_under_either_names() {
    for folder in [
        ".mindweaver/skills",
        ".mindflayer/cache",
        ".mindflayer/rules/x",
    ] {
        assert!(
            misplaced_own(Kind::Rule, Path::new(folder)).is_some(),
            "{folder}"
        );
    }
    assert!(misplaced_own(Kind::Rule, Path::new(".mindflayer/library/rules")).is_none());
}
