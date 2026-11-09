//! Unit tests for `policy.rs`: what a request from outside may name, and the
//! code each thing it may not name is refused with.

use std::fs;
use std::path::MAIN_SEPARATOR_STR as SEPARATOR;

use tempfile::TempDir;

use super::*;
use crate::known;
use crate::mother::UrlMother;
use crate::workspace::{MindProject, WEAVER_DIR};

/// A workspace linking `alpha` under it and `beside` next to it, and loading
/// `library`, a workspace next to it too.
fn fixture() -> (TempDir, WeaverWorkspace) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("work");
    let (mut workspace, _) = WeaverWorkspace::init(&root).unwrap();
    for project in [root.join("alpha"), dir.path().join("beside")] {
        let (project, _) = MindProject::init(project).unwrap();
        workspace.link(&project).unwrap();
    }
    let (library, _) = WeaverWorkspace::init(dir.path().join("library")).unwrap();
    workspace.load(&library).unwrap();
    let workspace = WeaverWorkspace::open(&root).unwrap();
    (dir, workspace)
}

/// `parts` joined under `base` with the platform's separator, as given.
fn spelled_under(base: &Path, parts: &[&str]) -> String {
    let mut text = base.to_str().unwrap().to_owned();
    for part in parts {
        text.push_str(SEPARATOR);
        text.push_str(part);
    }
    text
}

fn code<T: std::fmt::Debug>(result: Result<T, PolicyError>) -> &'static str {
    result.expect_err("it was taken").code()
}

#[test]
fn an_absolute_normalized_path_outside_any_workspace_folder_is_taken_as_given() {
    let (dir, _workspace) = fixture();
    let given = spelled_under(dir.path(), &["new", "project"]);

    assert_eq!(path(&given).unwrap(), PathBuf::from(&given));
}

#[test]
fn a_relative_path_is_refused_whatever_it_starts_with() {
    for given in ["beta", "./beta", "../beta", "-rf", "--yes", "", "~/beta"] {
        assert_eq!(code(path(given)), "path_relative", "{given:?}");
    }
}

#[test]
fn a_url_is_refused_as_one() {
    for given in [
        "file:///etc",
        "https://example.com/repo",
        "ssh://git@host/x",
    ] {
        assert_eq!(code(path(given)), "path_url", "{given:?}");
    }
}

/// An address is the gather's to judge, a credential in it among what it
/// refuses: every form a source is typed in, with or without one, is
/// taken here as given.
#[test]
fn an_address_to_gather_from_is_a_value_the_gather_judges() {
    // Every clean form but the two that name a folder on this machine.
    let typed = UrlMother::every_clean()
        .into_iter()
        .filter(|url| !url.starts_with("file://") && !url.starts_with('/'))
        .chain(
            UrlMother::every_credentialed()
                .into_iter()
                .map(|url| url.typed),
        );

    for given in typed {
        assert!(source(&given).is_ok(), "{given:?}");
    }
    assert!(source("github.com:acme/skills.git").is_ok());
}

/// A folder on this machine, as a path or behind `file://`, is held as a
/// folder to link is, so what the preview shows is the repository read.
#[test]
fn a_folder_to_gather_from_is_held_as_a_folder_to_link_is() {
    let (dir, workspace) = fixture();
    let beside = spelled_under(dir.path(), &["src", "acme-skills"]);
    let inside = spelled_under(&workspace.weaver_dir(), &["cache", "acme"]);

    assert!(source(&beside).is_ok());
    // A `file://` URL's path, as a Unix path is spelled after it.
    #[cfg(unix)]
    {
        assert!(source(&format!("file://{beside}")).is_ok());
        assert!(source(&format!("FILE://{beside}")).is_ok());
        assert_eq!(
            code(source(&format!("file://{inside}"))),
            "path_in_mindflayer"
        );
    }
    for (given, refused) in [
        (String::from("../src/acme-skills"), "path_relative"),
        (String::from("acme-skills"), "path_relative"),
        (String::from("./acme:skills"), "path_relative"),
        (String::from("file://acme/skills"), "path_relative"),
        (
            spelled_under(dir.path(), &["src", "..", "src"]),
            "path_unnormalized",
        ),
        (inside, "path_in_mindflayer"),
    ] {
        assert_eq!(code(source(&given)), refused, "{given:?}");
    }
}

#[test]
fn a_path_not_spelled_as_its_folder_is_refused() {
    let (dir, _workspace) = fixture();
    let base = dir.path();
    let doubled = format!("{}{SEPARATOR}{SEPARATOR}alpha", base.to_str().unwrap());

    for given in [
        spelled_under(base, &["work", "..", "beside"]),
        spelled_under(base, &["work", ".", "alpha"]),
        spelled_under(base, &["..", "..", "etc"]),
        format!("{}{SEPARATOR}", spelled_under(base, &["beside"])),
        doubled,
    ] {
        assert_eq!(code(path(&given)), "path_unnormalized", "{given:?}");
    }
}

#[test]
fn a_path_inside_a_mindweaver_is_refused() {
    let (dir, workspace) = fixture();
    let weaver = workspace.weaver_dir();

    for given in [
        weaver.to_str().unwrap().to_owned(),
        spelled_under(&weaver, &["shelf", "github.com"]),
        // One that does not exist yet is judged on its spelling.
        spelled_under(dir.path(), &["elsewhere", WEAVER_DIR, "skills"]),
    ] {
        assert_eq!(code(path(&given)), "path_in_mindflayer", "{given:?}");
    }
}

/// A Mindflayer build's `.mindflayer` is a workspace's own folder too,
/// refused under the code it always was, which says the folder it is in.
#[test]
fn a_path_inside_a_mindflayer_a_mindflayer_build_made_is_refused() {
    let dir = TempDir::new().unwrap();
    let old = crate::mother::MindflayerMother::create(&dir.path().join("old"));

    for given in [
        spelled_under(&old.weaver_dir(), &["library", "skills"]),
        spelled_under(dir.path(), &["elsewhere", ".mindflayer"]),
    ] {
        let refused = path(&given).expect_err("it was taken");
        assert_eq!(refused.code(), "path_in_mindflayer", "{given:?}");
        assert!(
            refused.to_string().contains("inside a `.mindflayer`"),
            "{refused}"
        );
    }
    let refused = path(&spelled_under(dir.path(), &[WEAVER_DIR])).unwrap_err();
    assert!(
        refused.to_string().contains("inside a `.mindweaver`"),
        "{refused}"
    );
}

#[cfg(unix)]
#[test]
fn a_path_that_leads_into_a_mindweaver_through_a_link_is_refused() {
    let (dir, workspace) = fixture();
    let link = dir.path().join("innocent");
    std::os::unix::fs::symlink(workspace.weaver_dir(), &link).unwrap();

    assert_eq!(code(path(link.to_str().unwrap())), "path_in_mindflayer");
}

/// Followed as far as it exists: `mind init` would make the new folder
/// through the link, inside the workspace's own.
#[cfg(unix)]
#[test]
fn a_new_folder_under_a_link_into_a_mindweaver_is_refused() {
    let (dir, workspace) = fixture();
    let link = dir.path().join("innocent");
    std::os::unix::fs::symlink(workspace.weaver_dir(), &link).unwrap();

    for parts in [&["new"][..], &["new", "deeper"]] {
        let given = spelled_under(&link, parts);
        assert_eq!(code(path(&given)), "path_in_mindflayer", "{given}");
    }
}

#[test]
fn a_linked_project_is_named_by_its_entry_as_the_marker_spells_it() {
    let (dir, workspace) = fixture();

    let alpha = linked(&workspace, "alpha").unwrap();
    assert_eq!(alpha.root, workspace.root().join("alpha"));
    let beside = linked(&workspace, "../beside").unwrap();
    assert_eq!(beside.root, dir.path().join("beside"));
}

#[test]
fn anything_but_an_entry_the_registry_holds_names_no_linked_project() {
    let (dir, workspace) = fixture();
    let absolute = workspace.root().join("alpha");

    for given in [
        "beta",
        "./alpha",
        "alpha/",
        "-C",
        "--home=/etc",
        absolute.to_str().unwrap(),
        // Its name, which another project could declare too.
        "beside",
        dir.path().join("beside").to_str().unwrap(),
    ] {
        assert_eq!(
            code(linked(&workspace, given)),
            "not_registered",
            "{given:?}"
        );
    }
}

#[test]
fn a_loaded_workspace_is_named_by_its_entry_as_the_marker_spells_it() {
    let (dir, workspace) = fixture();

    assert_eq!(
        loaded(&workspace, "../library").unwrap().root,
        dir.path().join("library")
    );
    for given in ["library", "../library/", "-x", "../beside"] {
        assert_eq!(code(loaded(&workspace, given)), "not_loaded", "{given:?}");
    }
}

#[test]
fn a_path_to_a_linked_project_or_a_loaded_workspace_is_one_an_entry_points_at() {
    let (dir, workspace) = fixture();
    let beside = dir.path().join("beside");
    let library = dir.path().join("library");

    assert_eq!(
        linked_path(&workspace, beside.to_str().unwrap()).unwrap(),
        beside
    );
    assert_eq!(
        loaded_path(&workspace, library.to_str().unwrap()).unwrap(),
        library
    );
    assert_eq!(
        code(linked_path(&workspace, library.to_str().unwrap())),
        "not_registered"
    );
    assert_eq!(
        code(loaded_path(&workspace, beside.to_str().unwrap())),
        "not_loaded"
    );
    // Held to the path rules first.
    assert_eq!(code(linked_path(&workspace, "../beside")), "path_relative");
}

#[test]
fn a_resolved_path_is_spelled_as_it_is() {
    let (dir, _workspace) = fixture();

    assert_eq!(spelled(dir.path()).unwrap(), dir.path().to_str().unwrap());
}

#[cfg(unix)]
#[test]
fn a_resolved_path_that_is_not_utf8_cannot_be_spelled() {
    use std::os::unix::ffi::OsStrExt as _;
    let path = Path::new(std::ffi::OsStr::from_bytes(b"/work/\xff"));

    assert_eq!(code(spelled(path)), "path_not_utf8");
}

#[test]
fn nothing_named_here_touches_the_disk_but_to_read() {
    let (dir, workspace) = fixture();
    let before = fs::read_to_string(workspace.weaver_dir().join(crate::WEAVER_CONFIG)).unwrap();

    let _ = path(&spelled_under(dir.path(), &["new"]));
    let _ = linked(&workspace, "alpha");
    let _ = loaded(&workspace, "../library");

    assert!(!dir.path().join("new").exists());
    assert_eq!(
        fs::read_to_string(workspace.weaver_dir().join(crate::WEAVER_CONFIG)).unwrap(),
        before
    );
}

/// What a front end launched on `launched` may switch to, the default in
/// `home` knowing `remembered`, while it is on `in_use`.
fn listed(home: &Path, launched: &WeaverWorkspace, in_use: &WeaverWorkspace) -> Vec<Switchable> {
    let launch = known::Launch {
        root: launched.root(),
        reason: Reason::Above,
    };
    known::switchable(Some(home), &launch, in_use)
}

/// A default workspace in `home` remembering `writing` beside it, and
/// `elsewhere`, a workspace nothing remembers, the one launched on.
fn machine() -> (TempDir, PathBuf, WeaverWorkspace, WeaverWorkspace) {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    WeaverWorkspace::init(&home).unwrap();
    let (writing, _) = WeaverWorkspace::init(dir.path().join("writing")).unwrap();
    known::remember(Some(&home), &writing).unwrap();
    let (elsewhere, _) = WeaverWorkspace::init(dir.path().join("elsewhere")).unwrap();
    (dir, home, writing, elsewhere)
}

#[test]
fn a_listed_workspace_is_switched_to_by_its_path_as_listed_with_its_reason() {
    let (_dir, home, writing, elsewhere) = machine();
    let rows = listed(&home, &elsewhere, &elsewhere);

    let (to, reason) = switched(&rows, writing.root().to_str().unwrap()).unwrap();
    assert_eq!(to.root(), writing.root());
    assert_eq!(reason, Reason::Explicit);

    // Back to the launch, by the rule that chose it then.
    let rows = listed(&home, &elsewhere, &writing);
    let (to, reason) = switched(&rows, elsewhere.root().to_str().unwrap()).unwrap();
    assert_eq!(to.root(), elsewhere.root());
    assert_eq!(reason, Reason::Above);
}

#[test]
fn a_path_no_list_holds_is_refused_whatever_it_leads_to() {
    let (dir, home, writing, elsewhere) = machine();
    let (unknown, _) = WeaverWorkspace::init(dir.path().join("unknown")).unwrap();
    let rows = listed(&home, &elsewhere, &elsewhere);
    let writing = writing.root().to_str().unwrap();
    let other = [
        // A workspace, but none this machine knows.
        unknown.root().to_str().unwrap().to_owned(),
        // Listed ones, spelled otherwise: through `.mindweaver`, with a
        // trailing separator, or with a `.` part.
        format!("{writing}{SEPARATOR}.mindweaver"),
        format!("{writing}{SEPARATOR}"),
        spelled_under(dir.path(), &[".", "writing"]),
        String::from("writing"),
        String::from("-W"),
        String::from("--workspace=/"),
        String::new(),
    ];
    for given in other {
        assert_eq!(
            code(switched(&rows, &given)),
            "switch_unlisted",
            "{given:?}"
        );
    }
}

#[test]
fn a_listed_workspace_that_does_not_open_is_refused_and_never_made() {
    let (_dir, home, writing, elsewhere) = machine();
    let rows = listed(&home, &elsewhere, &elsewhere);
    fs::remove_dir_all(writing.root().join(WEAVER_DIR)).unwrap();

    let refused = switched(&rows, writing.root().to_str().unwrap());

    assert_eq!(code(refused), "not_a_workspace");
    assert!(!writing.root().join(WEAVER_DIR).exists(), "it was made");
}

#[test]
fn the_one_in_use_is_taken_as_a_switch_that_changes_nothing() {
    let (_dir, home, _writing, elsewhere) = machine();
    let rows = listed(&home, &elsewhere, &elsewhere);

    let (to, reason) = switched(&rows, elsewhere.root().to_str().unwrap()).unwrap();

    assert_eq!(to.root(), elsewhere.root());
    assert_eq!(reason, Reason::Above);
}

#[test]
fn a_yes_is_taken_only_with_a_server_from_elsewhere_than_the_library() {
    for from in ["load:../team", "https://github.com/acme/servers"] {
        assert!(confirmed(Kind::Mcp, "github", Some(from)).is_ok(), "{from}");
    }
    for (kind, from) in [
        (Kind::Mcp, Some(install::WORKSPACE_ORIGIN)),
        (Kind::Mcp, None),
        (Kind::Skill, Some("load:../team")),
        (Kind::Rule, Some("load:../team")),
    ] {
        let refused = confirmed(kind, "github", from);

        assert_eq!(code(refused), "install_yes_unasked", "{kind:?} {from:?}");
    }
    assert_eq!(
        confirmed(Kind::Mcp, "github", None)
            .unwrap_err()
            .to_string(),
        "MCP server github from no place named: nothing is asked before it is installed, so no \
         yes is sent with it"
    );
}

#[test]
fn a_yes_refused_says_where_it_comes_from_without_the_credential_it_was_typed_with() {
    use crate::mother::UrlMother;

    for credentialed in UrlMother::every_credentialed() {
        let from = format!("{}#main", credentialed.typed);

        let said = confirmed(Kind::Skill, "deploy", Some(&from))
            .unwrap_err()
            .to_string();

        assert!(!said.contains(&credentialed.secret), "{said}");
        assert!(
            said.contains(&format!(" from {}#main:", credentialed.clean)),
            "{said}"
        );
    }
}
