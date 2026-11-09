//! The [`Upstream`] a backup is sent through: the system's `git`, the one
//! place core runs it (D-27, in docs/architecture.md).
//!
//! `gix` checks out and fetches, and cannot push; a backup is the one thing
//! Mindweaver sends, and `git` is what already knows how to authenticate as
//! the person running it: their credential helpers (`gh auth setup-git`
//! among them), their ssh keys and agent, their `insteadOf` rewrites. So it
//! is run as it is configured, with three things taken away: it never asks
//! on the terminal (`GIT_TERMINAL_PROMPT=0`), nor through an askpass
//! program, from the environment or from its configuration, and it reads no
//! standard input. A token handed over for the run reaches it only through a
//! credential helper given on its command line, which reads the token from
//! a variable of the child's own environment, so the token is never an
//! argument another process could list, and lasts the one process.
//!
//! What ssh itself asks on the terminal, a passphrase or a host key, is out
//! of `git`'s reach as it is out of a gather's, and the same open question.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use crate::port::{CheckedOut, Published, Token, Upstream, UpstreamError};
use crate::url::redacted;
use crate::value::Revision;

/// The variable a token reaches the credential helper through, in the
/// environment of the one `git` it is handed to.
const TOKEN_VARIABLE: &str = "MINDWEAVER_BACKUP_TOKEN";

/// The helper that answers with the token: for a `get` alone, so a `store`
/// or an `erase` the run asks for keeps nothing anywhere. `x-access-token`
/// is the user GitHub takes any token with, and others ignore.
const TOKEN_HELPER: &str = "!f() { test \"$1\" = get || exit 0; \
     echo username=x-access-token; echo \"password=$MINDWEAVER_BACKUP_TOKEN\"; }; f";

/// The branch an empty repository's first commit is made on when the remote
/// does not say, rather than whatever this machine's git would default to.
const FIRST_BRANCH: &str = "main";

/// Who a commit is by when the configuration names nobody: the commit is
/// still made, rather than the backup refused for want of a name.
const FALLBACK_NAME: &str = "Mindweaver";
const FALLBACK_EMAIL: &str = "mindweaver@localhost";

/// The system's `git`, found on the `PATH` unless another program is named.
#[derive(Debug, Clone)]
pub struct SystemGit {
    program: PathBuf,
    /// Variables set for every `git` it runs beside what it inherits: none,
    /// but in a test, which gives it a home of its own
    /// ([`in_home`](Self::in_home)).
    environment: Vec<(&'static str, PathBuf)>,
}

impl Default for SystemGit {
    fn default() -> Self {
        Self::at("git")
    }
}

impl SystemGit {
    /// The `git` at `program`, for a test, or a front end told which.
    pub fn at(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            environment: Vec::new(),
        }
    }

    /// The same, reading its configuration from `home` alone, never the
    /// developer's: no test reads the real home, and a `git` that did would
    /// sign, or reach a helper, as its owner set it up to.
    #[cfg(test)]
    pub(crate) fn in_home(mut self, home: &Path) -> Self {
        self.environment = vec![
            ("HOME", home.to_path_buf()),
            ("XDG_CONFIG_HOME", home.join(".config")),
            ("GIT_CONFIG_GLOBAL", home.join(".gitconfig")),
            ("GIT_CONFIG_NOSYSTEM", PathBuf::from("1")),
        ];
        self
    }

    /// `git` run in `dir` with `args`, never asking anybody anything, the
    /// token offered when one is given.
    fn run<I, S>(&self, dir: &Path, args: I, token: Option<&Token>) -> Result<Output, UpstreamError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(&self.program);
        command
            .current_dir(dir)
            .stdin(Stdio::null())
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C")
            .env_remove("GIT_ASKPASS")
            .env_remove("SSH_ASKPASS")
            .args(["-c", "core.askPass="]);
        for (name, value) in &self.environment {
            command.env(name, value);
        }
        if let Some(token) = token {
            // Every helper configured cleared first, so the token is the one
            // answer and no helper is told it to keep.
            command
                .args(["-c", "credential.helper="])
                .args(["-c", &format!("credential.helper={TOKEN_HELPER}")])
                .env(TOKEN_VARIABLE, token.reveal());
        }
        command
            .args(args)
            .output()
            .map_err(|error| UpstreamError::Unavailable {
                detail: format!("{}: {error}", self.program.display()),
            })
    }

    /// Whether the remote holds `branch`.
    fn has_branch(
        &self,
        dir: &Path,
        url: &str,
        branch: &str,
        token: Option<&Token>,
    ) -> Result<bool, UpstreamError> {
        let reference = format!("refs/heads/{branch}");
        let output = self.run(
            dir,
            ["ls-remote", "--exit-code", "--heads", "--", url, &reference],
            token,
        )?;
        match output.status.code() {
            Some(0) => Ok(true),
            // What `--exit-code` says when nothing matched.
            Some(2) => Ok(false),
            _ => Err(checked_out(url, &output)),
        }
    }
}

impl Upstream for SystemGit {
    fn check_out(
        &self,
        url: &str,
        branch: Option<&str>,
        into: &Path,
        token: Option<&Token>,
    ) -> Result<CheckedOut, UpstreamError> {
        let parent = into.parent().unwrap_or(into);
        let named = match branch {
            Some(branch) => Some((branch, self.has_branch(parent, url, branch, token)?)),
            None => None,
        };
        let mut clone: Vec<&OsStr> = vec![
            OsStr::new("-c"),
            OsStr::new("init.defaultBranch=main"),
            OsStr::new("clone"),
            OsStr::new("--quiet"),
            OsStr::new("--depth"),
            OsStr::new("1"),
            OsStr::new("--no-tags"),
        ];
        if let Some((branch, true)) = named {
            clone.extend([OsStr::new("--branch"), OsStr::new(branch)]);
        }
        clone.extend([OsStr::new("--"), OsStr::new(url), into.as_os_str()]);
        let output = self.run(parent, clone, token)?;
        if !output.status.success() {
            return Err(checked_out(url, &output));
        }
        if let Some((branch, false)) = named {
            let output = self.run(into, ["checkout", "--quiet", "-b", branch], None)?;
            if !output.status.success() {
                return Err(checked_out(url, &output));
            }
        }
        let output = self.run(into, ["symbolic-ref", "--short", "HEAD"], None)?;
        if !output.status.success() {
            return Err(checked_out(url, &output));
        }
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        Ok(CheckedOut {
            branch: if branch.is_empty() {
                FIRST_BRANCH.to_owned()
            } else {
                branch
            },
        })
    }

    fn publish(
        &self,
        work: &Path,
        paths: &[&str],
        message: &str,
        token: Option<&Token>,
    ) -> Result<Published, UpstreamError> {
        let committed = |output: &Output| UpstreamError::Commit {
            detail: said(output),
        };
        let mut add = vec!["add", "--all", "--"];
        add.extend(paths);
        let output = self.run(work, add, None)?;
        if !output.status.success() {
            return Err(committed(&output));
        }
        let output = self.run(work, ["diff", "--cached", "--quiet"], None)?;
        let changed = match output.status.code() {
            Some(0) => false,
            Some(1) => true,
            _ => return Err(committed(&output)),
        };
        let output = self.run(work, ["symbolic-ref", "--short", "HEAD"], None)?;
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !changed {
            // A branch the remote has not got yet is still sent, holding
            // what the one it was started from holds: asked for, it is
            // there afterwards. Nothing to send when there is no commit at
            // all, or the remote has the branch already.
            let head = self.run(work, ["rev-parse", "--verify", "--quiet", "HEAD"], None)?;
            let tracked = format!("refs/remotes/origin/{branch}");
            let known = self.run(work, ["rev-parse", "--verify", "--quiet", &tracked], None)?;
            if !head.status.success() || known.status.success() {
                return Ok(Published::Unchanged);
            }
        }

        let mut commit: Vec<String> = Vec::new();
        for (key, fallback) in [("user.name", FALLBACK_NAME), ("user.email", FALLBACK_EMAIL)] {
            let set = self.run(work, ["config", "--get", key], None)?;
            if !set.status.success() || set.stdout.trim_ascii().is_empty() {
                commit.extend(["-c".to_owned(), format!("{key}={fallback}")]);
            }
        }
        // No hook: what runs on a commit is somebody's own, set up for the
        // repositories they work in, and this one is Mindweaver's to write.
        commit
            .extend(["commit", "--quiet", "--no-verify", "--message", message].map(str::to_owned));
        if changed {
            let output = self.run(work, &commit, None)?;
            if !output.status.success() {
                return Err(committed(&output));
            }
        }
        let output = self.run(work, ["rev-parse", "HEAD"], None)?;
        if !output.status.success() {
            return Err(committed(&output));
        }
        let revision = Revision::from(String::from_utf8_lossy(&output.stdout).trim().to_owned());

        let url = remote(self, work)?;
        let refspec = format!("HEAD:refs/heads/{branch}");
        let output = self.run(work, ["push", "--quiet", "origin", &refspec], token)?;
        if !output.status.success() {
            return Err(if asks_who(&output) {
                UpstreamError::Credentials { url }
            } else {
                UpstreamError::Push {
                    url,
                    detail: said(&output),
                }
            });
        }
        Ok(Published::Pushed { revision })
    }
}

/// The URL the work tree was checked out from, without its credentials,
/// for an error to name.
fn remote(git: &SystemGit, work: &Path) -> Result<String, UpstreamError> {
    let output = git.run(work, ["remote", "get-url", "origin"], None)?;
    Ok(redacted(String::from_utf8_lossy(&output.stdout).trim()).into_owned())
}

/// Why a check-out failed: credentials, when that is what `git` said.
fn checked_out(url: &str, output: &Output) -> UpstreamError {
    let url = redacted(url).into_owned();
    if asks_who(output) {
        UpstreamError::Credentials { url }
    } else {
        UpstreamError::CheckOut {
            url,
            detail: said(output),
        }
    }
}

/// Whether `git` stopped because the server asked who was there and nobody
/// it may ask had an answer: over https, the prompt it was told not to show
/// or a login refused; over ssh, a key refused.
fn asks_who(output: &Output) -> bool {
    let said = String::from_utf8_lossy(&output.stderr);
    [
        "terminal prompts disabled",
        "could not read Username",
        "could not read Password",
        "Authentication failed",
        "Invalid username or password",
        "Permission denied (publickey",
    ]
    .iter()
    .any(|sign| said.contains(sign))
}

/// What `git` said on stderr, its last line that says something, every URL
/// in it without credentials: the line that says why, where the others are
/// hints of what to try.
fn said(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty() && !line.starts_with("hint:"))
        .unwrap_or("it said nothing")
        .trim_start_matches("fatal: ")
        .trim_start_matches("error: ");
    redacted(line).into_owned()
}

#[cfg(test)]
#[path = "git_test.rs"]
mod tests;
