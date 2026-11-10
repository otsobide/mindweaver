//! `mindweaver backup`: the library exported into a check-out of a git
//! repository, committed and pushed with the system's `git`, as core does it
//! ([`mindweaver_core::backup`]). Here the token is read, and what came of
//! it said.
//!
//! No `mind` form, as `mindweaver export` has none.

use std::fmt::Write as _;

use mindweaver_core::adapter::git::SystemGit;
use mindweaver_core::backup::{self, Request};
use mindweaver_core::port::{Published, Token};
use mindweaver_core::{Kind, WeaverWorkspace};

use crate::commands::export::listed;
use crate::progress::{Progress, Watch};
use crate::{CliError, Failure, Outcome};

/// What `mindweaver backup <url>` was told, beside the workspace.
pub(crate) struct Backing<'a> {
    pub url: &'a str,
    pub branch: Option<&'a str>,
    pub kinds: &'a [Kind],
    pub message: Option<&'a str>,
    pub token_env: Option<&'a str>,
}

/// Back the library up to `asked.url`, with the token the variable
/// `asked.token_env` names holds, read here, for this run, and handed to
/// core: a front end decides where a credential comes from (D-16), and core
/// never reads the environment.
///
/// Each step it takes is told to `watch`, and a cancel of it stops the
/// backup until it pushes, as a gather's stops it until it places: what it
/// then says is the same either way, so a front end that draws the steps
/// shows the same report afterwards as one that does not.
pub(crate) fn backup_to(
    workspace: &WeaverWorkspace,
    asked: &Backing,
    watch: &Watch,
) -> Result<Outcome, Failure> {
    let token = asked.token_env.map(token_from).transpose()?;
    let watch = watch.relaying(Progress::Backup);
    let request = Request {
        url: asked.url,
        branch: asked.branch,
        kinds: asked.kinds,
        message: asked.message,
        token: token.as_ref(),
        watch: &watch,
    };
    let backed =
        backup::backup(workspace, &request, &SystemGit::default()).map_err(CliError::from)?;

    let mut stdout = String::new();
    let _ = writeln!(stdout, "{}, branch {}", backed.url, backed.branch);
    stdout.push_str(&listed(&backed.plan, true, "to back up"));
    match &backed.published {
        Published::Pushed { revision } => {
            let revision: &str = revision.as_ref();
            let short = revision.get(..7).unwrap_or(revision);
            let _ = writeln!(stdout, "pushed {short} to {}", backed.branch);
        }
        Published::Unchanged => {
            let _ = writeln!(
                stdout,
                "nothing to push: {} holds this already",
                backed.branch
            );
        }
    }
    Ok(Outcome::plain(stdout))
}

/// The token the variable `name` holds, refused when it is unset or empty:
/// a token asked for and not there is a mistake to say, never a reason to
/// log in some other way unasked.
fn token_from(name: &str) -> Result<Token, CliError> {
    match std::env::var(name) {
        Ok(token) if !token.trim().is_empty() => Ok(Token::new(token.trim())),
        _ => Err(CliError::TokenUnset(name.to_owned())),
    }
}
