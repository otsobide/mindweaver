//! Unit tests for `backup.rs`, kept beside it rather than inside it.
//!
//! Through a stand-in [`Upstream`] whose remote is a folder on the disk: a
//! check-out copies it, a commit says whether the check-out's workspace
//! folder holds something else, and a push puts it back. What a backup
//! writes, what it sends and when it sends nothing, what it refuses before
//! asking the upstream anything, the steps it says, a cancel honoured until
//! the push and too late after, and the check-out taken away whatever came
//! of it.

use super::*;

use std::sync::{Arc, Mutex};

use tempfile::TempDir;

use crate::copy;
use crate::mother::{random, McpMother, SkillMother, WorkspaceMother};
use crate::port::CheckedOut;
use crate::value::Revision;

/// A remote that is a folder: what its branch holds.
#[derive(Debug)]
struct Folder {
    remote: TempDir,
    /// Each call, as `check out <url> <branch>`, `commit <paths> <message>`
    /// or `push`.
    calls: Mutex<Vec<String>>,
    /// The paths the last commit found changed, for the push to put back.
    ready: Mutex<Vec<String>>,
    /// The call during which somebody cancels the watch, as one pressing
    /// a key while `git` runs: `check out` or `commit`.
    cancelled_during: Option<&'static str>,
}

impl Folder {
    fn new() -> Self {
        Self {
            remote: TempDir::new().unwrap(),
            calls: Mutex::new(Vec::new()),
            ready: Mutex::new(Vec::new()),
            cancelled_during: None,
        }
    }

    /// One whose watch is cancelled while `call` runs.
    fn cancelled_during(call: &'static str) -> Self {
        Self {
            cancelled_during: Some(call),
            ..Self::new()
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn holds(&self, route: &str) -> bool {
        self.remote.path().join(route).exists()
    }

    /// Note `call`, and stop as `git` would be stopped when the watch is
    /// cancelled by then.
    fn called(&self, call: String, watch: &Watch) -> Result<(), UpstreamError> {
        let cancels = self
            .cancelled_during
            .is_some_and(|during| call.starts_with(during));
        self.calls.lock().unwrap().push(call);
        if cancels {
            watch.cancel();
        }
        if watch.is_cancelled() {
            return Err(UpstreamError::Cancelled);
        }
        Ok(())
    }
}

impl Upstream for Folder {
    fn check_out(
        &self,
        url: &str,
        branch: Option<&str>,
        into: &Path,
        _token: Option<&Token>,
        watch: &Watch,
    ) -> Result<CheckedOut, UpstreamError> {
        assert!(!into.exists(), "a check-out is made where nothing is");
        copy::tree(self.remote.path(), into, self.remote.path()).unwrap();
        self.called(format!("check out {url} {}", branch.unwrap_or("-")), watch)?;
        Ok(CheckedOut {
            branch: branch.unwrap_or("main").to_owned(),
        })
    }

    fn commit(
        &self,
        work: &Path,
        paths: &[&str],
        message: &str,
        watch: &Watch,
    ) -> Result<Commit, UpstreamError> {
        self.called(format!("commit {} {message}", paths.join(",")), watch)?;
        let changed: Vec<String> = paths
            .iter()
            .filter(|path| {
                let (from, to) = (work.join(path), self.remote.path().join(path));
                !to.exists() || !copy::same(&from, &to).unwrap()
            })
            .map(|path| (*path).to_owned())
            .collect();
        let ready = !changed.is_empty();
        *self.ready.lock().unwrap() = changed;
        Ok(if ready {
            Commit::Ready {
                revision: Revision::from(random::hex(40)),
            }
        } else {
            Commit::Nothing
        })
    }

    fn push(&self, work: &Path, _token: Option<&Token>) -> Result<(), UpstreamError> {
        self.calls.lock().unwrap().push(String::from("push"));
        for path in self.ready.lock().unwrap().drain(..) {
            let (from, to) = (work.join(&path), self.remote.path().join(&path));
            copy::replace(&from, &to, &from).unwrap();
        }
        Ok(())
    }
}

/// A watch that says each step to the list it hands back, and cancels
/// itself when it is told `cancel_at`.
fn followed(cancel_at: Option<Progress>) -> (Watch, Arc<Mutex<Vec<Progress>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let watch = Watch::new();
    let cancel = watch.clone();
    let watch = watch.reporting_to({
        let seen = Arc::clone(&seen);
        move |progress| {
            seen.lock().unwrap().push(progress);
            if Some(progress) == cancel_at {
                cancel.cancel();
            }
        }
    });
    (watch, seen)
}

/// A skill `name` in `workspace`'s library.
fn skill(workspace: &WeaverWorkspace, name: &str) -> PathBuf {
    let folder = workspace
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join(name);
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("SKILL.md"),
        SkillMother::create(name, &SkillMother::description()),
    )
    .unwrap();
    folder
}

fn asked(url: &str) -> Request<'_> {
    Request {
        url,
        branch: None,
        kinds: &[],
        message: None,
        token: None,
        // A watch of its own, never shared: a stand-in cancels the one it is
        // handed, and one test's cancel is no other's.
        watch: Box::leak(Box::new(Watch::new())),
    }
}

/// What the workspace's scratch folder holds.
fn scratch(workspace: &WeaverWorkspace) -> Vec<PathBuf> {
    match fs::read_dir(workspace.scratch_dir()) {
        Ok(entries) => entries.map(|entry| entry.unwrap().path()).collect(),
        Err(_) => Vec::new(),
    }
}

#[test]
fn a_backup_exports_the_library_into_a_check_out_and_pushes_it() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let name = random::kebab();
    skill(&workspace, &name);
    let upstream = Folder::new();

    let backed = backup(
        &workspace,
        &asked("https://github.com/you/backup.git"),
        &upstream,
    )
    .unwrap();

    assert_eq!(backed.branch, "main");
    assert!(matches!(backed.published, Published::Pushed { .. }));
    assert!(backed.plan.made);
    assert_eq!(backed.plan.count(export::Change::Added), 1);
    assert!(upstream.holds(&format!(".mindweaver/library/skills/{name}/SKILL.md")));
    assert_eq!(
        upstream.calls(),
        [
            "check out https://github.com/you/backup.git -".to_owned(),
            format!("commit .mindweaver {}", message(&workspace)),
            "push".to_owned(),
        ]
    );
    assert!(
        scratch(&workspace).is_empty(),
        "the check-out is taken away"
    );
}

#[test]
fn a_backup_holding_what_the_branch_holds_pushes_nothing() {
    let (_dir, workspace) = WorkspaceMother::empty();
    skill(&workspace, "deploy");
    let upstream = Folder::new();
    backup(&workspace, &asked("/srv/backup.git"), &upstream).unwrap();

    let backed = backup(&workspace, &asked("/srv/backup.git"), &upstream).unwrap();

    assert_eq!(backed.published, Published::Unchanged);
    assert!(!backed.plan.made);
    assert_eq!(backed.plan.count(export::Change::Unchanged), 1);
}

#[test]
fn a_backup_takes_away_what_the_library_no_longer_holds_with_no_yes() {
    let (_dir, workspace) = WorkspaceMother::empty();
    skill(&workspace, "deploy");
    let old = skill(&workspace, "old");
    let upstream = Folder::new();
    backup(&workspace, &asked("/srv/backup.git"), &upstream).unwrap();
    fs::remove_dir_all(&old).unwrap();

    let backed = backup(&workspace, &asked("/srv/backup.git"), &upstream).unwrap();

    assert_eq!(backed.plan.count(export::Change::Removed), 1);
    assert!(matches!(backed.published, Published::Pushed { .. }));
    assert!(!upstream.holds(".mindweaver/library/skills/old"));
    assert!(upstream.holds(".mindweaver/library/skills/deploy/SKILL.md"));
}

#[test]
fn the_branch_and_message_asked_for_are_the_ones_used() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let upstream = Folder::new();
    let message = random::word();
    let request = Request {
        branch: Some("library"),
        message: Some(&message),
        ..asked("/srv/backup.git")
    };

    let backed = backup(&workspace, &request, &upstream).unwrap();

    assert_eq!(backed.branch, "library");
    assert_eq!(
        upstream.calls(),
        [
            "check out /srv/backup.git library".to_owned(),
            format!("commit .mindweaver {message}"),
            "push".to_owned(),
        ]
    );
}

#[test]
fn a_refused_export_sends_nothing_and_leaves_no_check_out() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let (_, token) = McpMother::token();
    let (definition, _) = McpMother::with_literal("github", "env", &token);
    let mcp = workspace.own().directory_for(Kind::Mcp).unwrap();
    fs::create_dir_all(&mcp).unwrap();
    fs::write(mcp.join("github.toml"), definition).unwrap();
    let upstream = Folder::new();

    let refused = backup(&workspace, &asked("/srv/backup.git"), &upstream).unwrap_err();

    assert_eq!(refused.code(), "export_secret_literal");
    assert_eq!(upstream.calls(), ["check out /srv/backup.git -"]);
    assert!(scratch(&workspace).is_empty());
}

#[test]
fn a_url_carrying_a_credential_is_refused_before_anything_runs() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let secret = random::hex(24);
    let url = format!("https://you:{secret}@github.com/you/backup.git");
    let upstream = Folder::new();

    let refused = backup(&workspace, &asked(&url), &upstream).unwrap_err();

    assert_eq!(refused.code(), "backup_url_credentials");
    assert!(!refused.to_string().contains(&secret));
    assert!(upstream.calls().is_empty());
}

#[test]
fn a_token_for_an_ssh_url_is_refused() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let token = Token::new(random::hex(40));
    let upstream = Folder::new();
    let request = Request {
        token: Some(&token),
        ..asked("git@github.com:you/backup.git")
    };

    let refused = backup(&workspace, &request, &upstream).unwrap_err();

    assert_eq!(refused.code(), "backup_token_over_ssh");
    assert!(upstream.calls().is_empty());
}

#[test]
fn a_workspace_nothing_has_made_has_no_library_to_back_up() {
    let dir = TempDir::new().unwrap();
    let workspace = WeaverWorkspace::unmade(dir.path()).unwrap();
    let upstream = Folder::new();

    let refused = backup(&workspace, &asked("/srv/backup.git"), &upstream).unwrap_err();

    assert_eq!(refused.code(), "backup_workspace_unmade");
    assert!(upstream.calls().is_empty());
    assert!(!workspace.weaver_dir().exists());
}

#[test]
fn a_branch_git_would_refuse_or_read_as_an_option_is_refused_before_anything_runs() {
    let (_dir, workspace) = WorkspaceMother::empty();
    for branch in [
        "", "-f", "--force", "a b", "a..b", "a:b", "a~1", "x.lock", "/a", "a/", "@",
    ] {
        let upstream = Folder::new();
        let request = Request {
            branch: Some(branch),
            ..asked("/srv/backup.git")
        };

        let refused = backup(&workspace, &request, &upstream).unwrap_err();

        assert_eq!(refused.code(), "backup_branch_invalid", "{branch:?}");
        assert!(upstream.calls().is_empty(), "{branch:?}");
    }
    for branch in ["main", "library", "backups/skills", "v1.2"] {
        assert!(is_branch(branch), "{branch}");
    }
}

#[test]
fn a_backup_says_each_step_it_takes_in_order() {
    let (_dir, workspace) = WorkspaceMother::empty();
    skill(&workspace, &random::kebab());
    let upstream = Folder::new();
    let (watch, seen) = followed(None);
    let request = Request {
        watch: &watch,
        ..asked("/srv/backup.git")
    };

    backup(&workspace, &request, &upstream).unwrap();

    assert_eq!(
        *seen.lock().unwrap(),
        [
            Progress::CheckingOut,
            Progress::Exporting,
            Progress::Committing,
            Progress::Pushing,
        ]
    );
}

#[test]
fn a_backup_with_nothing_to_send_never_says_it_pushes() {
    let (_dir, workspace) = WorkspaceMother::empty();
    skill(&workspace, "deploy");
    let upstream = Folder::new();
    backup(&workspace, &asked("/srv/backup.git"), &upstream).unwrap();
    let (watch, seen) = followed(None);
    let request = Request {
        watch: &watch,
        ..asked("/srv/backup.git")
    };

    let backed = backup(&workspace, &request, &upstream).unwrap();

    assert_eq!(backed.published, Published::Unchanged);
    assert_eq!(
        *seen.lock().unwrap(),
        [
            Progress::CheckingOut,
            Progress::Exporting,
            Progress::Committing
        ]
    );
}

#[test]
fn a_cancel_at_any_step_before_the_push_sends_nothing_and_takes_the_check_out_away() {
    for step in [
        Progress::CheckingOut,
        Progress::Exporting,
        Progress::Committing,
    ] {
        let (_dir, workspace) = WorkspaceMother::empty();
        let name = random::kebab();
        skill(&workspace, &name);
        let upstream = Folder::new();
        let (watch, seen) = followed(Some(step));
        let request = Request {
            watch: &watch,
            ..asked("/srv/backup.git")
        };

        let refused = backup(&workspace, &request, &upstream).unwrap_err();

        assert_eq!(refused.code(), "backup_cancelled", "{step:?}");
        assert_eq!(seen.lock().unwrap().last(), Some(&step), "{step:?}");
        assert!(!upstream.calls().contains(&"push".to_owned()), "{step:?}");
        assert!(
            !upstream.holds(&format!(".mindweaver/library/skills/{name}")),
            "{step:?}: the remote is as it was"
        );
        assert!(scratch(&workspace).is_empty(), "{step:?}");
    }
}

#[test]
fn a_cancel_while_the_upstream_runs_before_the_push_is_the_backups_own() {
    for during in ["check out", "commit"] {
        let (_dir, workspace) = WorkspaceMother::empty();
        skill(&workspace, &random::kebab());
        let upstream = Folder::cancelled_during(during);

        let refused = backup(&workspace, &asked("/srv/backup.git"), &upstream).unwrap_err();

        assert!(matches!(refused, BackupError::Cancelled), "{during}");
        assert_eq!(
            refused.to_string(),
            "the backup was cancelled before anything was pushed: the repository is as it was"
        );
        assert!(!upstream.calls().contains(&"push".to_owned()), "{during}");
        assert!(scratch(&workspace).is_empty(), "{during}");
    }
}

#[test]
fn a_cancel_once_the_push_has_started_is_too_late_and_the_backup_finishes() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let name = random::kebab();
    skill(&workspace, &name);
    let upstream = Folder::new();
    let (watch, _) = followed(Some(Progress::Pushing));
    let request = Request {
        watch: &watch,
        ..asked("/srv/backup.git")
    };

    let backed = backup(&workspace, &request, &upstream).unwrap();

    assert!(watch.is_cancelled());
    assert!(matches!(backed.published, Published::Pushed { .. }));
    assert!(upstream.holds(&format!(".mindweaver/library/skills/{name}/SKILL.md")));
    assert!(scratch(&workspace).is_empty());
}

#[test]
fn a_backup_cancelled_before_it_starts_asks_the_upstream_nothing() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let upstream = Folder::new();
    let (watch, seen) = followed(None);
    watch.cancel();
    let request = Request {
        watch: &watch,
        ..asked("/srv/backup.git")
    };

    let refused = backup(&workspace, &request, &upstream).unwrap_err();

    assert_eq!(refused.code(), "backup_cancelled");
    assert!(seen.lock().unwrap().is_empty());
    assert!(upstream.calls().is_empty());
    assert!(scratch(&workspace).is_empty());
}

#[test]
fn only_the_push_is_past_a_cancel() {
    for (progress, cancellable) in [
        (Progress::CheckingOut, true),
        (Progress::Exporting, true),
        (Progress::Committing, true),
        (Progress::Pushing, false),
    ] {
        assert_eq!(progress.cancellable(), cancellable, "{progress:?}");
    }
}

#[test]
fn an_upstream_that_honoured_a_cancel_is_said_as_the_backups_cancel() {
    let error = BackupError::from(UpstreamError::Cancelled);
    assert_eq!(error.code(), "backup_cancelled");

    let error = BackupError::from(UpstreamError::Commit {
        detail: random::word(),
    });
    assert_eq!(error.code(), "backup_commit_failed");
}
