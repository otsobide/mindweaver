//! Unit tests for `backup.rs`, kept beside it rather than inside it.
//!
//! Through a stand-in [`Upstream`] whose remote is a folder on the disk: a
//! check-out copies it, and a push puts the check-out's workspace folder
//! back when it holds something else. What a backup writes, what it sends
//! and when it sends nothing, what it refuses before asking the upstream
//! anything, and the check-out taken away whatever came of it.

use super::*;

use std::sync::Mutex;

use tempfile::TempDir;

use crate::copy;
use crate::mother::{random, McpMother, SkillMother, WorkspaceMother};
use crate::port::CheckedOut;
use crate::value::Revision;

/// A remote that is a folder: what its branch holds.
#[derive(Debug)]
struct Folder {
    remote: TempDir,
    /// Each call, as `check out <url> <branch>` or `publish <paths> <message>`.
    calls: Mutex<Vec<String>>,
}

impl Folder {
    fn new() -> Self {
        Self {
            remote: TempDir::new().unwrap(),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn holds(&self, route: &str) -> bool {
        self.remote.path().join(route).exists()
    }
}

impl Upstream for Folder {
    fn check_out(
        &self,
        url: &str,
        branch: Option<&str>,
        into: &Path,
        _token: Option<&Token>,
    ) -> Result<CheckedOut, UpstreamError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("check out {url} {}", branch.unwrap_or("-")));
        assert!(!into.exists(), "a check-out is made where nothing is");
        copy::tree(self.remote.path(), into, self.remote.path()).unwrap();
        Ok(CheckedOut {
            branch: branch.unwrap_or("main").to_owned(),
        })
    }

    fn publish(
        &self,
        work: &Path,
        paths: &[&str],
        message: &str,
        _token: Option<&Token>,
    ) -> Result<Published, UpstreamError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("publish {} {message}", paths.join(",")));
        let mut changed = false;
        for path in paths {
            let (from, to) = (work.join(path), self.remote.path().join(path));
            if !to.exists() || !copy::same(&from, &to).unwrap() {
                copy::replace(&from, &to, &from).unwrap();
                changed = true;
            }
        }
        Ok(if changed {
            Published::Pushed {
                revision: Revision::from(random::hex(40)),
            }
        } else {
            Published::Unchanged
        })
    }
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
            format!("publish .mindweaver {}", message(&workspace)),
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
            format!("publish .mindweaver {message}"),
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
