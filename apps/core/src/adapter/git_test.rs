//! Unit tests for `adapter/git.rs`, kept beside it rather than inside it.
//!
//! The real `git` against repositories on the disk, made with `gix`, given a
//! home of its own (`SystemGit::in_home`): a first backup into an empty
//! repository, one that changes nothing, and a branch the remote does not
//! have yet, each read back by checking it out again. And a stand-in `git`
//! that writes down what it was run with, to hold the token to the one
//! place it may be, and to say what a refused login looks like.
//!
//! No `git` on the PATH skips the first three, said on stderr, except under
//! `$CI`, where it fails them: CI runs every check.

use super::*;

use std::fs;

use tempfile::TempDir;

use crate::mother::{random, GitRepositoryMother};

/// The real `git`, its home `home`, or `None` when there is none to run.
fn git(home: &Path) -> Option<SystemGit> {
    let found = Command::new("git").arg("--version").output().is_ok();
    if !found {
        assert!(
            std::env::var_os("CI").is_none(),
            "no git on the PATH, and CI must run every check here"
        );
        eprintln!("skipped: no git on the PATH");
        return None;
    }
    Some(SystemGit::default().in_home(home))
}

/// A bare repository with no commit, as a new repository on a forge is,
/// its HEAD naming `main` as GitHub's does: a clone of it is on the branch
/// the remote names, whatever this machine's git would default to.
fn empty_remote() -> TempDir {
    let dir = TempDir::new().unwrap();
    gix::init_bare(dir.path()).expect("make a bare repository");
    fs::write(dir.path().join("HEAD"), "ref: refs/heads/main\n").unwrap();
    dir
}

fn files(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name() == Some(OsStr::new(".git")) {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else {
                let route = path.strip_prefix(root).unwrap();
                found.push(route.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

#[test]
fn a_first_backup_into_an_empty_repository_is_its_first_commit_on_the_branch_it_names() {
    let home = TempDir::new().unwrap();
    let Some(git) = git(home.path()) else { return };
    let remote = empty_remote();
    let url = remote.path().to_string_lossy().into_owned();
    let work = TempDir::new().unwrap();
    let into = work.path().join("backup");
    let text = random::word();

    let checked = git.check_out(&url, None, &into, None).unwrap();
    fs::create_dir_all(into.join(".mindweaver")).unwrap();
    fs::write(into.join(".mindweaver/weaver.toml"), &text).unwrap();
    let published = git
        .publish(&into, &[".mindweaver"], "back up the library", None)
        .unwrap();

    assert_eq!(checked.branch, "main");
    assert!(matches!(published, Published::Pushed { .. }));
    let again = work.path().join("again");
    git.check_out(&url, None, &again, None).unwrap();
    assert_eq!(files(&again), [".mindweaver/weaver.toml"]);
    assert_eq!(
        fs::read_to_string(again.join(".mindweaver/weaver.toml")).unwrap(),
        text
    );
}

#[test]
fn a_backup_holding_what_the_branch_holds_sends_nothing() {
    let home = TempDir::new().unwrap();
    let Some(git) = git(home.path()) else { return };
    let remote = empty_remote();
    let url = remote.path().to_string_lossy().into_owned();
    let work = TempDir::new().unwrap();
    let first = work.path().join("first");
    git.check_out(&url, None, &first, None).unwrap();
    fs::create_dir_all(first.join(".mindweaver")).unwrap();
    fs::write(first.join(".mindweaver/weaver.toml"), "kept\n").unwrap();
    git.publish(&first, &[".mindweaver"], "first", None)
        .unwrap();
    let second = work.path().join("second");
    git.check_out(&url, None, &second, None).unwrap();

    let published = git
        .publish(&second, &[".mindweaver"], "second", None)
        .unwrap();

    assert_eq!(published, Published::Unchanged);
}

#[test]
fn a_branch_the_remote_has_not_got_is_started_from_the_one_it_names() {
    let home = TempDir::new().unwrap();
    let Some(git) = git(home.path()) else { return };
    let remote = GitRepositoryMother::create(&[("README.md", "the team's\n")]);
    let url = GitRepositoryMother::url(&remote);
    let work = TempDir::new().unwrap();
    let into = work.path().join("backup");

    let checked = git.check_out(&url, Some("library"), &into, None).unwrap();
    fs::create_dir_all(into.join(".mindweaver")).unwrap();
    fs::write(into.join(".mindweaver/weaver.toml"), "kept\n").unwrap();
    git.publish(&into, &[".mindweaver"], "back up", None)
        .unwrap();

    assert_eq!(checked.branch, "library");
    let again = work.path().join("again");
    git.check_out(&url, Some("library"), &again, None).unwrap();
    assert_eq!(files(&again), [".mindweaver/weaver.toml", "README.md"]);
}

#[test]
fn a_branch_asked_for_is_made_on_the_remote_even_when_nothing_changed() {
    let home = TempDir::new().unwrap();
    let Some(git) = git(home.path()) else { return };
    let remote = GitRepositoryMother::create(&[(".mindweaver/weaver.toml", "kept\n")]);
    let url = GitRepositoryMother::url(&remote);
    let work = TempDir::new().unwrap();
    let into = work.path().join("backup");
    git.check_out(&url, Some("library"), &into, None).unwrap();

    let published = git
        .publish(&into, &[".mindweaver"], "back up", None)
        .unwrap();

    assert!(matches!(published, Published::Pushed { .. }));
    let again = work.path().join("again");
    git.check_out(&url, Some("library"), &again, None).unwrap();
    let branch = git
        .publish(&again, &[".mindweaver"], "back up", None)
        .unwrap();
    assert_eq!(branch, Published::Unchanged, "the remote has it now");
}

/// A stand-in `git` that writes each run's arguments, and the variable the
/// token goes through, one line each into `log`, then says `stderr` and
/// exits with `status`.
#[cfg(unix)]
fn stand_in(dir: &Path, log: &Path, stderr: &str, status: i32) -> SystemGit {
    use std::os::unix::fs::PermissionsExt as _;
    let program = dir.join("git");
    let script = format!(
        "#!/bin/sh\n\
         printf 'args: %s\\n' \"$*\" >> '{log}'\n\
         printf 'token: %s\\n' \"${TOKEN_VARIABLE}\" >> '{log}'\n\
         printf '%s\\n' '{stderr}' >&2\n\
         exit {status}\n",
        log = log.display(),
    );
    fs::write(&program, script).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    SystemGit::at(program).in_home(dir)
}

#[cfg(unix)]
#[test]
fn a_token_reaches_git_through_its_environment_and_never_its_arguments() {
    let dir = TempDir::new().unwrap();
    let log = dir.path().join("log");
    let git = stand_in(dir.path(), &log, "fatal: repository not found", 128);
    let token = Token::new(random::hex(40));

    let refused = git
        .check_out(
            "https://example.com/you/backup.git",
            None,
            &dir.path().join("backup"),
            Some(&token),
        )
        .unwrap_err();

    assert_eq!(refused.code(), "backup_check_out_failed");
    let logged = fs::read_to_string(&log).unwrap();
    let args = logged
        .lines()
        .find(|line| line.starts_with("args:"))
        .unwrap();
    assert!(!args.contains(token.reveal()), "{args}");
    assert!(args.contains("credential.helper="), "{args}");
    assert!(logged.contains(&format!("token: {}", token.reveal())));
    assert_eq!(format!("{token:?}"), "Token(..)");
}

#[cfg(unix)]
#[test]
fn a_login_nobody_could_answer_is_said_as_credentials_needed_without_the_url_s_credentials() {
    let dir = TempDir::new().unwrap();
    let log = dir.path().join("log");
    let git = stand_in(
        dir.path(),
        &log,
        "fatal: could not read Username for 'https://example.com': terminal prompts disabled",
        128,
    );

    let refused = git
        .check_out(
            "https://example.com/you/backup.git",
            None,
            &dir.path().join("backup"),
            None,
        )
        .unwrap_err();

    assert_eq!(refused.code(), "backup_credentials_needed");
    let args = fs::read_to_string(&log).unwrap();
    assert!(!args.contains("credential.helper="), "{args}");
}

#[test]
fn no_git_to_run_is_said_as_that() {
    let dir = TempDir::new().unwrap();
    let git = SystemGit::at(dir.path().join("no-such-git")).in_home(dir.path());

    let refused = git
        .check_out("/srv/backup.git", None, &dir.path().join("backup"), None)
        .unwrap_err();

    assert_eq!(refused.code(), "backup_git_unavailable");
}
