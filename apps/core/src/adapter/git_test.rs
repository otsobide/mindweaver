//! Unit tests for `adapter/git.rs`, kept beside it rather than inside it.
//!
//! The real `git` against repositories on the disk, made with `gix`, given a
//! home of its own (`SystemGit::in_home`): a first backup into an empty
//! repository, one that changes nothing, and a branch the remote does not
//! have yet, each read back by checking it out again. And a stand-in `git`
//! that writes down what it was run with, to hold the token to the one
//! place it may be, to say what a refused login looks like, and to be
//! stopped by a cancel while it runs rather than waited for.
//!
//! No `git` on the PATH skips the first three, said on stderr, except under
//! `$CI`, where it fails them: CI runs every check.

use super::*;

use std::fs;
use std::time::{Duration, Instant};

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

/// What a backup does once its export is written: commit what
/// `.mindweaver` holds, and push what that left ready.
fn publish(git: &SystemGit, work: &Path, message: &str) -> Commit {
    let commit = git
        .commit(work, &[".mindweaver"], message, &Watch::new())
        .unwrap();
    if let Commit::Ready { .. } = commit {
        git.push(work, None).unwrap();
    }
    commit
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

    let checked = git
        .check_out(&url, None, &into, None, &Watch::new())
        .unwrap();
    fs::create_dir_all(into.join(".mindweaver")).unwrap();
    fs::write(into.join(".mindweaver/weaver.toml"), &text).unwrap();
    let published = publish(&git, &into, "back up the library");

    assert_eq!(checked.branch, "main");
    assert!(matches!(published, Commit::Ready { .. }));
    let again = work.path().join("again");
    git.check_out(&url, None, &again, None, &Watch::new())
        .unwrap();
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
    git.check_out(&url, None, &first, None, &Watch::new())
        .unwrap();
    fs::create_dir_all(first.join(".mindweaver")).unwrap();
    fs::write(first.join(".mindweaver/weaver.toml"), "kept\n").unwrap();
    publish(&git, &first, "first");
    let second = work.path().join("second");
    git.check_out(&url, None, &second, None, &Watch::new())
        .unwrap();

    let published = publish(&git, &second, "second");

    assert_eq!(published, Commit::Nothing);
}

#[test]
fn a_branch_the_remote_has_not_got_is_started_from_the_one_it_names() {
    let home = TempDir::new().unwrap();
    let Some(git) = git(home.path()) else { return };
    let remote = GitRepositoryMother::create(&[("README.md", "the team's\n")]);
    let url = GitRepositoryMother::url(&remote);
    let work = TempDir::new().unwrap();
    let into = work.path().join("backup");

    let checked = git
        .check_out(&url, Some("library"), &into, None, &Watch::new())
        .unwrap();
    fs::create_dir_all(into.join(".mindweaver")).unwrap();
    fs::write(into.join(".mindweaver/weaver.toml"), "kept\n").unwrap();
    publish(&git, &into, "back up");

    assert_eq!(checked.branch, "library");
    let again = work.path().join("again");
    git.check_out(&url, Some("library"), &again, None, &Watch::new())
        .unwrap();
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
    git.check_out(&url, Some("library"), &into, None, &Watch::new())
        .unwrap();

    let published = publish(&git, &into, "back up");

    assert!(matches!(published, Commit::Ready { .. }));
    let again = work.path().join("again");
    git.check_out(&url, Some("library"), &again, None, &Watch::new())
        .unwrap();
    let branch = publish(&git, &again, "back up");
    assert_eq!(branch, Commit::Nothing, "the remote has it now");
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
            &Watch::new(),
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
            &Watch::new(),
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
        .check_out(
            "/srv/backup.git",
            None,
            &dir.path().join("backup"),
            None,
            &Watch::new(),
        )
        .unwrap_err();

    assert_eq!(refused.code(), "backup_git_unavailable");
}

/// A stand-in `git` that writes `started` into `log` and then takes
/// `seconds`, as a clone from a server that is slow to answer does.
#[cfg(unix)]
fn slow(dir: &Path, log: &Path, seconds: u32) -> SystemGit {
    use std::os::unix::fs::PermissionsExt as _;
    let program = dir.join("git");
    let script = format!(
        "#!/bin/sh\n\
         echo started >> '{log}'\n\
         exec sleep {seconds}\n",
        log = log.display(),
    );
    fs::write(&program, script).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    SystemGit::at(program).in_home(dir)
}

#[cfg(unix)]
#[test]
fn a_cancel_stops_the_git_under_way_rather_than_waiting_for_it() {
    let dir = TempDir::new().unwrap();
    let log = dir.path().join("log");
    let git = slow(dir.path(), &log, 60);
    let watch = Watch::new();
    let cancel = watch.clone();
    let started = log.clone();
    let canceller = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(30);
        while !started.exists() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        cancel.cancel();
    });
    let began = Instant::now();

    let refused = git
        .check_out(
            "/srv/backup.git",
            None,
            &dir.path().join("backup"),
            None,
            &watch,
        )
        .unwrap_err();

    canceller.join().unwrap();
    assert!(matches!(refused, UpstreamError::Cancelled), "{refused}");
    assert_eq!(refused.code(), "backup_git_cancelled");
    assert!(
        began.elapsed() < Duration::from_secs(20),
        "waited {:?} for a git a cancel should have stopped",
        began.elapsed()
    );
    assert_eq!(fs::read_to_string(&log).unwrap(), "started\n");
}

#[cfg(unix)]
#[test]
fn a_cancelled_watch_starts_no_git_at_all() {
    let dir = TempDir::new().unwrap();
    let log = dir.path().join("log");
    let git = slow(dir.path(), &log, 60);
    let watch = Watch::new();
    watch.cancel();
    let work = dir.path().join("work");
    fs::create_dir_all(&work).unwrap();

    let checked = git
        .check_out(
            "/srv/backup.git",
            Some("library"),
            &dir.path().join("backup"),
            None,
            &watch,
        )
        .unwrap_err();
    let committed = git
        .commit(&work, &[".mindweaver"], "back up", &watch)
        .unwrap_err();

    assert_eq!(checked.code(), "backup_git_cancelled");
    assert_eq!(committed.code(), "backup_git_cancelled");
    assert!(!log.exists(), "no git was run");
}
