//! `mindweaver gather`: collecting artifacts onto the workspace's shelf, and
//! listing everything that can be installed from elsewhere.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use mindweaver_core::gather::{self, Report, Request, Scrub};
use mindweaver_core::install::{is_installable_name, offered_by_loads, Offer};
use mindweaver_core::port::Token;
use mindweaver_core::sources::{self, Again, Regathered, Removal, Renamed};
use mindweaver_core::url::redacted;
use mindweaver_core::view::ShelfSource;
use mindweaver_core::{Kind, Ledger, Licence, WeaverWorkspace, DEFAULT_SUBDIRECTORY};

use super::{reading_workspace, workspace_here};
use crate::progress::{Progress, Watch};
use crate::render::{counted_kind, indent, json_or_text, printable, table, utc};
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
            kind,
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
            // Each only when given, so core says what a gather of a kind
            // looks in when no folder is named, and which kinds have none.
            let mut request = Request::git(url.clone());
            if let Some(kind) = kind {
                request = request.taking(*kind);
            }
            if let Some(path) = path {
                request = request.from_subdirectory(path.clone());
            }
            let request = request
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
        GatherCommand::Sources { json } => {
            let mut found = None;
            let text = reading_workspace(directory, lookup, |workspace| {
                let ledger = workspace.ledger_for_reading().map_err(CliError::from)?;
                let listed = ShelfSource::read(&ledger).map_err(CliError::from)?;
                let stdout = sources_listing(&listed);
                found = Some(listed);
                Ok(Outcome::plain(stdout))
            })?;
            Ok(json_or_text(found.filter(|_| *json), text))
        }
        // Every source when none is named, which the grammar lets through
        // only with `--all`.
        GatherCommand::Update {
            names, credential, ..
        } => writing(directory, lookup, |workspace, ledger| {
            regathered(workspace, ledger, names, credential, watch)
        }),
        GatherCommand::Rename { from, to } => writing(directory, lookup, |workspace, ledger| {
            let renamed = sources::rename(workspace, ledger, from, to)?;
            Ok(Outcome::plain(renamed_said(&renamed)))
        }),
        // The dry run is a read, which makes no workspace and no ledger, as
        // `gather list` makes none.
        GatherCommand::Remove { name, yes: false } => {
            reading_workspace(directory, lookup, |workspace| {
                let ledger = workspace.ledger_for_reading().map_err(CliError::from)?;
                let removal = sources::removal(workspace, &ledger, name).map_err(CliError::from)?;
                Ok(removal_said(&removal, false))
            })
        }
        GatherCommand::Remove { name, yes: true } => {
            writing(directory, lookup, |workspace, ledger| {
                Ok(removal_said(
                    &sources::remove(workspace, ledger, name)?,
                    true,
                ))
            })
        }
        GatherCommand::Search {
            query,
            finders,
            at,
            take,
            json,
            credential,
        } => {
            let asked = super::search::Asked {
                query,
                finders,
                at,
                credential,
            };
            match take {
                Some(number) => super::search::run_take(&asked, *number, directory, lookup, watch),
                None => super::search::run_search(&asked, *json, directory, lookup, watch),
            }
        }
        GatherCommand::Mcp(command) => super::registry::run_registry(command, directory, lookup),
    }
}

/// Run `change` on the workspace a command that writes acts on, with its
/// ledger opened to write, saying what the scrub that opening runs took out
/// before whatever `change` says, or beside why it failed.
fn writing(
    directory: &Path,
    lookup: &Lookup,
    change: impl FnOnce(&WeaverWorkspace, &Ledger) -> Result<Outcome, CliError>,
) -> Result<Outcome, Failure> {
    let workspace = workspace_here(directory, lookup)?;
    let (ledger, scrub) = workspace.ledger_to_write().map_err(CliError::from)?;
    let said = scrubbed(&scrub);
    match change(&workspace, &ledger) {
        Ok(outcome) => Ok(said.before(outcome)),
        Err(error) => Err(Failure {
            error: Box::new(error),
            warnings: said.lines(),
        }),
    }
}

/// `mindweaver gather update`: each source `names` names, or every one when
/// it names none, gathered again, and what each did, said as `gather git`
/// says it, one after the other.
///
/// The token `credential` asks for is read before anything is gathered, so
/// a variable holding none, or a `gh` that has none, is said before a
/// fetch, as `gather git` says it: the variable once for every source, and
/// `gh` once for each host a token goes to. A source no token goes to, one
/// reached over ssh or a path, is refused it, as a gather of it would be,
/// and the rest are gathered.
fn regathered(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    names: &[String],
    credential: &Credential,
    watch: &Watch,
) -> Result<Outcome, CliError> {
    let chosen = sources::chosen(ledger, names)?;
    if chosen.is_empty() {
        return Ok(Outcome::plain(String::from(
            "no sources yet\n  take some with `mindweaver gather git <url>`\n",
        )));
    }
    let tokens = tokens_for(&chosen, credential)?;
    let again = sources::update(workspace, ledger, chosen, |source, kind| {
        let token = match tokens.get(&source.id) {
            Some(token) => Some(token.clone()),
            // None goes to it: why, as a gather handed one would say it.
            None if credential.gh => {
                gather::token_host(source.url.as_str())?;
                None
            }
            None => None,
        };
        Ok(Request::again(source, kind)
            .watched_by(watch.relaying(Progress::Gather))
            .with_token(token))
    })?;

    let mut outcome = Outcome::plain(String::new());
    for Regathered { source, again, .. } in again {
        match again {
            Again::Gathered(report) => {
                let said = gathered(source.url.as_str(), &report);
                // A blank line between two sources' reports, as between two
                // commands' in a shell.
                if !outcome.stdout.is_empty() {
                    outcome.stdout.push('\n');
                }
                outcome.stdout.push_str(&said.stdout);
                outcome.stderr.extend(said.stderr);
                outcome.ok &= said.ok;
            }
            Again::Failed(error) => {
                outcome.stderr.push(Warning::coded(
                    error.code(),
                    format!("warning: {}: {}", printable(&source.name), error),
                ));
                outcome.ok = false;
            }
            // A note: the cancel that stopped it is the warning.
            Again::Unreached => outcome.stderr.push(Warning::from(format!(
                "note: {}: not gathered, since the update was cancelled",
                printable(&source.name)
            ))),
        }
    }
    Ok(outcome)
}

/// The token each of `chosen` is handed, by its row id, as `credential`
/// says: the one in the variable `--token-env` names, read once, or what
/// `gh` has for the host core says a token for it goes to, asked once a
/// host. None for a source no token goes to, nor when neither is given.
fn tokens_for(
    chosen: &[mindweaver_core::ledger::Source],
    credential: &Credential,
) -> Result<HashMap<i64, Token>, CliError> {
    let mut tokens = HashMap::new();
    if let Some(name) = &credential.token_env {
        let token = token::from_variable(name)?;
        for source in chosen {
            tokens.insert(source.id, token.clone());
        }
    } else if credential.gh {
        let mut asked: HashMap<String, Token> = HashMap::new();
        for source in chosen {
            // Refused for it when it is gathered, which says why.
            let Ok(host) = gather::token_host(source.url.as_str()) else {
                continue;
            };
            let token = match asked.get(&host) {
                Some(token) => token.clone(),
                None => {
                    let token = token::from_gh(&host)?;
                    asked.insert(host, token.clone());
                    token
                }
            };
            tokens.insert(source.id, token);
        }
    }
    Ok(tokens)
}

/// What renaming a source did, by the name it had and the name it has, and
/// the URL it is gathered from, which is what an install from it recorded.
fn renamed_said(renamed: &Renamed) -> String {
    let source = &renamed.source;
    if renamed.was == source.name {
        return format!(
            "{}: called that already, gathered from {}\n",
            printable(&source.name),
            source.url
        );
    }
    format!(
        "renamed source {} to {}, gathered from {}\n",
        printable(&renamed.was),
        printable(&source.name),
        source.url
    )
}

/// What forgetting a source took away, or without `yes` would, and what it
/// leaves installed, orphaned, one line each, as `adopt` and `remove
/// --everywhere` say theirs.
///
/// What it leaves is said as a note once it is done, not a warning: it is
/// what forgetting a source does, which the dry run said, and taking it out
/// is a command of its own.
fn removal_said(removal: &Removal, yes: bool) -> Outcome {
    let (forget, remove, leave) = if yes {
        ("forgot", "removed", "left")
    } else {
        ("would forget", "would remove", "would leave")
    };
    let source = &removal.source;
    let mut stdout = format!(
        "{forget} the source {}, gathered from {}\n",
        printable(&source.name),
        source.url
    );
    for (kind, path) in &removal.shelves {
        let held = removal
            .shelved
            .iter()
            .find(|counted| counted.kind == *kind)
            .map_or(0, |counted| counted.count);
        let what = match held {
            0 => String::from("its folder"),
            held => format!("its {}", counted_kind(held, *kind)),
        };
        let _ = writeln!(
            stdout,
            "{remove} {what} from the shelf, at {}",
            path.display()
        );
    }
    if let Some(clone) = &removal.clone {
        let _ = writeln!(stdout, "{remove} its clone, at {}", clone.display());
    }
    for installed in &removal.installed {
        let _ = writeln!(
            stdout,
            "{leave} {} installed in {}, orphaned",
            printable(&installed.name),
            printable(&installed.project)
        );
    }
    let mut stderr = Vec::new();
    if !yes {
        stdout.push_str("nothing removed: add --yes to go ahead\n");
    } else if !removal.installed.is_empty() {
        stderr.push(Warning::from(String::from(
            "note: nothing offers what was installed from it any more; `mindweaver uninstall \
             --orphaned --all-projects` takes out what nothing offers",
        )));
    }
    Outcome {
        stdout,
        stderr,
        ok: true,
    }
}

/// Every source the shelf is filled from, one a line, as `--from` takes it
/// first: its name, then its URL, and its branch, its folder and its pin,
/// each a column that earns its place only once a source listed has one,
/// as `gather list` lays out its pin, then how much of it the shelf holds
/// and when it was last gathered, in UTC, which reads the same wherever the
/// listing is read.
fn sources_listing(listed: &[ShelfSource]) -> String {
    if listed.is_empty() {
        return String::from("no sources yet\n  take some with `mindweaver gather git <url>`\n");
    }
    let named_reference = listed.iter().any(|one| one.source.reference.is_some());
    let named_path = listed
        .iter()
        .any(|one| one.source.subdirectory != DEFAULT_SUBDIRECTORY);
    let named_pin = listed.iter().any(|one| one.source.pin.is_some());
    let rows: Vec<Vec<String>> = listed
        .iter()
        .map(|one| {
            let source = &one.source;
            let mut row = vec![printable(&source.name), printable(&source.url)];
            if named_reference {
                row.push(
                    source
                        .reference
                        .as_deref()
                        .map_or_else(String::new, |at| format!("ref {}", printable(at))),
                );
            }
            if named_path {
                row.push(match source.subdirectory.as_str() {
                    DEFAULT_SUBDIRECTORY => String::new(),
                    path => format!("path {}", printable(path)),
                });
            }
            if named_pin {
                row.push(
                    source
                        .pin
                        .as_ref()
                        .map_or_else(String::new, |pin| format!("pinned {}", short(pin))),
                );
            }
            let held: Vec<String> = one
                .shelved
                .iter()
                .map(|counted| counted_kind(counted.count, counted.kind))
                .collect();
            if held.is_empty() {
                row.push(String::from("nothing on the shelf"));
            } else {
                row.push(held.join(", "));
            }
            row.push(
                one.gathered_at
                    .map_or_else(String::new, |at| format!("gathered {}", utc(at))),
            );
            row
        })
        .collect();
    table(&rows)
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
        // Listed only if it could be installed: a name no copy can be filed
        // under never can be, wherever it came from. A skill's is one
        // folder name, a rule's a route held to the name rules, as core
        // says of each kind.
        if is_installable_name(gathered.kind, &gathered.name) {
            offers.push(Offer::Shelf(gathered));
        } else {
            let unlike = match gathered.kind {
                Kind::Skill => String::from("which is not one folder name"),
                Kind::Rule | Kind::Mcp => {
                    format!("which no {} can be installed under", gathered.kind.noun())
                }
            };
            stderr.push(
                format!(
                    "warning: {}: declares the name `{}`, {unlike}, so it is not offered",
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

    // Added first, then updated, then removed, then unchanged: a second
    // gather is run to find out what moved, and what moved should not be
    // at the bottom.
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (label, harvested) in [
        ("added", &report.added),
        ("updated", &report.updated),
        ("removed", &report.removed),
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
            "\n{}: {} added, {} updated, {} unchanged{}",
            counted_kind(report.total(), report.kind),
            report.added.len(),
            report.updated.len(),
            report.unchanged.len(),
            counted_removed(report, "removed")
        );
    }

    said(stdout, report, "was")
}

/// How many of its kind `report` took off the shelf, said after the rest of the
/// count, `removed` or `to remove`: nothing when none were, so a gather of
/// a source that dropped nothing reads as it did before one could.
fn counted_removed(report: &Report, removed: &str) -> String {
    match report.removed.len() {
        0 => String::new(),
        count => format!(", {count} {removed}"),
    }
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
        ("would remove", &report.removed),
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
            "\n{}: {} to add, {} to update, {} unchanged{}\n\
             nothing gathered: run it without --dry-run to gather",
            counted_kind(report.total(), report.kind),
            report.added.len(),
            report.updated.len(),
            report.unchanged.len(),
            counted_removed(report, "to remove")
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
