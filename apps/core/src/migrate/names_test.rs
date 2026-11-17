//! Unit tests for `migrate/names.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn a_route_inside_the_old_folder_moves_with_it_and_no_other() {
    assert_eq!(
        moved_route(Path::new(".mindflayer/library/skills")),
        Some(PathBuf::from(".mindweaver/library/skills"))
    );
    assert_eq!(
        moved_route(Path::new("./.mindflayer/library/skills")),
        Some(PathBuf::from(".mindweaver/library/skills"))
    );
    assert_eq!(
        moved_route(Path::new("rules/../.mindflayer/library/rules")),
        Some(PathBuf::from(".mindweaver/library/rules"))
    );
    assert_eq!(moved_route(Path::new("skills")), None);
    assert_eq!(moved_route(Path::new(".mindflayer/../skills")), None);
    assert_eq!(moved_route(Path::new("a/.mindflayer/skills")), None);
    assert_eq!(moved_route(Path::new(".mindflayerish/skills")), None);
}

#[test]
fn only_what_is_named_after_the_product_is_renamed_in_the_folder() {
    let dir = tempfile::TempDir::new().unwrap();
    for name in [
        "flayer.toml",
        "flayer.toml.v1",
        "flayer.toml.12345-abc.tmp",
        "mindflayer.db",
        "mindflayer.db.v2.bak",
        "mindflayer.db.v2.bak.tmp",
        "lock",
    ] {
        fs::write(dir.path().join(name), "").unwrap();
    }

    let renamed = renamed_inside(dir.path()).unwrap();

    assert_eq!(
        renamed,
        [
            (String::from("flayer.toml"), String::from("weaver.toml")),
            (
                String::from("flayer.toml.v1"),
                String::from("weaver.toml.v1")
            ),
            (String::from("mindflayer.db"), String::from("mindweaver.db")),
            (
                String::from("mindflayer.db.v2.bak"),
                String::from("mindweaver.db.v2.bak")
            ),
        ]
    );
}

/// A workspace's `.mindflayer` as a move finds it: its marker, a file in
/// the library and the lock, taken and held, as the move holds it.
fn old_folder(root: &Path) -> (PathBuf, crate::lock::WorkspaceLock) {
    let old = root.join(".mindflayer");
    fs::create_dir_all(old.join("library").join("skills").join("review")).unwrap();
    fs::write(old.join("weaver.toml"), "version = 2\n").unwrap();
    fs::write(
        old.join("library")
            .join("skills")
            .join("review")
            .join("SKILL.md"),
        "# Review\n",
    )
    .unwrap();
    let lock = crate::lock::WorkspaceLock::acquire(&old, crate::lock::WAIT).unwrap();
    (old, lock)
}

/// Every file under `root`, by its route from it, with what it holds: the
/// lock's file by its name alone, which Windows lets no other handle read
/// while it is held.
fn held(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                let route = path.strip_prefix(root).unwrap().to_path_buf();
                let bytes = if route == Path::new(crate::lock::LOCK_FILE) {
                    Vec::new()
                } else {
                    fs::read(&path).unwrap()
                };
                found.push((route, bytes));
            }
        }
    }
    found.sort();
    found
}

#[test]
fn a_folder_moved_entry_by_entry_holds_what_it_did_and_the_lock_goes_with_it() {
    let dir = tempfile::TempDir::new().unwrap();
    let (old, mut lock) = old_folder(dir.path());
    let before = held(&old);
    let new = dir.path().join(".mindweaver");

    entry_by_entry(&old, &new, &mut Done::default()).unwrap();
    lock.moved(&new);

    assert!(!old.exists());
    assert_eq!(held(&new), before);
    // Let go where it went: its holder's id is taken away there.
    drop(lock);
    assert!(new.join(crate::lock::LOCK_FILE).is_file());
    assert!(!new.join(crate::lock::HOLDER_FILE).exists());
}

#[test]
fn a_folder_moved_entry_by_entry_is_put_back_whole() {
    let dir = tempfile::TempDir::new().unwrap();
    let (old, lock) = old_folder(dir.path());
    let before = held(&old);
    let new = dir.path().join(".mindweaver");
    let (_elsewhere, workspace) = {
        let elsewhere = tempfile::TempDir::new().unwrap();
        let (workspace, _) = WeaverWorkspace::init(elsewhere.path()).unwrap();
        (elsewhere, workspace)
    };
    let mut done = Done::default();
    entry_by_entry(&old, &new, &mut done).unwrap();

    let stranded = done.put_back(&workspace);

    assert!(stranded.is_empty(), "{stranded:?}");
    assert!(!new.exists());
    assert_eq!(held(&old), before);
    drop(lock);
}

#[test]
fn a_folder_is_never_moved_entry_by_entry_onto_one_there_by_now() {
    let dir = tempfile::TempDir::new().unwrap();
    let (old, lock) = old_folder(dir.path());
    let before = held(&old);
    let new = dir.path().join(".mindweaver");
    fs::create_dir(&new).unwrap();
    let mut done = Done::default();

    let failure = entry_by_entry(&old, &new, &mut done).unwrap_err();

    assert_eq!(failure.code(), "migrate_rename_failed");
    assert!(done.0.is_empty());
    assert_eq!(held(&old), before);
    assert_eq!(held(&new), []);
    drop(lock);
}
