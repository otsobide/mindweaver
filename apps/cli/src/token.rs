//! Where a token a command logs in with for one run comes from (D-16): the
//! variable `--token-env` names, or what `gh auth token` gives for a host,
//! with `--gh`. Read here, by the front end, and handed to core as a
//! `port::Token`, since core never reads the environment and never runs
//! `gh`. Neither is printed, and nothing here asks a person for anything.
//!
//! A gather's token comes from a command typed in a shell alone: no task a
//! front end sends for a gather carries a token, the name of a variable or
//! `--gh`, so its lines never spell either option. A backup's task may name
//! a variable, never the token, held to core's rule for one
//! (`policy::token_variable`), and its line reads it here as any other.

use std::io;
use std::process::{Command, Stdio};

use mindweaver_core::port::Token;

use crate::CliError;

/// The program `--gh` runs, found on the PATH as a shell finds it.
const GH: &str = "gh";

/// The token the variable `name` holds, refused when it is unset or empty:
/// a token asked for and not there is a mistake to say, never a reason to
/// log in some other way unasked.
pub(crate) fn from_variable(name: &str) -> Result<Token, CliError> {
    match std::env::var(name) {
        Ok(token) if !token.trim().is_empty() => Ok(Token::new(token.trim())),
        _ => Err(CliError::TokenUnset(name.to_owned())),
    }
}

/// The token `gh auth token --hostname <host>` gives, for `host` alone.
///
/// The host named rather than `gh`'s default one, so that a GitHub token is
/// never handed to another forge a URL names: `gh` then says it has none
/// for that host, which is said here in words of its own. What `gh` prints
/// is never shown, its token on standard output least of all; it is told not
/// to prompt, and given no standard input to prompt on.
pub(crate) fn from_gh(host: &str) -> Result<Token, CliError> {
    let output = Command::new(GH)
        .args(["auth", "token", "--hostname", host])
        .stdin(Stdio::null())
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .output()
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => CliError::GhMissing,
            _ => CliError::GhUnrun(error.to_string()),
        })?;
    let printed = String::from_utf8(output.stdout).unwrap_or_default();
    match printed.lines().next().map(str::trim) {
        Some(token) if output.status.success() && !token.is_empty() => Ok(Token::new(token)),
        _ => Err(CliError::GhLoggedOut(host.to_owned())),
    }
}
