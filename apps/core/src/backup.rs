//! Sending the library to a repository: `mindweaver backup` (D-27, in
//! docs/architecture.md).
//!
//! A backup is an export ([`crate::export`]) into a fresh check-out of the
//! repository, committed and pushed: what the repository then holds is what
//! `mindweaver export` would write into a folder, a workspace whose library
//! is this one's, so `git clone` and `mindweaver load` bring it back. The
//! check-out is made in the workspace's scratch folder and taken away
//! whatever came of the backup, so nothing of the repository is kept here
//! between runs, and two machines backing up to one branch each start from
//! what the other pushed.
//!
//! An export into it takes away, with no yes, what the library no longer
//! holds: an export asks first because the folder it writes may have been
//! changed by hand since, and a branch keeps every earlier backup in its
//! history, where nothing a later one takes away is lost.
//!
//! What reaches the repository goes through the [`Upstream`] port, the
//! system's `git` unless another is handed over, and never prompts. A URL
//! carrying a credential is refused before anything runs, as a gather
//! refuses one: a token comes from the front end for one run
//! ([`Request::token`]), and is refused for an ssh URL, which logs in with
//! keys of its own.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::code::codes;
use crate::export::{self, ExportError, Place, Plan};
use crate::kind::Kind;
use crate::port::{Published, Token, Upstream, UpstreamError};
use crate::url::{carries_credentials, over_ssh};
use crate::value::Redacted;
use crate::workspace::WeaverWorkspace;

/// What a backup is asked for.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// The repository, as `git` takes it: an https or ssh URL, or a path.
    pub url: &'a str,
    /// The branch to commit to, made from the remote's own when it has no
    /// such branch; the remote's own when none is given.
    pub branch: Option<&'a str>,
    /// The kinds to back up, every kind the workspace keeps its own of when
    /// none is given, as for an export.
    pub kinds: &'a [Kind],
    /// What the commit says, [`message`] when nothing is given.
    pub message: Option<&'a str>,
    /// A token to log in with over https, which the front end read for this
    /// run (D-16). Without one, whatever `git` logs in with is used.
    pub token: Option<&'a Token>,
}

/// What a backup did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackedUp {
    /// The repository, without its credentials.
    pub url: Redacted,
    /// The branch committed to.
    pub branch: String,
    /// What the export into the check-out did, its folder being the
    /// check-out, now gone.
    pub plan: Plan,
    /// Whether a commit was pushed, and which.
    pub published: Published,
}

/// Why nothing was backed up.
#[derive(Debug, Error)]
pub enum BackupError {
    #[error(
        "{url}: carries a credential, which would be kept in the check-out and said in every \
         error: name a variable holding a token with --token-env instead"
    )]
    Credentialed { url: Redacted },
    #[error(
        "{url}: reached over ssh, which logs in with keys of its own, so a token would be for \
         nothing: leave --token-env out, or use the https URL"
    )]
    TokenOverSsh { url: Redacted },
    #[error(
        "`{branch}` is not a branch name a backup commits to: a name git takes, not starting \
         with `-`, with no space, `..`, `~`, `^`, `:`, `?`, `*`, `[` or `\\` in it"
    )]
    Branch { branch: String },
    #[error(
        "the workspace at {} is not made yet, so it has no library to back up",
        root.display()
    )]
    Unmade { root: PathBuf },
    #[error("{}: the scratch folder cannot be prepared: {source}", path.display())]
    Scratch {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Export(#[from] ExportError),
    #[error(transparent)]
    Upstream(#[from] UpstreamError),
}

codes!(BackupError {
    Self::Credentialed { .. } => "backup_url_credentials",
    Self::TokenOverSsh { .. } => "backup_token_over_ssh",
    Self::Branch { .. } => "backup_branch_invalid",
    Self::Unmade { .. } => "backup_workspace_unmade",
    Self::Scratch { .. } => "backup_scratch_failed",
} carried {
    Self::Export(error) => error,
    Self::Upstream(error) => error,
});

/// What a backup's commit says when nothing else is given.
pub fn message(workspace: &WeaverWorkspace) -> String {
    format!("Back up the library of {}", workspace.name())
}

/// Back `workspace`'s library up to the repository `request` names, through
/// `upstream`.
pub fn backup(
    workspace: &WeaverWorkspace,
    request: &Request,
    upstream: &dyn Upstream,
) -> Result<BackedUp, BackupError> {
    let url = Redacted::new(request.url);
    if carries_credentials(request.url) {
        return Err(BackupError::Credentialed { url });
    }
    if request.token.is_some() && over_ssh(request.url) {
        return Err(BackupError::TokenOverSsh { url });
    }
    if let Some(branch) = request.branch.filter(|branch| !is_branch(branch)) {
        return Err(BackupError::Branch {
            branch: branch.to_owned(),
        });
    }
    if !workspace.is_made() {
        return Err(BackupError::Unmade {
            root: workspace.root().to_path_buf(),
        });
    }

    let work = CheckOut::new(&workspace.scratch_dir())?;
    let checked = upstream.check_out(request.url, request.branch, &work.0, request.token)?;
    let exported = export::export_at(workspace, &work.0, request.kinds, true, Place::Scratch)?;
    let held = WeaverWorkspace::open(&work.0)
        .map_err(ExportError::from)?
        .names()
        .workspace_dir();
    let message = request
        .message
        .map_or_else(|| message(workspace), str::to_owned);
    let published = upstream.publish(&work.0, &[held], &message, request.token)?;

    Ok(BackedUp {
        url,
        branch: checked.branch,
        plan: exported.plan,
        published,
    })
}

/// Whether `name` is one git takes for a branch, by the rules of `git
/// check-ref-format` a typed name can break, and never one a command line
/// would read as an option: what reaches `git checkout -b` is a name.
fn is_branch(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with(['/', '.'])
        && !name.ends_with(".lock")
        && !name.contains("..")
        && !name.contains("//")
        && !name.contains("@{")
        && name != "@"
        && !name
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "~^:?*[\\".contains(c))
}

/// A check-out's folder in the scratch folder, a name nothing holds yet,
/// taken away with everything in it when dropped, whatever came of the
/// backup: a repository left there would be read by nothing, and could hold
/// what a later backup should not find.
struct CheckOut(PathBuf);

impl CheckOut {
    fn new(scratch: &Path) -> Result<Self, BackupError> {
        fs::create_dir_all(scratch).map_err(|source| BackupError::Scratch {
            path: scratch.to_path_buf(),
            source,
        })?;
        let pid = std::process::id();
        let mut attempt = 0u32;
        loop {
            let path = scratch.join(format!("backup-{pid}-{attempt}"));
            if fs::symlink_metadata(&path).is_err() {
                return Ok(Self(path));
            }
            attempt += 1;
        }
    }
}

impl Drop for CheckOut {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
#[path = "backup_test.rs"]
mod tests;
