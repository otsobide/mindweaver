//! Unit tests for `copy.rs`, kept beside it rather than inside it.

use super::*;
use tempfile::TempDir;

/// A folder holding a skill to copy, and somewhere to copy it to.
fn fixture() -> (TempDir, PathBuf, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let from = dir.path().join("source").join("deploy");
    fs::create_dir_all(from.join("scripts")).unwrap();
    fs::write(from.join("SKILL.md"), "first").unwrap();
    fs::write(from.join("scripts").join("run.sh"), "echo hi\n").unwrap();
    let to = dir.path().join("project").join("skills").join("deploy");
    let scratch = dir.path().join("project").join(".mind").join("tmp");
    (dir, from, to, scratch)
}

/// The names directly inside `dir`, sorted.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The stages in a scratch folder, which is everything but its own two files.
fn stages(scratch: &Path) -> Vec<String> {
    names(scratch)
        .into_iter()
        .filter(|name| name != ".gitignore" && name != LOCK)
        .collect()
}

/// What is left out is never copied, only at the top, and a copy that holds
/// all the rest is unchanged by the next replace.
#[test]
fn replacing_leaving_names_out_copies_the_rest_and_compares_without_them() {
    let (_dir, from, to, _) = fixture();
    fs::create_dir_all(from.join(".claude-plugin")).unwrap();
    fs::write(from.join(".claude-plugin").join("plugin.json"), "{}").unwrap();
    fs::create_dir_all(from.join("scripts").join(".claude-plugin")).unwrap();
    fs::write(from.join("scripts/.claude-plugin/example.json"), "{}").unwrap();
    let leave_out = [".claude-plugin", ".git"];

    let first = replace_leaving(&from, &to, &from, &leave_out).unwrap();
    let again = replace_leaving(&from, &to, &from, &leave_out).unwrap();
    fs::write(from.join("SKILL.md"), "second").unwrap();
    let changed = replace_leaving(&from, &to, &from, &leave_out).unwrap();

    assert_eq!(
        (first, again, changed),
        (Change::Added, Change::Unchanged, Change::Updated)
    );
    assert_eq!(names(&to), ["SKILL.md", "scripts"]);
    assert!(to.join("scripts/.claude-plugin/example.json").is_file());
    assert_eq!(names(to.parent().unwrap()), ["deploy"], "nothing beside it");
}

#[test]
fn a_copy_compared_with_where_it_would_go_says_what_replacing_it_would_be() {
    let (dir, from, to, _) = fixture();
    let copy = dir.path().join("copy");
    tree(&from, &copy, &from).unwrap();

    let nothing_there = compared(&copy, &to).unwrap();
    tree(&from, &to, &from).unwrap();
    let the_same = compared(&copy, &to).unwrap();
    fs::write(to.join("SKILL.md"), "edited").unwrap();
    let different = compared(&copy, &to).unwrap();

    assert_eq!(
        (nothing_there, the_same, different),
        (Change::Added, Change::Unchanged, Change::Updated)
    );
    // Nothing is replaced by asking.
    assert_eq!(fs::read_to_string(to.join("SKILL.md")).unwrap(), "edited");
}

#[test]
fn a_scratch_folder_is_made_when_first_needed_and_ignores_itself() {
    let (_dir, _, _, scratch) = fixture();
    assert!(!scratch.exists());

    let stage = Stage::open(&scratch).unwrap();

    assert!(stage.path().starts_with(&scratch) && stage.path().is_dir());
    let ignore = fs::read_to_string(scratch.join(".gitignore")).unwrap();
    assert!(ignore.lines().any(|line| line == "*"), "{ignore}");
}

#[test]
fn a_gitignore_somebody_changed_is_left_as_it_is() {
    let (_dir, _, _, scratch) = fixture();
    fs::create_dir_all(&scratch).unwrap();
    fs::write(scratch.join(".gitignore"), "mine\n").unwrap();

    Stage::open(&scratch).unwrap();

    assert_eq!(
        fs::read_to_string(scratch.join(".gitignore")).unwrap(),
        "mine\n"
    );
}

#[test]
fn a_stage_is_named_after_its_process_and_something_random() {
    let (_dir, _, _, scratch) = fixture();

    let stage = Stage::open(&scratch).unwrap();

    let name = stage.path().file_name().unwrap().to_str().unwrap();
    let (pid, suffix) = name.split_once('-').unwrap();
    assert_eq!(pid, std::process::id().to_string());
    assert_eq!(suffix.len(), SUFFIX_LEN, "{name}");
    assert!(is_stage_name(name), "{name}");
}

#[test]
fn two_stages_open_at_once_never_share_a_folder() {
    let (_dir, from, to, scratch) = fixture();
    let mut first = Stage::open(&scratch).unwrap();
    fs::write(first.path().join("mine"), "first").unwrap();

    let mut second = Stage::open(&scratch).unwrap();

    assert_ne!(first.path(), second.path());
    // The first is in use, so the second must not have swept it.
    assert!(second.swept().is_empty(), "{:?}", second.swept());
    assert_eq!(
        fs::read_to_string(first.path().join("mine")).unwrap(),
        "first"
    );
    // Both build the same thing, each in its own folder.
    assert_eq!(first.replace(&from, &to, &from).unwrap(), Change::Added);
    assert_eq!(
        second.replace(&from, &to, &from).unwrap(),
        Change::Unchanged
    );
}

#[test]
fn a_dropped_stage_takes_its_folder_with_it() {
    let (_dir, _, _, scratch) = fixture();
    let stage = Stage::open(&scratch).unwrap();
    let path = stage.path().to_path_buf();

    drop(stage);

    assert!(!path.exists());
    assert!(stages(&scratch).is_empty(), "{:?}", stages(&scratch));
}

#[test]
fn what_an_interrupted_run_left_is_swept_by_the_next_and_said() {
    let (_dir, _, _, scratch) = fixture();
    // Named as a stage is, by a process that is not running any more, with
    // half a copy in it.
    let left = scratch.join("4242-00000000deadbeef");
    fs::create_dir_all(left.join("new").join("deploy")).unwrap();
    fs::write(left.join("new").join("deploy").join("SKILL.md"), "half").unwrap();

    let stage = Stage::open(&scratch).unwrap();

    assert_eq!(stage.swept(), std::slice::from_ref(&left));
    assert!(!left.exists());
}

#[test]
fn nothing_is_swept_while_another_run_holds_a_stage() {
    let (_dir, _, _, scratch) = fixture();
    let running = Stage::open(&scratch).unwrap();
    let left = scratch.join("4242-00000000deadbeef");
    fs::create_dir_all(&left).unwrap();

    // Whether that one is a leftover or another run's cannot be told while
    // a run is going, so it waits for a run that finds itself alone.
    let meanwhile = Stage::open(&scratch).unwrap();
    assert!(meanwhile.swept().is_empty(), "{:?}", meanwhile.swept());
    assert!(left.is_dir() && running.path().is_dir());

    drop((running, meanwhile));
    let alone = Stage::open(&scratch).unwrap();
    assert_eq!(alone.swept(), [left]);
}

#[test]
fn only_what_is_named_as_a_stage_is_swept() {
    let (_dir, _, _, scratch) = fixture();
    fs::create_dir_all(scratch.join("notes")).unwrap();
    fs::create_dir_all(scratch.join("4242-short")).unwrap();
    fs::write(scratch.join("4242-00000000deadbeef"), "a file").unwrap();

    let stage = Stage::open(&scratch).unwrap();

    assert!(stage.swept().is_empty(), "{:?}", stage.swept());
    assert!(scratch.join("notes").is_dir());
    assert!(scratch.join("4242-short").is_dir());
    assert!(scratch.join("4242-00000000deadbeef").is_file());
}

#[test]
fn a_stage_name_is_digits_a_dash_and_sixteen_hex_digits() {
    assert!(is_stage_name("1-0123456789abcdef"));
    assert!(!is_stage_name("-0123456789abcdef"));
    assert!(!is_stage_name("1-0123456789abcde"));
    assert!(!is_stage_name("1-0123456789abcdeg"));
    assert!(!is_stage_name("x1-0123456789abcdef"));
    assert!(!is_stage_name(".gitignore"));
    assert!(!is_stage_name(LOCK));
}

#[test]
fn replacing_through_a_stage_leaves_nothing_beside_the_target() {
    let (_dir, from, to, scratch) = fixture();
    let skills = to.parent().unwrap().to_path_buf();
    let mut stage = Stage::open(&scratch).unwrap();

    assert_eq!(stage.replace(&from, &to, &from).unwrap(), Change::Added);
    assert_eq!(names(&skills), ["deploy"]);

    fs::write(from.join("SKILL.md"), "second").unwrap();
    assert_eq!(stage.replace(&from, &to, &from).unwrap(), Change::Updated);
    assert_eq!(names(&skills), ["deploy"]);
    assert_eq!(fs::read_to_string(to.join("SKILL.md")).unwrap(), "second");

    assert_eq!(stage.replace(&from, &to, &from).unwrap(), Change::Unchanged);
    // What was built and what was moved aside went with each replacement.
    assert!(names(stage.path()).is_empty(), "{:?}", names(stage.path()));
}

#[test]
fn replacing_makes_the_folder_the_target_goes_in() {
    let (_dir, from, to, scratch) = fixture();
    assert!(!to.parent().unwrap().exists());

    let change = Stage::open(&scratch)
        .unwrap()
        .replace(&from, &to, &from)
        .unwrap();

    assert_eq!(change, Change::Added);
    assert!(to.join("scripts").join("run.sh").is_file());
}

#[cfg(unix)]
#[test]
fn a_copy_that_fails_leaves_the_target_and_its_folder_as_they_were() {
    let (dir, from, to, scratch) = fixture();
    let mut stage = Stage::open(&scratch).unwrap();
    stage.replace(&from, &to, &from).unwrap();
    fs::write(dir.path().join("private.env"), "SECRET").unwrap();
    std::os::unix::fs::symlink(dir.path().join("private.env"), from.join("aaa.env")).unwrap();
    fs::write(from.join("SKILL.md"), "second").unwrap();

    let error = stage.replace(&from, &to, &from).unwrap_err();

    assert!(error.to_string().contains("outside"), "{error}");
    assert_eq!(fs::read_to_string(to.join("SKILL.md")).unwrap(), "first");
    assert_eq!(names(to.parent().unwrap()), ["deploy"]);
    assert!(names(stage.path()).is_empty(), "{:?}", names(stage.path()));
}

/// Where a project's two agents' folders would hold `deploy`.
fn two_targets(dir: &TempDir) -> Vec<PathBuf> {
    let project = dir.path().join("project");
    vec![
        project.join(".agents").join("skills").join("deploy"),
        project.join(".claude").join("skills").join("deploy"),
    ]
}

#[test]
fn placing_puts_one_copy_at_every_target_and_nothing_beside_any() {
    let (dir, from, _, scratch) = fixture();
    let targets = two_targets(&dir);
    let mut stage = Stage::open(&scratch).unwrap();

    let changes = stage.place(&from, &targets, &from, &[]).unwrap();

    assert_eq!(changes, [Change::Added, Change::Added]);
    for target in &targets {
        assert!(same(&from, target).unwrap(), "{}", target.display());
        assert_eq!(names(target.parent().unwrap()), ["deploy"]);
    }
    assert!(names(stage.path()).is_empty(), "{:?}", names(stage.path()));
}

#[test]
fn placing_again_replaces_only_the_targets_that_differ() {
    let (dir, from, _, scratch) = fixture();
    let targets = two_targets(&dir);
    let mut stage = Stage::open(&scratch).unwrap();
    stage.place(&from, &targets, &from, &[]).unwrap();
    fs::write(targets[1].join("SKILL.md"), "edited").unwrap();

    let changes = stage.place(&from, &targets, &from, &[]).unwrap();

    assert_eq!(changes, [Change::Unchanged, Change::Updated]);
    assert!(same(&targets[0], &targets[1]).unwrap());
    fs::write(from.join("SKILL.md"), "second").unwrap();
    let changes = stage.place(&from, &targets, &from, &[]).unwrap();
    assert_eq!(changes, [Change::Updated, Change::Updated]);
    assert_eq!(
        fs::read_to_string(targets[0].join("SKILL.md")).unwrap(),
        "second"
    );
    assert!(names(stage.path()).is_empty(), "{:?}", names(stage.path()));
}

#[test]
fn what_is_left_out_at_the_top_is_in_no_copy_and_compares_as_absent() {
    let (dir, from, _, scratch) = fixture();
    let targets = two_targets(&dir);
    fs::create_dir_all(from.join(".claude-plugin")).unwrap();
    fs::write(from.join(".claude-plugin").join("plugin.json"), "{}").unwrap();
    // Further down, the same name is the skill's own.
    fs::create_dir_all(from.join("scripts").join(".claude-plugin")).unwrap();
    let mut stage = Stage::open(&scratch).unwrap();

    let changes = stage
        .place(&from, &targets, &from, &[".claude-plugin"])
        .unwrap();

    assert_eq!(changes, [Change::Added, Change::Added]);
    for target in &targets {
        assert!(!target.join(".claude-plugin").exists());
        assert!(target.join("scripts").join(".claude-plugin").is_dir());
        assert!(same_leaving(&from, target, &[".claude-plugin"]).unwrap());
        assert!(!same(&from, target).unwrap());
    }
    let again = stage
        .place(&from, &targets, &from, &[".claude-plugin"])
        .unwrap();
    assert_eq!(again, [Change::Unchanged, Change::Unchanged]);
    // A target holding the name is not what leaving it out copies.
    fs::create_dir_all(targets[0].join(".claude-plugin")).unwrap();
    assert!(!same_leaving(&from, &targets[0], &[".claude-plugin"]).unwrap());
}

#[cfg(unix)]
#[test]
fn what_is_left_out_is_never_read_so_a_link_in_it_refuses_nothing() {
    let (dir, from, _, scratch) = fixture();
    let targets = two_targets(&dir);
    fs::write(dir.path().join("private.env"), "SECRET").unwrap();
    fs::create_dir_all(from.join(".claude-plugin")).unwrap();
    std::os::unix::fs::symlink(
        dir.path().join("private.env"),
        from.join(".claude-plugin").join("hooks.json"),
    )
    .unwrap();

    let changes = Stage::open(&scratch)
        .unwrap()
        .place(&from, &targets, &from, &[".claude-plugin"])
        .unwrap();

    assert_eq!(changes, [Change::Added, Change::Added]);
}

#[test]
fn a_target_that_cannot_be_put_in_place_leaves_every_target_as_it_was() {
    let (dir, from, _, scratch) = fixture();
    let mut targets = two_targets(&dir);
    let mut stage = Stage::open(&scratch).unwrap();
    stage
        .place(&from, &targets[..1], &from, &[])
        .expect("the first target holds the first version");
    // A folder no copy can go in, since a file has its name.
    let blocked = dir.path().join("project").join("blocked");
    fs::write(&blocked, "a file").unwrap();
    targets.push(blocked.join("deploy"));
    fs::write(from.join("SKILL.md"), "second").unwrap();

    let error = stage.place(&from, &targets, &from, &[]).unwrap_err();

    assert_eq!(error.target.as_ref(), Some(&targets[2]));
    // Put back, the one it replaced and the one it added alike.
    assert_eq!(
        fs::read_to_string(targets[0].join("SKILL.md")).unwrap(),
        "first"
    );
    assert_eq!(names(targets[0].parent().unwrap()), ["deploy"]);
    assert!(!targets[1].exists());
    assert_eq!(fs::read_to_string(&blocked).unwrap(), "a file");
    drop(stage);
    assert!(stages(&scratch).is_empty(), "{:?}", stages(&scratch));
}

#[test]
fn a_rename_that_fails_says_what_it_was_moving_where() {
    let dir = TempDir::new().unwrap();
    let from = dir.path().join("missing");
    let to = dir.path().join("there");

    let error = moved(&from, &to).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    let message = error.to_string();
    assert!(
        message.contains(&from.display().to_string())
            && message.contains(&to.display().to_string()),
        "{message}"
    );
}

#[test]
fn replacing_beside_still_leaves_nothing_behind() {
    // Gathering's way, onto the shelf: staged beside, and cleared after.
    let (_dir, from, to, _) = fixture();
    let skills = to.parent().unwrap().to_path_buf();

    assert_eq!(replace(&from, &to, &from).unwrap(), Change::Added);
    fs::write(from.join("SKILL.md"), "second").unwrap();
    assert_eq!(replace(&from, &to, &from).unwrap(), Change::Updated);

    assert_eq!(names(&skills), ["deploy"]);
}

#[cfg(unix)]
#[test]
fn a_link_is_followed_to_where_it_leads_and_judged_there() {
    let (dir, from, _, _) = fixture();
    let bounds = [fs::canonicalize(&from).unwrap()];
    fs::write(dir.path().join("private.env"), "SECRET").unwrap();
    std::os::unix::fs::symlink(dir.path().join("private.env"), from.join("out.env")).unwrap();
    std::os::unix::fs::symlink(from.join("SKILL.md"), from.join("in.md")).unwrap();

    let (outside, inside) = followed(&from.join("out.env"), &bounds).unwrap();
    assert_eq!(
        outside,
        fs::canonicalize(dir.path().join("private.env")).unwrap()
    );
    assert!(!inside);
    assert_eq!(
        followed(&from.join("in.md"), &bounds).unwrap(),
        (fs::canonicalize(from.join("SKILL.md")).unwrap(), true)
    );
}

#[test]
fn a_neighbour_whose_name_starts_with_a_bound_is_not_inside_it() {
    let (dir, from, _, _) = fixture();
    let neighbour = dir.path().join("source").join("deploy-old");
    fs::create_dir_all(&neighbour).unwrap();
    fs::write(neighbour.join("SKILL.md"), "older").unwrap();

    let (_, inside) = followed(
        &neighbour.join("SKILL.md"),
        &[fs::canonicalize(&from).unwrap()],
    )
    .unwrap();

    assert!(!inside);
}

#[test]
fn nothing_there_is_no_answer_but_an_error() {
    let (_dir, from, _, _) = fixture();

    let error = followed(&from.join("missing.md"), std::slice::from_ref(&from)).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}
