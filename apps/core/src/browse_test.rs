//! Unit tests for `browse.rs`: the folders directly inside one, as a front
//! end with no terminal is handed them to pick one from.

use std::collections::BTreeMap;
use std::fs;

use tempfile::TempDir;

use super::*;
use crate::workspace::{MarkerFile, MIND_DIR, WEAVER_DIR};

/// A workspace at `work`, linking `alpha` under it, which `mind init` made,
/// and nothing else.
fn fixture() -> (TempDir, WeaverWorkspace) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("work");
    let (mut workspace, _) = WeaverWorkspace::init(&root).unwrap();
    let (alpha, _) = MindProject::init(root.join("alpha")).unwrap();
    workspace.link(&alpha).unwrap();
    let workspace = WeaverWorkspace::open(&root).unwrap();
    (dir, workspace)
}

/// The folders `folder` lists, hidden ones as `hidden` asks.
fn listed(workspace: &WeaverWorkspace, folder: &Path, hidden: bool) -> Folders {
    Folders::read(workspace, folder, hidden, None).unwrap()
}

/// The name of each folder listed, in its order.
fn names(folders: &Folders) -> Vec<&str> {
    folders
        .folders
        .iter()
        .map(|folder| folder.name.as_str())
        .collect()
}

/// Every file under `dir` with what it holds, to tell that nothing changed.
fn files(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                folders.push(entry.path());
            } else {
                found.insert(entry.path(), fs::read(entry.path()).unwrap_or_default());
            }
        }
    }
    found
}

#[test]
fn only_folders_are_listed_in_the_order_of_their_names() {
    let (dir, workspace) = fixture();
    let root = dir.path().join("work");
    for name in ["zeta", "Beta", "gamma"] {
        fs::create_dir(root.join(name)).unwrap();
    }
    fs::write(root.join("notes.md"), "a file\n").unwrap();

    let folders = listed(&workspace, &root, false);

    // By the bytes of their names, as `ls` lists them with no locale.
    assert_eq!(names(&folders), ["Beta", "alpha", "gamma", "zeta"]);
    assert!(!folders.truncated);
    assert!(folders.failures.is_empty(), "{:?}", folders.failures);
}

#[test]
fn hidden_folders_are_left_out_and_counted_unless_asked_for() {
    let (dir, workspace) = fixture();
    let root = dir.path().join("work");
    for name in [".git", ".github", "src"] {
        fs::create_dir(root.join(name)).unwrap();
    }
    fs::write(root.join(".env"), "a hidden file\n").unwrap();

    let without = listed(&workspace, &root, false);
    let with = listed(&workspace, &root, true);

    assert_eq!(names(&without), ["alpha", "src"]);
    assert_eq!(names(&with), [".git", ".github", "alpha", "src"]);
    // The workspace's own `.mindweaver` is hidden too, and counted in
    // neither, since asking for hidden folders never lists it.
    assert_eq!(without.hidden, 2);
    assert_eq!(with.hidden, 2);
}

#[cfg(unix)]
#[test]
fn a_mindweaver_is_never_listed_nor_a_link_into_one() {
    use std::os::unix::fs::symlink;

    let (dir, workspace) = fixture();
    let root = dir.path().join("work");
    let inside = root.join(WEAVER_DIR).join("kept");
    fs::create_dir(&inside).unwrap();
    symlink(root.join(WEAVER_DIR), dir.path().join("hub")).unwrap();
    symlink(&inside, dir.path().join("inside")).unwrap();
    fs::create_dir(dir.path().join("kept")).unwrap();

    let at_root = listed(&workspace, &root, true);
    let beside = listed(&workspace, dir.path(), true);

    assert_eq!(names(&at_root), ["alpha"]);
    assert_eq!(at_root.hidden, 0);
    assert_eq!(names(&beside), ["kept", "work"]);
    assert!(beside.failures.is_empty(), "{:?}", beside.failures);
}

#[test]
fn each_folder_says_what_it_is_the_strongest_first() {
    let (dir, workspace) = fixture();
    let root = dir.path().join("work");
    for (name, file) in [
        ("settings", MarkerFile::SettingsToml),
        ("json", MarkerFile::SettingsJson),
        ("legacy", MarkerFile::MindToml),
    ] {
        let mind = root.join(name).join(MIND_DIR);
        fs::create_dir_all(&mind).unwrap();
        fs::write(mind.join(file.name()), "").unwrap();
    }
    WeaverWorkspace::init(root.join("hub")).unwrap();
    fs::create_dir_all(root.join("repository").join(".git")).unwrap();
    // A worktree's `.git` is a file.
    fs::create_dir(root.join("worktree")).unwrap();
    fs::write(root.join("worktree").join(".git"), "gitdir: elsewhere\n").unwrap();
    // A project at a workspace's root, in a repository, is a project.
    let both = root.join("both");
    WeaverWorkspace::init(&both).unwrap();
    MindProject::init(&both).unwrap();
    fs::create_dir(both.join(".git")).unwrap();
    fs::create_dir(root.join("plain")).unwrap();

    let folders = listed(&workspace, &root, false);

    let said: Vec<(&str, &str)> = folders
        .folders
        .iter()
        .map(|folder| (folder.name.as_str(), folder.is.slug()))
        .collect();
    assert_eq!(
        said,
        [
            ("alpha", "project"),
            ("both", "project"),
            ("hub", "workspace"),
            ("json", "project"),
            ("legacy", "project"),
            ("plain", "folder"),
            ("repository", "repository"),
            ("settings", "project"),
            ("worktree", "repository"),
        ]
    );
    // And the folder listed, by the same words, so it can be chosen too.
    assert_eq!(folders.is, Is::Workspace);
    assert_eq!(
        listed(&workspace, &root.join("alpha"), false).is,
        Is::Project
    );
    assert_eq!(
        listed(&workspace, &root.join("plain"), false).is,
        Is::Folder
    );
}

#[test]
fn a_folder_the_workspace_links_is_said_to_be_linked() {
    let (dir, workspace) = fixture();
    let root = dir.path().join("work");
    MindProject::init(root.join("beta")).unwrap();

    let folders = listed(&workspace, &root, false);

    let linked: Vec<(&str, bool)> = folders
        .folders
        .iter()
        .map(|folder| (folder.name.as_str(), folder.linked))
        .collect();
    assert_eq!(linked, [("alpha", true), ("beta", false)]);
    assert!(!folders.linked, "the workspace's root links nothing");
    assert!(listed(&workspace, &root.join("alpha"), false).linked);
}

#[cfg(unix)]
#[test]
fn a_link_to_a_folder_is_listed_marked_and_opened_as_one() {
    use std::os::unix::fs::symlink;

    let (dir, workspace) = fixture();
    let root = dir.path().join("work");
    let elsewhere = dir.path().join("elsewhere");
    fs::create_dir_all(elsewhere.join("inside")).unwrap();
    symlink(&elsewhere, root.join("reached")).unwrap();
    // A link to the linked project is linked: the entry leads there.
    symlink(root.join("alpha"), root.join("shortcut")).unwrap();
    // A link to a file, and one to nothing, are no folders.
    fs::write(dir.path().join("file.md"), "x\n").unwrap();
    symlink(dir.path().join("file.md"), root.join("file-link")).unwrap();
    symlink(dir.path().join("gone"), root.join("dangling")).unwrap();

    let folders = listed(&workspace, &root, false);

    let said: Vec<(&str, bool, bool, Is)> = folders
        .folders
        .iter()
        .map(|folder| {
            (
                folder.name.as_str(),
                folder.symlink,
                folder.linked,
                folder.is,
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            ("alpha", false, true, Is::Project),
            ("reached", true, false, Is::Folder),
            ("shortcut", true, true, Is::Project),
        ]
    );
    assert!(folders.failures.is_empty(), "{:?}", folders.failures);
    let through = listed(&workspace, &root.join("reached"), false);
    assert_eq!(names(&through), ["inside"]);
    assert_eq!(through.folders[0].path, root.join("reached").join("inside"));
}

#[test]
fn the_trail_goes_from_the_root_down_to_the_folder_listed() {
    let (dir, workspace) = fixture();
    let alpha = dir.path().join("work").join("alpha");

    let folders = listed(&workspace, &alpha, false);

    let root = alpha.ancestors().last().unwrap();
    assert_eq!(folders.trail.first().unwrap().path, root);
    assert_eq!(folders.trail.first().unwrap().name, root.to_string_lossy());
    let last: Vec<(&str, &Path)> = folders
        .trail
        .iter()
        .rev()
        .take(2)
        .map(|crumb| (crumb.name.as_str(), crumb.path.as_path()))
        .collect();
    assert_eq!(
        last,
        [
            ("alpha", alpha.as_path()),
            ("work", dir.path().join("work").as_path())
        ]
    );
    assert_eq!(folders.trail.len(), alpha.ancestors().count());
    assert_eq!(folders.path, alpha);
}

#[test]
fn a_listing_stops_at_its_bound_and_says_so() {
    let (dir, workspace) = fixture();
    let crowded = dir.path().join("crowded");
    for index in 0..=FOLDERS_CAP {
        fs::create_dir_all(crowded.join(format!("{index:04}"))).unwrap();
    }

    let folders = listed(&workspace, &crowded, false);

    assert_eq!(folders.folders.len(), FOLDERS_CAP);
    assert!(folders.truncated);
    assert_eq!(folders.folders.last().unwrap().name, "0999");
    fs::remove_dir(crowded.join("0000")).unwrap();
    assert!(!listed(&workspace, &crowded, false).truncated);
}

#[test]
fn a_folder_that_is_not_there_or_is_a_file_is_refused_with_its_code() {
    let (dir, workspace) = fixture();
    let file = dir.path().join("notes.md");
    fs::write(&file, "x\n").unwrap();

    let code = |folder: &Path| {
        Folders::read(&workspace, folder, false, None)
            .unwrap_err()
            .code()
    };

    assert_eq!(code(&dir.path().join("wrok")), "browse_missing");
    assert_eq!(code(&file), "browse_not_a_folder");
}

#[cfg(unix)]
#[test]
fn a_folder_nobody_may_list_is_refused_with_its_code() {
    use std::os::unix::fs::PermissionsExt;

    let (dir, workspace) = fixture();
    let locked = dir.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    // Root lists it all the same, and says nothing of what a person meets.
    let listable = fs::read_dir(&locked).is_ok();

    let read = Folders::read(&workspace, &locked, false, None);

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    if listable {
        eprintln!(
            "skipped: {} can be listed whatever its mode",
            locked.display()
        );
        return;
    }
    assert_eq!(read.unwrap_err().code(), "browse_unlisted");
}

/// Linux alone, whose file systems take any bytes in a name, where macOS's
/// refuse one that is not UTF-8.
#[cfg(target_os = "linux")]
#[test]
fn a_name_that_is_not_utf8_is_said_with_its_code_and_not_listed() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt as _;

    let (dir, workspace) = fixture();
    let root = dir.path().join("work");
    fs::create_dir(root.join(OsStr::from_bytes(b"not-\xffutf8"))).unwrap();
    fs::create_dir(root.join(OsStr::from_bytes(b".\xffhidden"))).unwrap();

    let without = listed(&workspace, &root, false);
    let with = listed(&workspace, &root, true);

    assert_eq!(names(&without), ["alpha"]);
    let codes = |folders: &Folders| {
        folders
            .failures
            .iter()
            .map(|said| said.code)
            .collect::<Vec<_>>()
    };
    // A hidden one is said only once hidden folders are asked for.
    assert_eq!(codes(&without), ["browse_name_unfit"]);
    assert_eq!(codes(&with), ["browse_name_unfit", "browse_name_unfit"]);
    assert_eq!(without.hidden, 0);
}

#[cfg(unix)]
#[test]
fn a_link_whose_type_will_not_read_is_said_with_its_code_beside_the_rest() {
    use std::os::unix::fs::symlink;

    let (dir, workspace) = fixture();
    let root = dir.path().join("work");
    symlink(root.join("loop"), root.join("loop")).unwrap();

    let folders = listed(&workspace, &root, false);

    assert_eq!(names(&folders), ["alpha"]);
    assert_eq!(folders.failures.len(), 1, "{:?}", folders.failures);
    assert_eq!(folders.failures[0].code, "browse_entry_unread");
    assert!(
        folders.failures[0].message.contains("loop"),
        "{:?}",
        folders.failures
    );
}

#[cfg(unix)]
#[test]
fn a_hidden_link_whose_type_will_not_read_is_said_only_once_hidden_folders_are_asked_for() {
    use std::os::unix::fs::symlink;

    let (dir, workspace) = fixture();
    let root = dir.path().join("work");
    symlink(root.join(".loop"), root.join(".loop")).unwrap();

    let without = listed(&workspace, &root, false);
    let with = listed(&workspace, &root, true);

    assert!(without.failures.is_empty(), "{:?}", without.failures);
    let codes: Vec<&str> = with.failures.iter().map(|said| said.code).collect();
    assert_eq!(codes, ["browse_entry_unread"]);
    assert!(
        with.failures[0].message.contains(".loop"),
        "{:?}",
        with.failures
    );
    // Not known to be a folder, so counted as no hidden one either way.
    assert_eq!((without.hidden, with.hidden), (0, 0));
    assert_eq!(names(&without), ["alpha"]);
}

#[test]
fn the_home_is_named_only_as_a_folder_a_request_may_name() {
    let (dir, workspace) = fixture();
    let root = dir.path().join("work");
    let home = dir.path().join("home");

    let named = Folders::read(&workspace, &root, false, Some(&home)).unwrap();
    // As `HOME=/home/you/` hands it over, which normalizing spells as named.
    let trailing = format!("{}/", home.display());
    let unnormalized = Folders::read(&workspace, &root, false, Some(Path::new(&trailing))).unwrap();
    let relative = Folders::read(&workspace, &root, false, Some(Path::new("home"))).unwrap();
    let inside = Folders::read(&workspace, &root, false, Some(&root.join(WEAVER_DIR))).unwrap();

    assert_eq!(named.home.as_deref(), home.to_str());
    assert_eq!(unnormalized.home.as_deref(), home.to_str());
    assert_eq!(relative.home, None);
    assert_eq!(inside.home, None);
}

#[test]
fn a_listing_reads_and_writes_nothing() {
    let (dir, workspace) = fixture();
    let root = dir.path().join("work");
    fs::create_dir_all(root.join("beta").join(".git")).unwrap();
    let before = files(dir.path());

    listed(&workspace, &root, true);
    listed(&workspace, &root.join("beta"), true);

    assert_eq!(files(dir.path()), before);
    assert!(!root.join("beta").join(MIND_DIR).exists());
}

/// Every field by the name the panel reads it by, and each word as a
/// command line would take it.
#[test]
fn a_listing_is_serialized_field_by_field() {
    let folders = Folders {
        path: PathBuf::from("/work"),
        is: Is::Workspace,
        linked: false,
        trail: vec![
            Crumb {
                name: "/".to_owned(),
                path: PathBuf::from("/"),
            },
            Crumb {
                name: "work".to_owned(),
                path: PathBuf::from("/work"),
            },
        ],
        folders: vec![Folder {
            name: "alpha".to_owned(),
            path: PathBuf::from("/work/alpha"),
            is: Is::Project,
            linked: true,
            symlink: false,
        }],
        hidden: 1,
        truncated: false,
        failures: vec![Said::of(
            "browse_name_unfit",
            &BrowseFailure::NameUnfit {
                path: PathBuf::from("/work/x"),
            },
        )],
        home: Some("/home/you".to_owned()),
    };

    assert_eq!(
        serde_json::to_value(&folders).unwrap(),
        serde_json::json!({
            "path": "/work",
            "is": "workspace",
            "linked": false,
            "trail": [
                {"name": "/", "path": "/"},
                {"name": "work", "path": "/work"},
            ],
            "folders": [{
                "name": "alpha",
                "path": "/work/alpha",
                "is": "project",
                "linked": true,
                "symlink": false,
            }],
            "hidden": 1,
            "truncated": false,
            "failures": [{
                "code": "browse_name_unfit",
                "message": "/work/x has a name that is not UTF-8, which no request can send back, so it is not listed",
            }],
            "home": "/home/you",
        })
    );
    assert_eq!(
        serde_json::to_value(Is::ALL).unwrap(),
        serde_json::json!(["project", "workspace", "repository", "folder"])
    );
}
