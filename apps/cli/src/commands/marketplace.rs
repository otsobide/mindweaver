//! `mindweaver gather marketplace`: a Claude Code plugin marketplace's
//! catalog recorded and listed, and the plugins chosen of it gathered, each
//! as a source of its own (#103), through core's [`marketplace`].
//!
//! What a catalog lists, which of its plugins a gather takes, where each is
//! fetched from and what is never run are core's: this reads the token a
//! person names (D-16), asks first for `--all` past the bound, and lays out
//! what came of it.

use std::fmt::Write as _;
use std::path::Path;

use mindweaver_core::marketplace::{self, Entry, Extra, Gathered, Listing, Plugin, Request, Took};
use mindweaver_core::WeaverWorkspace;

use super::gather::{gathered as reported, handed, scrubbed, short};
use super::workspace_here;
use crate::progress::{Progress, Watch};
use crate::render::{indent, json_or_text, plural, printable, table};
use crate::{CliError, Credential, Failure, Lookup, Outcome, Warning};

/// What `gather marketplace` was asked: the marketplace, at which branch,
/// called what, which of its plugins, and where a token comes from.
pub(crate) struct Asked<'a> {
    pub source: &'a str,
    pub reference: Option<&'a str>,
    pub name: Option<&'a str>,
    /// `--plugin`, each once as typed.
    pub plugins: &'a [String],
    pub all: bool,
    pub yes: bool,
    pub json: bool,
    pub credential: &'a Credential,
}

impl Asked<'_> {
    /// What core is asked to gather, reporting to `watch`, with the token
    /// read for this run.
    fn request(&self, watch: &Watch) -> Result<Request, CliError> {
        let request = Request::listing(self.source)
            .at(self.reference)
            .named(self.name)
            .watched_by(watch.relaying(Progress::Gather))
            .with_token(handed(self.source, self.credential)?);
        Ok(if !self.plugins.is_empty() {
            request.plugins(self.plugins.to_vec())
        } else if self.all {
            request.all(self.yes)
        } else {
            request
        })
    }
}

/// `mindweaver gather marketplace`: its catalog recorded and listed with
/// `--list`, and with `--plugin` or `--all` each plugin chosen gathered,
/// said as `gather git` says a gather, one after the other.
///
/// `--all` without `--yes` reads the catalog first with nothing written,
/// the ledger only read, and stops there when core says it asks first
/// ([`marketplace::Catalog::asks_first`]): a dry run writes nothing, its
/// workspace's ledger not even made.
pub(crate) fn run_marketplace(
    asked: &Asked<'_>,
    directory: &Path,
    lookup: &Lookup,
    watch: &Watch,
) -> Result<Outcome, Failure> {
    // Found as a gather finds it, the read `--all` asks first among it,
    // which fetches into the workspace's scratch folder.
    let workspace = workspace_here(directory, lookup)?;
    let request = asked.request(watch)?;
    if asked.all && !asked.yes {
        let ledger = workspace.ledger_for_reading().map_err(CliError::from)?;
        let listing = marketplace::read(&workspace, &ledger, &request).map_err(CliError::from)?;
        if listing.catalog.asks_first() {
            return Ok(unconfirmed(&listing));
        }
    }
    gather(&workspace, &request, asked.json)
}

/// The gather itself, with the ledger opened to write, what its scrub took
/// out said first.
fn gather(workspace: &WeaverWorkspace, request: &Request, json: bool) -> Result<Outcome, Failure> {
    let (ledger, scrub) = workspace.ledger_to_write().map_err(CliError::from)?;
    let said = scrubbed(&scrub);
    match marketplace::gather(workspace, &ledger, request) {
        Ok(gathered) => {
            let text = marketplace_said(&gathered);
            let outcome = json_or_text(json.then_some(&gathered.listing), text);
            Ok(said.before(outcome))
        }
        Err(error) => Err(Failure {
            error: Box::new(error.into()),
            warnings: said.lines(),
        }),
    }
}

/// What a gather of a marketplace did: its catalog listed when no plugin
/// was chosen, and otherwise the line it was read at, then what each plugin
/// chosen came to, as `gather git` reports a gather, one after the other.
///
/// Also what `gather update` says of a marketplace it gathered again.
pub(crate) fn marketplace_said(gathered: &Gathered) -> Outcome {
    let listing = &gathered.listing;
    if gathered.unconfirmed {
        return unconfirmed(listing);
    }
    let mut outcome = Outcome::plain(String::new());
    outcome.stderr.extend(unread(listing));
    if gathered.plugins.is_empty() {
        outcome.stdout = listed(listing);
        outcome.stderr.extend(helpers(&listing.catalog.entries));
        outcome.ok = outcome.stderr.iter().all(|line| line.code.is_none());
        return outcome;
    }
    outcome.stdout = format!(
        "{}, {} listed\n",
        heading(listing).trim_end(),
        plural(listing.catalog.entries.len(), "plugin")
    );
    for plugin in &gathered.plugins {
        said_of(plugin, &mut outcome);
    }
    outcome.ok = outcome.ok && outcome.stderr.iter().all(|line| line.code.is_none());
    outcome
}

/// What came of `plugin`, added to `outcome`: its gather's report, as
/// `gather git` says one, after a blank line, and on stderr why it was not
/// gathered and what of it was left.
fn said_of(plugin: &Plugin, outcome: &mut Outcome) {
    let called = plugin
        .name
        .as_ref()
        .map_or_else(|| printable(&plugin.entry), |name| printable(name));
    match &plugin.took {
        Took::Gathered(report) => {
            let url = plugin.url.as_ref().map_or("", |url| url.as_str());
            let said = reported(url, report);
            outcome.stdout.push('\n');
            outcome.stdout.push_str(&said.stdout);
            outcome.stderr.extend(said.stderr);
            outcome.ok &= said.ok;
            if !plugin.left.is_empty() {
                outcome.stderr.push(Warning::from(format!(
                    "note: {}: its {} are not gathered: only a plugin's skills are",
                    printable(&plugin.entry),
                    extras(&plugin.left),
                )));
            }
        }
        Took::Failed(error) => {
            outcome.stderr.push(Warning::coded(
                error.code(),
                format!("warning: {called}: {}", printable(&error.to_string())),
            ));
            outcome.ok = false;
        }
        // Said as what it is, and failing the gather only when it was
        // named: `--all` passes over what no gather takes.
        Took::NotGathered(why) if plugin.fails() => {
            outcome.stderr.push(Warning::coded(
                why.code(),
                format!("warning: {}", printable(&why.to_string())),
            ));
            outcome.ok = false;
        }
        Took::NotGathered(why) => outcome.stderr.push(Warning::from(format!(
            "note: {}",
            printable(&why.to_string())
        ))),
        // A note: the cancel that stopped it is the warning.
        Took::Unreached => outcome.stderr.push(Warning::from(format!(
            "note: {}: not gathered, since the gather was cancelled",
            printable(&plugin.entry)
        ))),
    }
    if plugin.unrun {
        outcome.stderr.push(Warning::from(helper(&plugin.entry)));
    }
}

/// What `--all` says of a catalog listing more plugins to gather than it
/// gathers unasked: the listing, and what it would take, nothing written.
fn unconfirmed(listing: &Listing) -> Outcome {
    let mut stdout = listed(listing);
    let _ = writeln!(
        stdout,
        "{} to gather, more than {} at once: nothing gathered: add --yes to gather every one, or \
         name the ones wanted with --plugin",
        plural(listing.catalog.gatherable(), "plugin"),
        marketplace::ASKED_BEYOND
    );
    let mut stderr = unread(listing);
    stderr.extend(helpers(&listing.catalog.entries));
    Outcome {
        ok: stderr.iter().all(|line| line.code.is_none()),
        stdout,
        stderr,
    }
}

/// A catalog's plugins, a row each in the order it lists them, under the
/// line naming the marketplace: what each is called, where it is, the
/// commit it is pinned to and whether it is gathered, each of those two a
/// column only once a plugin has one, then what it says it does; and how
/// many there are.
fn listed(listing: &Listing) -> String {
    let mut stdout = heading(listing);
    let entries = &listing.catalog.entries;
    if entries.is_empty() {
        stdout.push_str("  no plugins listed\n");
        return stdout;
    }
    let pinned = entries.iter().any(|entry| entry.source.sha().is_some());
    let standing = entries
        .iter()
        .any(|entry| entry.gathered.is_some() || !entry.gatherable);
    let rows: Vec<Vec<String>> = entries
        .iter()
        .map(|entry| {
            let mut row = vec![printable(&entry.name), printable(&entry.source.said())];
            if pinned {
                row.push(
                    entry
                        .source
                        .sha()
                        .map_or_else(String::new, |pin| format!("pinned {}", short(pin))),
                );
            }
            if standing {
                row.push(stands(entry));
            }
            row.push(printable(entry.description.as_deref().unwrap_or("")));
            row
        })
        .collect();
    stdout.push_str(&indent(&table(&rows)));
    let gathered = entries
        .iter()
        .filter(|entry| entry.gathered.is_some())
        .count();
    let _ = writeln!(
        stdout,
        "\n{} listed, {}",
        plural(entries.len(), "plugin"),
        match gathered {
            0 => String::from("none gathered"),
            gathered => format!("{gathered} of them gathered"),
        }
    );
    stdout
}

/// Whether `entry` is gathered, by the source it is, or why no gather
/// takes it: blank for one a gather takes that none has yet.
fn stands(entry: &Entry) -> String {
    match &entry.gathered {
        Some(name) => format!("gathered as {}", printable(name)),
        None if entry.gatherable => String::new(),
        None => String::from("not gathered"),
    }
}

/// The line a listing starts with: the marketplace by its name and the URL
/// it was read from, and the revision it was read at.
fn heading(listing: &Listing) -> String {
    let at = listing
        .revision
        .as_deref()
        .map(short)
        .map_or_else(String::new, |revision| format!(" at {revision}"));
    format!(
        "{}: {}{at}\n",
        printable(&listing.name),
        printable(listing.url.as_str())
    )
}

/// Each plugin of `listing`'s catalog that could not be read, a warning of
/// its code.
fn unread(listing: &Listing) -> Vec<Warning> {
    listing
        .catalog
        .failures
        .iter()
        .map(|failure| {
            Warning::coded(
                failure.code(),
                format!("warning: {}", printable(&failure.to_string())),
            )
        })
        .collect()
}

/// A note for each of `entries` naming a `headersHelper`, which is never
/// run.
fn helpers(entries: &[Entry]) -> Vec<Warning> {
    entries
        .iter()
        .filter(|entry| entry.headers_helper)
        .map(|entry| Warning::from(helper(&entry.name)))
        .collect()
}

/// What is said of the plugin `entry` naming a `headersHelper`.
fn helper(entry: &str) -> String {
    format!(
        "note: {}: its catalog entry names a headersHelper, a command Mindweaver never runs",
        printable(entry)
    )
}

/// `left` as a sentence names them: `agents`, `agents and hooks`.
fn extras(left: &[Extra]) -> String {
    let said: Vec<&str> = left.iter().map(|extra| extra.said()).collect();
    match said.as_slice() {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}
