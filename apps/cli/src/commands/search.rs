//! `mindweaver gather search`: the skill directories a workspace enables
//! asked what they find for a query (#108), each through core's HTTP
//! client, and, with `--take`, the result picked gathered as `gather git`
//! gathers it, through the same function that command runs.
//!
//! Which finders are asked, what each is sent and answers, and what of it
//! is a result are core's ([`finder`]): this reads the token a person names
//! (D-16), lays out what was found and says what was not. The token is read
//! here, since core never reads the environment, and handed to core for
//! this one search: never printed, logged or kept, and sent to GitHub's
//! API alone, as a secret the HTTP cache keeps nothing of.

use std::path::Path;

use mindweaver_core::finder::{self, Enabled, Finder, Found, Hit, Search};
use mindweaver_core::http::{Cache, Client};
use mindweaver_core::port::Token;
use mindweaver_core::{Kind, WeaverWorkspace};

use super::gather::run_gather;
use super::{reading_workspace, workspace_here};
use crate::progress::{Progress, Watch};
use crate::render::{json_or_text, plural, printable, table};
use crate::task::{shell, Task};
use crate::{
    token, CliError, Credential, Failure, GatherCommand, Lookup, Outcome, SearchCredential, Warning,
};

/// What `gather search` was asked: the query, the finders named, where a
/// finder is asked in place of its own address, and where a token comes
/// from.
pub(crate) struct Asked<'a> {
    pub query: &'a str,
    pub finders: &'a [Finder],
    /// Each `FINDER=URL` as `--finder-url` took it, which core reads.
    pub at: &'a [String],
    pub credential: &'a SearchCredential,
}

/// `mindweaver gather search`: what each finder asked finds, a row each.
///
/// A read: the workspace is not made for it, and what the finders answer
/// is kept in its cache only once it is.
pub(crate) fn run_search(
    asked: &Asked<'_>,
    json: bool,
    directory: &Path,
    lookup: &Lookup,
    watch: &Watch,
) -> Result<Outcome, Failure> {
    let mut found = None;
    let text = reading_workspace(directory, lookup, |workspace| {
        let (searched, enabled, _) = search(workspace, asked, watch)?;
        let outcome = listed(&searched, asked, &enabled);
        found = Some(searched);
        Ok(outcome)
    })?;
    Ok(json_or_text(found.filter(|_| json), text))
}

/// `mindweaver gather search --take N`: the search, then the result it
/// lists `number` gathered from its repository and its folder there, by
/// the line `gather git` would be typed as, said first so that what was
/// taken can be gathered again without searching.
///
/// A write, as a gather is, so the workspace is made when it is not yet.
/// What the search said, a finder that failed among it, is said before what
/// the gather says: the result taken is the one listed `number` by what did
/// answer.
pub(crate) fn run_take(
    asked: &Asked<'_>,
    number: u32,
    directory: &Path,
    lookup: &Lookup,
    watch: &Watch,
) -> Result<Outcome, Failure> {
    let workspace = workspace_here(directory, lookup)?;
    let (found, enabled, search) = search(&workspace, asked, watch)?;
    let said = listed(&found, asked, &enabled).stderr;
    let refused = |error: CliError, said: Vec<Warning>| Failure {
        error: Box::new(error),
        warnings: said,
    };
    let hit = match found.take(number as usize) {
        Ok(hit) => hit,
        Err(error) => return Err(refused(error.into(), said)),
    };

    // The token goes on to the gather only for a repository on the host it
    // is for: a finder may name any host, and a gather sends its token to
    // the one it fetches from.
    let credential = if search.token_reaches(hit) {
        asked.credential.as_gathered()
    } else {
        Credential::default()
    };
    let mut line = Task::Gather {
        url: hit.repository.to_string(),
        folder: hit.folder().to_owned(),
        reference: hit.reference.clone(),
        kind: Kind::Skill,
        name: None,
        pin: None,
        dry_run: false,
    }
    .commands()
    .remove(0);
    // Before the URL, and before the `--` a URL starting with a dash is
    // put behind, so the line read back takes the option as one.
    let handed: Vec<String> = match (&credential.token_env, credential.gh) {
        (Some(variable), _) => vec![String::from("--token-env"), variable.clone()],
        (None, true) => vec![String::from("--gh")],
        (None, false) => Vec::new(),
    };
    let at = line
        .iter()
        .position(|word| word == "--")
        .unwrap_or(line.len() - 1);
    line.splice(at..at, handed);
    let command = GatherCommand::Git {
        url: hit.repository.to_string(),
        kind: None,
        path: hit.path.clone(),
        reference: hit.reference.clone(),
        name: None,
        pin: None,
        dry_run: false,
        credential,
    };

    let taking = format!(
        "taking result {number}, {}: {}\n",
        printable(&hit.name),
        printable(&shell(&line))
    );
    let clean = said.iter().all(|line| line.code.is_none());
    match run_gather(&command, directory, lookup, watch) {
        Ok(mut outcome) => {
            outcome.stdout.insert_str(0, &taking);
            outcome.stderr.splice(0..0, said);
            outcome.ok = outcome.ok && clean;
            Ok(outcome)
        }
        Err(mut failure) => {
            failure.warnings.splice(0..0, said);
            Err(failure)
        }
    }
}

/// What the finders `asked` names, of those `workspace` enables, find,
/// asked through a client keeping what they answer in its cache once it is
/// made, reporting to `watch`; with what is enabled, and the search, which
/// says where a token may go on to.
fn search(
    workspace: &WeaverWorkspace,
    asked: &Asked<'_>,
    watch: &Watch,
) -> Result<(Found, Enabled, Search), Failure> {
    let enabled = finder::enabled(workspace);
    let finders = enabled.asked(asked.finders).map_err(CliError::from)?;
    // Only a workspace already made keeps what was answered: a read makes
    // nothing in a home, and the cache's folder would be the first thing.
    let client = if workspace.is_made() {
        Client::new().caching(Cache::of(workspace, finder::FRESH_FOR))
    } else {
        Client::new()
    };
    let mut search = Search::new(client).watched_by(watch.relaying(Progress::Search));
    for at in asked.at {
        let (finder, base) = finder::addressed(at).map_err(CliError::from)?;
        search = search.at(finder, &base).map_err(CliError::from)?;
    }
    // Read only when a finder to ask takes one, so `--gh` runs `gh` for a
    // search that would send its token somewhere, and for no other.
    let wants = finders.iter().any(|finder| finder.needs_token());
    let token = if wants {
        handed(asked.credential, &search)?
    } else {
        None
    };
    let found = search
        .clone()
        .with_token(token)
        .run(asked.query, &finders)
        .map_err(CliError::from)?;
    Ok((found, enabled, search))
}

/// The token `credential` says GitHub's search is asked with, read now for
/// this search alone: the variable `--token-env` names, or what `gh` has
/// for the host core says the token is for. `None` when neither is given.
fn handed(credential: &SearchCredential, search: &Search) -> Result<Option<Token>, CliError> {
    if let Some(name) = &credential.token_env {
        return token::from_variable(name).map(Some);
    }
    if credential.gh {
        return token::from_gh(&search.token_host()).map(Some);
    }
    Ok(None)
}

/// Each result found, a row each, numbered in the order core lists them
/// ([`rows`]), or that none found anything.
///
/// A finder skipped for want of a token is a hint, which fails nothing; a
/// name the marker lists that is no finder, a finder that failed, a result
/// dropped and the cache failing are each a warning with its code, which
/// fails the search, as a failure collected does.
fn listed(found: &Found, asked: &Asked<'_>, enabled: &Enabled) -> Outcome {
    let mut outcome = if found.hits.is_empty() {
        Outcome::plain(format!(
            "no finder found a skill for `{}`\n",
            printable(asked.query)
        ))
    } else {
        Outcome::plain(table(&rows(&found.hits)))
    };
    outcome.stderr.extend(found.skipped.iter().map(|finder| {
        Warning::from(format!(
            "hint: {finder} was not asked, since its search needs a token: --token-env or \
                 --gh hands one over"
        ))
    }));
    let said = enabled
        .unknown
        .iter()
        .map(|unknown| Warning::coded(unknown.code(), format!("warning: {unknown}")))
        .chain(
            found
                .failures
                .iter()
                .map(|failure| Warning::coded(failure.code(), format!("warning: {failure}"))),
        )
        .chain(
            found
                .warnings
                .iter()
                .map(|warning| Warning::coded(warning.code(), format!("warning: {warning}"))),
        );
    outcome.stderr.extend(said);
    outcome.ok = outcome.stderr.iter().all(|line| line.code.is_none());
    outcome
}

/// Each of `hits` as a row, numbered from 1 as `--take` names one: its
/// name, its repository, with the branch a finder named after a `#`, its
/// folder there, `.` for the root, its installs or its repository's stars,
/// and the finders that found it. What the command prints and the TUI's
/// results draw, so the two say a result in the same words.
pub(crate) fn rows(hits: &[Hit]) -> Vec<Vec<String>> {
    hits.iter()
        .enumerate()
        .map(|(at, hit)| {
            let repository = match &hit.reference {
                Some(reference) => format!("{}#{reference}", hit.repository),
                None => hit.repository.to_string(),
            };
            let counted = match (hit.installs, hit.stars) {
                (Some(installs), _) => plural(count(installs), "install"),
                (None, Some(stars)) => plural(count(stars), "star"),
                (None, None) => String::new(),
            };
            let finders: Vec<&str> = hit.finders.iter().map(|finder| finder.name()).collect();
            vec![
                (at + 1).to_string(),
                printable(&hit.name),
                printable(&repository),
                printable(hit.folder()),
                counted,
                finders.join(", "),
            ]
        })
        .collect()
}

/// A count as a row says it, however large a finder's.
fn count(count: u64) -> usize {
    usize::try_from(count).unwrap_or(usize::MAX)
}
