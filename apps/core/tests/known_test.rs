//! The workspaces this machine knows: remembered in the default workspace's
//! marker, never made for the occasion, and forgotten by any spelling.

use std::fs;
use std::path::{Path, PathBuf};

use mindweaver_core::known::{self, Launch};
use mindweaver_core::resolve::Reason;
use mindweaver_core::{
    MindProject, Registration, WeaverWorkspace, WorkspaceError, WEAVER_CONFIG, WEAVER_DIR,
};
use tempfile::TempDir;

/// A temporary directory holding `home`, a made default workspace, and
/// `home/weavers/writing`, a workspace somebody made under it.
fn home_and_writing() -> (TempDir, PathBuf, WeaverWorkspace) {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    WeaverWorkspace::init(&home).unwrap();
    let (writing, _) = WeaverWorkspace::init(home.join("weavers/writing")).unwrap();
    (dir, home, writing)
}

fn marker(root: &Path) -> String {
    fs::read_to_string(root.join(WEAVER_DIR).join(WEAVER_CONFIG)).unwrap()
}

/// `text` as a TOML string, for a spelling written into the marker by
/// hand: a Windows path in a basic string would read as escapes.
fn toml_string(text: &str) -> String {
    toml::Value::String(text.to_owned()).to_string()
}

#[test]
fn remember_records_a_workspace_as_a_route_from_home_and_says_what_known_is() {
    let (_dir, home, writing) = home_and_writing();

    let remembered = known::remember(Some(&home), &writing).unwrap();

    assert_eq!(
        remembered,
        Some((PathBuf::from("weavers/writing"), Registration::Added))
    );
    let text = marker(&home);
    assert!(text.contains("known = [\"weavers/writing\"]"), "{text}");
    assert!(
        text.contains("# `known` lists the other weaver workspaces on this machine"),
        "{text}"
    );
    assert!(text.contains("# Mindweaver workspace."), "comments kept");
    assert_eq!(
        WeaverWorkspace::open(&home).unwrap().config().known,
        [PathBuf::from("weavers/writing")]
    );
}

#[test]
fn remembering_twice_changes_nothing_and_says_the_spelling_the_file_uses() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    let before = marker(&home);

    let again = WeaverWorkspace::open(home.join("weavers/./writing/")).unwrap();
    let remembered = known::remember(Some(&home), &again).unwrap();

    assert_eq!(
        remembered,
        Some((
            PathBuf::from("weavers/writing"),
            Registration::AlreadyRegistered
        ))
    );
    assert_eq!(marker(&home), before);
}

#[test]
fn a_second_workspace_goes_after_the_first_under_one_comment() {
    let (_dir, home, writing) = home_and_writing();
    let (notes, _) = WeaverWorkspace::init(home.join("notes")).unwrap();

    known::remember(Some(&home), &writing).unwrap();
    known::remember(Some(&home), &notes).unwrap();

    let text = marker(&home);
    assert!(
        text.contains("known = [\"weavers/writing\", \"notes\"]"),
        "{text}"
    );
    assert_eq!(text.matches("# `known` lists").count(), 1, "{text}");
}

#[test]
fn a_workspace_outside_home_is_remembered_by_its_route_from_there() {
    let (dir, home, _) = home_and_writing();
    let (elsewhere, _) = WeaverWorkspace::init(dir.path().join("elsewhere")).unwrap();

    let remembered = known::remember(Some(&home), &elsewhere).unwrap();

    assert_eq!(
        remembered,
        Some((PathBuf::from("../elsewhere"), Registration::Added))
    );
}

#[test]
fn nothing_is_remembered_without_a_default() {
    let (_dir, home, writing) = home_and_writing();
    let before = marker(&home);

    assert_eq!(known::remember(None, &writing).unwrap(), None);
    assert_eq!(marker(&home), before);
}

#[test]
fn a_default_not_made_yet_is_not_made_to_remember_one() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();
    let (writing, _) = WeaverWorkspace::init(dir.path().join("writing")).unwrap();

    assert_eq!(known::remember(Some(&home), &writing).unwrap(), None);
    assert!(!home.join(WEAVER_DIR).exists(), "the default was made");
}

#[test]
fn the_default_workspace_does_not_remember_itself() {
    let (_dir, home, _) = home_and_writing();
    let before = marker(&home);
    let default = WeaverWorkspace::open(&home).unwrap();

    assert_eq!(known::remember(Some(&home), &default).unwrap(), None);
    assert_eq!(marker(&home), before);
}

#[test]
fn a_default_written_by_a_newer_build_is_refused_and_left_alone() {
    let (_dir, home, writing) = home_and_writing();
    let newer = marker(&home).replace("version = 2", "version = 99");
    fs::write(home.join(WEAVER_DIR).join(WEAVER_CONFIG), &newer).unwrap();

    let error = known::remember(Some(&home), &writing).unwrap_err();

    assert!(matches!(error, WorkspaceError::Version { .. }), "{error}");
    assert_eq!(marker(&home), newer);
}

#[test]
fn forget_removes_every_spelling_of_a_workspace() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    let absolute = writing.root().to_string_lossy().into_owned();
    let path = home.join(WEAVER_DIR).join(WEAVER_CONFIG);
    let text = marker(&home).replace(
        "known = [\"weavers/writing\"]",
        &format!(
            "known = [\"weavers/writing\", \"./weavers/writing/\", {}]",
            toml_string(&absolute)
        ),
    );
    fs::write(&path, text).unwrap();

    let forgotten = known::forget(&home, &home.join("weavers/writing")).unwrap();

    assert_eq!(
        forgotten,
        [
            PathBuf::from("weavers/writing"),
            PathBuf::from("./weavers/writing/"),
            PathBuf::from(&absolute),
        ]
    );
    let text = marker(&home);
    assert!(text.contains("known = []"), "{text}");
    assert!(WeaverWorkspace::open(&home)
        .unwrap()
        .config()
        .known
        .is_empty());
}

#[test]
fn a_workspace_that_has_gone_is_forgotten_by_its_path() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    fs::remove_dir_all(writing.root()).unwrap();

    let forgotten = known::forget(&home, writing.root()).unwrap();

    assert_eq!(forgotten, [PathBuf::from("weavers/writing")]);
}

#[test]
fn forgetting_takes_the_comment_above_its_entry_and_leaves_the_others() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    let path = home.join(WEAVER_DIR).join(WEAVER_CONFIG);
    let text = marker(&home).replace(
        "known = [\"weavers/writing\"]",
        "known = [\n    # the work weaver\n    \"weavers/work\",\n    # the writing weaver\n    \
         \"weavers/writing\",\n    # notes, keep\n    \"weavers/notes\",\n]",
    );
    fs::write(&path, text).unwrap();

    known::forget(&home, writing.root()).unwrap();
    let text = marker(&home);
    assert!(
        text.contains(
            "known = [\n    # the work weaver\n    \"weavers/work\",\n    \
             # notes, keep\n    \"weavers/notes\",\n]\n"
        ),
        "{text}"
    );

    known::forget(&home, &home.join("weavers/work")).unwrap();
    let text = marker(&home);
    assert!(
        text.contains("known = [\n    # notes, keep\n    \"weavers/notes\",\n]\n"),
        "{text}"
    );
}

#[test]
fn forgetting_what_nothing_remembers_is_refused_and_writes_nothing() {
    let (_dir, home, writing) = home_and_writing();
    let before = marker(&home);

    let error = known::forget(&home, writing.root()).unwrap_err();

    assert!(matches!(error, WorkspaceError::NotKnown { .. }), "{error}");
    assert_eq!(
        error.to_string(),
        format!(
            "{}: not known to the default workspace at {}",
            writing.root().display(),
            home.display()
        )
    );
    assert_eq!(marker(&home), before, "no empty `known` left behind");
}

#[test]
fn forgetting_with_no_default_made_is_refused_and_makes_nothing() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();

    let error = known::forget(&home, &dir.path().join("writing")).unwrap_err();

    assert!(matches!(error, WorkspaceError::NotKnown { .. }), "{error}");
    assert!(!home.join(WEAVER_DIR).exists(), "the default was made");
}

#[test]
fn linking_into_the_default_keeps_what_it_knows() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    let (project, _) = MindProject::init(home.join("collapse")).unwrap();

    WeaverWorkspace::open(&home)
        .unwrap()
        .link(&project)
        .unwrap();

    let text = marker(&home);
    assert!(text.contains("known = [\"weavers/writing\"]"), "{text}");
    assert!(text.contains("projects = [\"collapse\"]"), "{text}");
}

// ---------------------------------------------------------------------------
// The list
// ---------------------------------------------------------------------------

/// Each listed workspace as `(name, path, default, remembered, in use)`.
fn listed(known: &known::Known) -> Vec<(Option<&str>, &Path, bool, bool, bool)> {
    known
        .workspaces
        .iter()
        .map(|listed| {
            (
                listed.name.as_deref(),
                listed.path.as_path(),
                listed.default,
                listed.remembered,
                listed.in_use,
            )
        })
        .collect()
}

#[test]
fn the_default_comes_first_then_what_it_remembers_in_order() {
    let (_dir, home, writing) = home_and_writing();
    let (notes, _) = WeaverWorkspace::init(home.join("notes")).unwrap();
    known::remember(Some(&home), &writing).unwrap();
    known::remember(Some(&home), &notes).unwrap();

    let (known, failures) = known::known(Some(&home), None);

    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(
        listed(&known),
        [
            (Some("home"), home.as_path(), true, false, false),
            (Some("writing"), writing.root(), false, true, false),
            (Some("notes"), notes.root(), false, true, false),
        ]
    );
}

#[test]
fn the_workspace_in_use_is_marked_where_it_is_listed() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();

    let (known, _) = known::known(Some(&home), Some(&writing));

    assert_eq!(
        listed(&known),
        [
            (Some("home"), home.as_path(), true, false, false),
            (Some("writing"), writing.root(), false, true, true),
        ]
    );
}

#[test]
fn one_in_use_that_nothing_remembers_is_listed_last() {
    let (dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    let (elsewhere, _) = WeaverWorkspace::init(dir.path().join("elsewhere")).unwrap();

    let (known, _) = known::known(Some(&home), Some(&elsewhere));

    assert_eq!(
        listed(&known),
        [
            (Some("home"), home.as_path(), true, false, false),
            (Some("writing"), writing.root(), false, true, false),
            (Some("elsewhere"), elsewhere.root(), false, false, true),
        ]
    );
}

#[test]
fn a_remembered_workspace_that_has_gone_is_listed_with_why_and_a_failure() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    fs::remove_dir_all(writing.root()).unwrap();

    let (known, failures) = known::known(Some(&home), None);

    let gone = &known.workspaces[1];
    assert_eq!(gone.name, None);
    assert_eq!(gone.path, writing.root());
    assert!(gone.remembered);
    let why = format!(
        "{}: not a weaver workspace (no .mindweaver/weaver.toml or .mindflayer/flayer.toml)",
        writing.root().display()
    );
    assert_eq!(gone.stale.as_deref(), Some(why.as_str()));
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].to_string(), why);
}

#[test]
fn two_spellings_of_one_workspace_are_listed_once() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    let path = home.join(WEAVER_DIR).join(WEAVER_CONFIG);
    let absolute = writing.root().to_string_lossy().into_owned();
    let text = marker(&home).replace(
        "known = [\"weavers/writing\"]",
        &format!(
            "known = [\"weavers/writing\", {}, \".\"]",
            toml_string(&absolute)
        ),
    );
    fs::write(&path, text).unwrap();

    let (known, _) = known::known(Some(&home), None);

    // `.` is the default itself, which is listed once, as remembered too.
    assert_eq!(
        listed(&known),
        [
            (Some("home"), home.as_path(), true, true, false),
            (Some("writing"), writing.root(), false, true, false),
        ]
    );
}

#[test]
fn each_workspace_counts_its_projects_and_the_stale_ones() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    let mut writing = WeaverWorkspace::open(writing.root()).unwrap();
    for name in ["essays", "letters", "gone"] {
        let (project, _) = MindProject::init(writing.root().join(name)).unwrap();
        writing.link(&project).unwrap();
    }
    fs::remove_dir_all(writing.root().join("gone")).unwrap();

    let (known, failures) = known::known(Some(&home), None);

    assert!(
        failures.is_empty(),
        "a stale project is the count's, not a failure"
    );
    assert_eq!(
        (
            known.workspaces[0].projects,
            known.workspaces[0].stale_projects
        ),
        (0, 0)
    );
    assert_eq!(
        (
            known.workspaces[1].projects,
            known.workspaces[1].stale_projects
        ),
        (3, 1)
    );
}

#[test]
fn without_a_default_only_the_one_in_use_is_listed() {
    let (_dir, _home, writing) = home_and_writing();

    let (nothing, _) = known::known(None, None);
    let (in_use, _) = known::known(None, Some(&writing));

    assert!(nothing.workspaces.is_empty());
    assert_eq!(
        listed(&in_use),
        [(Some("writing"), writing.root(), false, false, true)]
    );
}

#[test]
fn listing_never_makes_the_default_nor_marks_it_unmade() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();
    let unmade = WeaverWorkspace::unmade(&home).unwrap();

    let (known, failures) = known::known(Some(&home), Some(&unmade));

    assert!(known.workspaces.is_empty());
    assert!(failures.is_empty());
    assert!(!home.join(WEAVER_DIR).exists(), "the default was made");
}

#[test]
fn a_default_that_will_not_open_is_listed_as_stale() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    let newer = marker(&home).replace("version = 2", "version = 99");
    fs::write(home.join(WEAVER_DIR).join(WEAVER_CONFIG), newer).unwrap();

    let (known, failures) = known::known(Some(&home), None);

    assert_eq!(known.workspaces.len(), 1, "what it knows cannot be read");
    assert!(known.workspaces[0].default);
    assert!(known.workspaces[0].stale.is_some());
    assert!(matches!(failures[..], [WorkspaceError::Version { .. }]));
}

#[test]
fn the_list_serializes_each_field_by_its_name() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();

    let (known, _) = known::known(Some(&home), Some(&writing));

    assert_eq!(
        serde_json::to_value(&known).unwrap(),
        serde_json::json!({
            "workspaces": [
                {
                    "name": "home",
                    "path": home.to_string_lossy(),
                    "default": true,
                    "remembered": false,
                    "in_use": false,
                    "projects": 0,
                    "stale_projects": 0,
                    "stale": null,
                },
                {
                    "name": "writing",
                    "path": writing.root().to_string_lossy(),
                    "default": false,
                    "remembered": true,
                    "in_use": true,
                    "projects": 0,
                    "stale_projects": 0,
                    "stale": null,
                },
            ]
        })
    );
}

#[test]
fn why_a_workspace_would_not_open_is_written_with_no_credential() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    // A marker that does not parse is quoted in the parser's own words,
    // whatever line it stopped at.
    let tainted = "https://me:ghp_t0ken@github.com/acme/skills";
    fs::write(
        writing.root().join(WEAVER_DIR).join(WEAVER_CONFIG),
        format!("version = 2\nsource = \"{tainted}\n"),
    )
    .unwrap();

    let (known, _) = known::known(Some(&home), None);

    // Held without it, not only written so: the value is `Redacted`, which
    // nothing makes from a text that keeps one. The URL is still quoted, so
    // the redaction had something to take out.
    let stale = known.workspaces[1].stale.as_deref().expect("said why");
    assert!(!stale.contains("ghp_t0ken"), "{stale}");
    assert!(stale.contains("https://github.com/acme/skills"), "{stale}");
    let json = serde_json::to_string(&known).unwrap();
    assert!(!json.contains("ghp_t0ken"), "{json}");
    assert!(json.contains("https://github.com/acme/skills"), "{json}");
}

/// Each row of a list of what a front end may switch to: its name, whether
/// it is the one in use, the one launched on, how it is chosen once switched
/// to, and whether a switch to it is offered.
fn offered(rows: &[known::Switchable]) -> Vec<(Option<&str>, bool, bool, Reason, bool)> {
    rows.iter()
        .map(|row| {
            (
                row.workspace.name.as_deref(),
                row.workspace.in_use,
                row.launched,
                row.reason,
                row.switchable,
            )
        })
        .collect()
}

#[test]
fn a_switch_is_offered_to_every_known_workspace_but_the_one_in_use() {
    let (_dir, home, writing) = home_and_writing();
    let (notes, _) = WeaverWorkspace::init(home.join("notes")).unwrap();
    known::remember(Some(&home), &writing).unwrap();
    known::remember(Some(&home), &notes).unwrap();
    let launch = Launch {
        root: writing.root(),
        reason: Reason::Explicit,
    };

    let rows = known::switchable(Some(&home), &launch, &writing);

    assert_eq!(
        offered(&rows),
        [
            (Some("home"), false, false, Reason::Explicit, true),
            (Some("writing"), true, true, Reason::Explicit, false),
            (Some("notes"), false, false, Reason::Explicit, true),
        ]
    );
}

#[test]
fn the_launch_is_listed_last_when_nothing_remembers_it_and_keeps_its_reason() {
    let (dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    let (elsewhere, _) = WeaverWorkspace::init(dir.path().join("elsewhere")).unwrap();
    let launch = Launch {
        root: elsewhere.root(),
        reason: Reason::Above,
    };

    // Switched to `writing` since: the launch is still offered, by the rule
    // that chose it, and the default by `-W`, as any other is.
    let rows = known::switchable(Some(&home), &launch, &writing);

    assert_eq!(
        offered(&rows),
        [
            (Some("home"), false, false, Reason::Explicit, true),
            (Some("writing"), true, false, Reason::Explicit, false),
            (Some("elsewhere"), false, true, Reason::Above, true),
        ]
    );
    assert_eq!(rows[2].workspace.path, elsewhere.root());
}

#[test]
fn the_default_launched_on_keeps_its_reason_and_a_gone_workspace_is_offered_nothing() {
    let (_dir, home, writing) = home_and_writing();
    known::remember(Some(&home), &writing).unwrap();
    let default = WeaverWorkspace::open(&home).unwrap();
    let launch = Launch {
        root: &home,
        reason: Reason::Home,
    };
    fs::remove_dir_all(writing.root()).unwrap();

    let rows = known::switchable(Some(&home), &launch, &default);

    assert_eq!(
        offered(&rows),
        [
            (Some("home"), true, true, Reason::Home, false),
            (None, false, false, Reason::Explicit, false),
        ]
    );
    assert!(rows[1].workspace.stale.is_some());
    assert!(!writing.root().exists(), "nothing was made");
}

#[test]
fn the_default_nothing_has_made_is_listed_while_it_is_the_one_in_use_and_never_made() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();
    let unmade = WeaverWorkspace::unmade(&home).unwrap();
    let launch = Launch {
        root: &home,
        reason: Reason::Home,
    };

    let rows = known::switchable(Some(&home), &launch, &unmade);

    assert_eq!(
        offered(&rows),
        [(Some("home"), true, true, Reason::Home, false)]
    );
    assert!(rows[0].workspace.default);
    assert!(!home.join(WEAVER_DIR).exists(), "the default was made");
}

#[test]
fn the_one_in_use_gone_since_it_was_gone_to_is_listed_with_why_and_never_made() {
    let (dir, home, writing) = home_and_writing();
    let (elsewhere, _) = WeaverWorkspace::init(dir.path().join("elsewhere")).unwrap();
    let launch = Launch {
        root: elsewhere.root(),
        reason: Reason::Explicit,
    };
    fs::remove_dir_all(elsewhere.root().join(WEAVER_DIR)).unwrap();
    // As a front end that stays on it hands it over once it will not open.
    let gone = WeaverWorkspace::unmade(elsewhere.root()).unwrap();

    let rows = known::switchable(Some(&home), &launch, &gone);

    assert_eq!(
        offered(&rows),
        [
            (Some("home"), false, false, Reason::Explicit, true),
            (None, true, true, Reason::Explicit, false),
        ]
    );
    assert_eq!(rows[1].workspace.path, elsewhere.root());
    assert!(!rows[1].workspace.default);
    assert!(rows[1].workspace.stale.is_some(), "{:?}", rows[1]);

    // Remembered, it is the row `known` lists with why, marked in use.
    known::remember(Some(&home), &writing).unwrap();
    fs::remove_dir_all(writing.root().join(WEAVER_DIR)).unwrap();
    let gone = WeaverWorkspace::unmade(writing.root()).unwrap();

    let rows = known::switchable(Some(&home), &launch, &gone);

    assert_eq!(
        offered(&rows),
        [
            (Some("home"), false, false, Reason::Explicit, true),
            (None, true, false, Reason::Explicit, false),
            (None, false, true, Reason::Explicit, false),
        ]
    );
    assert_eq!(rows[1].workspace.path, writing.root());
    assert!(rows[1].workspace.remembered);
    assert!(rows[1].workspace.stale.is_some(), "{:?}", rows[1]);
    assert!(
        !writing.root().join(WEAVER_DIR).exists(),
        "nothing was made"
    );
    assert!(
        !elsewhere.root().join(WEAVER_DIR).exists(),
        "nothing was made"
    );
}

#[test]
fn what_may_be_switched_to_serializes_each_field_by_its_name() {
    let (_dir, home, writing) = home_and_writing();
    let launch = Launch {
        root: writing.root(),
        reason: Reason::Activated,
    };

    let rows = known::switchable(Some(&home), &launch, &writing);

    assert_eq!(
        serde_json::to_value(&rows).unwrap(),
        serde_json::json!([
            {
                "name": "home",
                "path": home.to_string_lossy(),
                "default": true,
                "remembered": false,
                "in_use": false,
                "projects": 0,
                "stale_projects": 0,
                "stale": null,
                "launched": false,
                "reason": "explicit",
                "switchable": true,
            },
            {
                "name": "writing",
                "path": writing.root().to_string_lossy(),
                "default": false,
                "remembered": false,
                "in_use": true,
                "projects": 0,
                "stale_projects": 0,
                "stale": null,
                "launched": true,
                "reason": "activated",
                "switchable": false,
            },
        ])
    );
}
