//! `mindweaver gather`: collecting artifacts onto the workspace's shelf, and
//! listing everything that can be installed from elsewhere.

use std::fmt::Write as _;
use std::path::Path;

use mindweaver_core::gather::{self, Report, Request, Scrub};
use mindweaver_core::install::{offered_by_loads, Offer};
use mindweaver_core::port::Token;
use mindweaver_core::url::redacted;
use mindweaver_core::{Kind, Licence, WeaverWorkspace};

use super::{reading_workspace, workspace_here};
use crate::progress::{Progress, Watch};
use crate::render::{indent, json_or_text, plural, printable, table};
use crate::{token, CliError, Credential, Failure, GatherCommand, Lookup, Outcome, Warning};

/// Collect artifacts onto the workspace shelf, or say what is already on it.
///
/// A gather reports to `watch` and stops when it is cancelled; what it then
/// prints is the same either way, so a front end that draws the progress
/// shows the same report afterwards as one that does not.
pub(crate) fn run_gather(
    command: &GatherCommand,
    directory: &Path,
    lookup: &Lookup,
    watch: &Watch,
) -> Result<Outcome, Failure> {
    match command {
        GatherCommand::Git {
            url,
            path,
            reference,
            name,
            pin,
            dry_run,
            credential,
        } => {
            // Found as a gather finds it, a dry run's too, which fetches into
            // the workspace's scratch folder and so needs one made.
            let workspace = workspace_here(directory, lookup)?;
            let request = Request::git(url.clone())
                .from_subdirectory(path.clone())
                .at(reference.clone())
                .named(name.clone())
                .pinned(pin.clone())
                .watched_by(watch.relaying(Progress::Gather))
                .with_token(handed(url, credential)?);
            if *dry_run {
                // A read of the ledger, which a workspace that never
                // gathered does not make, and no scrub: a dry run writes
                // nothing of the ledger's, and says nothing of it either.
                let ledger = workspace.ledger_for_reading().map_err(CliError::from)?;
                let report =
                    gather::dry_run(&workspace, &ledger, &request).map_err(CliError::from)?;
                return Ok(foreseen(url, &report));
            }
            let (ledger, scrub) = workspace.ledger_to_write().map_err(CliError::from)?;
            let said = scrubbed(&scrub);
            match gather::gather(&workspace, &ledger, &request) {
                Ok(report) => Ok(said.before(gathered(url, &report))),
                Err(error) => Err(Failure {
                    error: Box::new(error.into()),
                    warnings: said.lines(),
                }),
            }
        }
        GatherCommand::List { json } => {
            let mut found = None;
            let text =
                reading_workspace(directory, lookup, |workspace| shelf(workspace, &mut found))?;
            Ok(json_or_text(found.filter(|_| *json), text))
        }
    }
}

/// The token `credential` says a gather of `url` logs in with, read now,
/// for this run alone: the variable `--token-env` names, or what `gh` has
/// for the host core says the token goes to, which refuses a URL no token
/// goes over before `gh` is asked. `None` when neither is given, and the
/// gather logs in as git config's helpers and ssh do.
fn handed(url: &str, credential: &Credential) -> Result<Option<Token>, CliError> {
    if let Some(name) = &credential.token_env {
        return token::from_variable(name).map(Some);
    }
    if credential.gh {
        return token::from_gh(&gather::token_host(url)?).map(Some);
    }
    Ok(None)
}

/// `mindweaver gather list`: what can be installed from elsewhere, from a
/// ledger read without being made, with each offer kept in `found`, for
/// `--json` to print in place of the text. A workspace that has gathered
/// nothing has no database, and listing that is no reason to create one.
fn shelf(workspace: &WeaverWorkspace, found: &mut Option<Vec<Offer>>) -> Result<Outcome, Failure> {
    let ledger = workspace.ledger_for_reading().map_err(CliError::from)?;
    // What the workspaces it loads offer first, as the install screen
    // orders them; what they cannot offer is a warning beside the rest, not
    // a reason to show nothing.
    let loads = offered_by_loads(workspace);
    let stderr: Vec<Warning> = loads
        .failures
        .iter()
        .map(|failure| Warning::coded(failure.code(), format!("warning: {failure}")))
        .collect();
    let mut offers: Vec<Offer> = loads.offers;
    let mut stderr = stderr;
    for gathered in ledger.gathered().map_err(CliError::from)? {
        // Listed only if it could be installed: a name that is not one folder
        // name never can be, wherever it came from.
        if mindweaver_core::install::is_folder_name(&gathered.name) {
            offers.push(Offer::Shelf(gathered));
        } else {
            stderr.push(
                format!(
                    "warning: {}: declares the name `{}`, which is not one folder name, so it is not offered",
                    gathered.path, gathered.name
                )
                .into(),
            );
        }
    }
    let ok = stderr.is_empty();
    let stdout = shelf_listing(&offers, workspace.loads().is_empty());
    *found = Some(offers);
    Ok(Outcome { stdout, stderr, ok })
}

/// What a scrub of the ledger has to tell, for a command that writes to it.
///
/// Notes rather than warnings, for what it took out: the command did what it
/// was asked, and an exit code that failed once because of a fix would be a
/// script broken by nothing it did. What it could not take out is a warning,
/// since the credential is still on disk.
#[derive(Debug, Default, Clone)]
pub(crate) struct Scrubbed {
    notes: Vec<Warning>,
    warnings: Vec<Warning>,
}

impl Scrubbed {
    /// Every line, notes first, for a command that stopped before it had
    /// anything else to say: the scrub happened all the same, and says so
    /// only this once.
    pub(crate) fn lines(self) -> Vec<Warning> {
        let mut lines = self.notes;
        lines.extend(self.warnings);
        lines
    }

    /// Put in front of what the command went on to say.
    pub(crate) fn before(self, mut outcome: Outcome) -> Outcome {
        outcome.ok = outcome.ok && self.warnings.is_empty();
        outcome.stderr.splice(0..0, self.lines());
        outcome
    }
}

/// What the scrub that opening the ledger to write runs took out
/// ([`mindweaver_core::WeaverWorkspace::ledger_to_write`]), said the way a
/// person can act on: every source it scrubbed now needs its credential
/// handed over for the run, or from a git credential helper.
pub(crate) fn scrubbed(scrub: &Scrub) -> Scrubbed {
    // What the step to schema 7 made one first, which came first: the
    // spelling that went is what a script may still name it by.
    let mut notes: Vec<Warning> = scrub
        .merged
        .iter()
        .map(|merged| {
            let went = match &merged.reference {
                Some(reference) => format!("{}#{reference}", merged.url),
                None => merged.url.to_string(),
            };
            format!(
                "note: {went}: the same repository as the source `{name}` ({kept}), so the two \
                 are one source now, and the shelf keeps what `{name}` gathered",
                name = merged.into.name,
                kept = merged.into.origin(),
            )
            .into()
        })
        .collect();
    notes.extend(scrub.sources.iter().map(|source| {
        format!(
            "note: {}: the user name, password or token an older Mindweaver stored in this \
                 URL is taken out; gathering it again needs it from --token-env, --gh or a git \
                 credential helper",
            source.url
        )
        .into()
    }));
    match scrub.actions {
        0 => {}
        1 => notes.push(
            String::from(
                "note: 1 action log entry: the user name, password or token an older Mindweaver \
                 stored in a URL in it is taken out",
            )
            .into(),
        ),
        entries => notes.push(
            format!(
                "note: {entries} action log entries: the user names, passwords or tokens an \
                 older Mindweaver stored in URLs in them are taken out"
            )
            .into(),
        ),
    }
    Scrubbed {
        notes,
        warnings: scrub
            .failures
            .iter()
            .map(|failure| Warning::coded(failure.code(), format!("warning: {failure}")))
            .chain(
                scrub.leftovers.iter().map(|leftover| {
                    Warning::coded(leftover.code(), format!("warning: {leftover}"))
                }),
            )
            .collect(),
    }
}

/// What one gather did, in the order it is worth reading: the source, by
/// the name `--from` takes and the URL it was gathered from, then every
/// artifact and what happened to it, then the count.
///
/// The source is named without credentials, as everywhere a URL is shown.
/// A gather that got this far was given none, since core refuses them, so
/// this costs nothing today and still holds if that ever changes.
fn gathered(url: &str, report: &Report) -> Outcome {
    let mut stdout = heading(url, report);

    // Added first, then updated, then unchanged: a second gather is run to
    // find out what moved, and what moved should not be at the bottom.
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (label, harvested) in [
        ("added", &report.added),
        ("updated", &report.updated),
        ("unchanged", &report.unchanged),
    ] {
        for artifact in harvested {
            rows.push(vec![
                label.to_owned(),
                printable(&artifact.name),
                printable(artifact.summary.as_deref().unwrap_or("")),
            ]);
        }
    }
    if rows.is_empty() {
        stdout.push_str("  nothing to gather there\n");
    } else {
        stdout.push_str(&indent(&table(&rows)));
        let _ = writeln!(
            stdout,
            "\n{}: {} added, {} updated, {} unchanged",
            plural(report.total(), "skill"),
            report.added.len(),
            report.updated.len(),
            report.unchanged.len()
        );
    }

    said(stdout, report, "was")
}

/// What a gather's dry run found, laid out as the gather would report it,
/// with where each skill is in the source beside it, since that is what a
/// source of many layouts leaves to be asked, and what `--path` takes.
///
/// It exits as the gather would, on what could not be taken and on what is
/// wrong with what would be, so a script can ask before it gathers.
fn foreseen(url: &str, report: &Report) -> Outcome {
    let mut stdout = heading(url, report);

    let mut rows: Vec<Vec<String>> = Vec::new();
    for (label, harvested) in [
        ("would add", &report.added),
        ("would update", &report.updated),
        ("unchanged", &report.unchanged),
    ] {
        for artifact in harvested {
            rows.push(vec![
                label.to_owned(),
                printable(&artifact.name),
                printable(&artifact.route),
                printable(artifact.summary.as_deref().unwrap_or("")),
            ]);
        }
    }
    if rows.is_empty() {
        stdout.push_str("  nothing to gather there\n");
    } else {
        stdout.push_str(&indent(&table(&rows)));
        let _ = writeln!(
            stdout,
            "\n{}: {} to add, {} to update, {} unchanged\n\
             nothing gathered: run it without --dry-run to gather",
            plural(report.total(), "skill"),
            report.added.len(),
            report.updated.len(),
            report.unchanged.len()
        );
    }
    said(stdout, report, "would be")
}

/// The line a gather's report starts with: the source by its name and the
/// URL it was gathered from, the revision it was gathered at, and whether
/// that was a pin, which the source is pinned to from now on (#99).
fn heading(url: &str, report: &Report) -> String {
    let url = redacted(url);
    let at = report
        .revision
        .as_deref()
        .map(short)
        .map_or_else(String::new, |revision| format!(" at {revision}"));
    let pinned = if report.pin.is_some() { ", pinned" } else { "" };
    format!("{}: {url}{at}{pinned}\n", printable(&report.name))
}

/// `stdout`, with what `report` has to say beside it on stderr: what could
/// not be taken, what is wrong with what was, both warnings, and what the
/// shelf's copies were taken without, a note, which `was` says of a gather
/// and `would be` of a dry run.
fn said(stdout: String, report: &Report, was: &str) -> Outcome {
    // An artifact that could not be taken is a warning beside what was, not a
    // reason to be told nothing about the rest; so is what is wrong with one
    // that was taken anyway. Printable, because a warning quotes what the
    // source holds, a folder's name or what a skill declares.
    let mut stderr: Vec<Warning> = report
        .failures
        .iter()
        .map(|failure| {
            Warning::coded(
                failure.code(),
                format!("warning: {}", printable(&failure.to_string())),
            )
        })
        .chain(report.warnings.iter().map(|warning| {
            Warning::coded(
                warning.issue.code(),
                format!("warning: {}", printable(&warning.to_string())),
            )
        }))
        .collect();
    let ok = stderr.is_empty();
    // A note rather than a warning: leaving it out is what gathering does,
    // and nothing for whoever gathered to fix.
    stderr.extend(report.left_out.iter().map(|unshelved| {
        let left = &unshelved.left;
        let what = left.path.file_name().map_or_else(
            || left.path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        Warning::from(format!(
            "note: {} {was} gathered without its {what}, since {}",
            printable(&unshelved.name),
            left.reason
        ))
    }));
    Outcome { stdout, stderr, ok }
}

/// Everything that can be installed from elsewhere, with where each thing
/// comes from, spelled as `--from` takes it: a source by its name
/// ([`Offer::listed_from`]), and what it says of its licence, as a gather
/// read it ([`Licence::said`]), with nothing there for one a gather has not
/// read since it was kept (#99), nor for a workspace's own, which no gather
/// read.
///
/// A source's pin, when it has one, goes beside its name, the name being
/// what `--from` takes, so the pin is not written into it. Each of the two
/// is a column that earns its place, as the kind's does, only once
/// something listed has one: a listing of what loaded workspaces offer
/// reads as it did.
///
/// Empty, it says how to fill it, and suggests loading only when nothing is
/// loaded yet; `mindweaver load` on its own lists what is.
fn shelf_listing(shelf: &[Offer], nothing_loaded: bool) -> String {
    if shelf.is_empty() {
        return if nothing_loaded {
            String::from(
                "nothing gathered yet\n  take some with `mindweaver gather git <url>`, \
                 or offer a workspace's own with `mindweaver load <path>`\n",
            )
        } else {
            String::from(
                "nothing gathered yet, and the loaded workspaces offer nothing\n  \
                 take some with `mindweaver gather git <url>`; `mindweaver load` lists what is loaded\n",
            )
        };
    }

    // The same rule a catalog listing follows: a kind column earns its place
    // only once more than one kind is on the shelf.
    let mut kinds: Vec<Kind> = shelf.iter().map(|entry| entry.kind()).collect();
    kinds.sort();
    kinds.dedup();
    let name_kind = kinds.len() > 1;
    let pin = |entry: &Offer| match entry {
        Offer::Shelf(gathered) => gathered
            .source
            .pin
            .as_ref()
            .map(|pin| format!("pinned {}", short(pin))),
        Offer::Workspace { .. } | Offer::Loaded { .. } => None,
    };
    let name_pin = shelf.iter().any(|entry| pin(entry).is_some());
    fn licence(entry: &Offer) -> Option<&str> {
        match entry {
            Offer::Shelf(gathered) => gathered.licence.as_ref().map(Licence::said),
            Offer::Workspace { .. } | Offer::Loaded { .. } => None,
        }
    }
    let name_licence = shelf.iter().any(|entry| licence(entry).is_some());

    let rows: Vec<Vec<String>> = shelf
        .iter()
        .map(|entry| {
            let mut row = Vec::new();
            if name_kind {
                row.push(entry.kind().slug().to_owned());
            }
            row.push(printable(entry.name()));
            row.push(printable(&entry.listed_from()));
            if name_pin {
                row.push(pin(entry).unwrap_or_default());
            }
            if name_licence {
                row.push(printable(licence(entry).unwrap_or("")));
            }
            row.push(printable(entry.summary().unwrap_or("")));
            row
        })
        .collect();
    table(&rows)
}

/// A revision as a person refers to it.
fn short(revision: &str) -> &str {
    revision.get(..7).unwrap_or(revision)
}
