//! `mindweaver gather mcp`: an MCP registry searched for servers (#107),
//! and one of them written into the workspace's library as a definition.
//!
//! What a registry is asked, what of its answer is a server and how a
//! server is made a definition are core's ([`mcp_registry`]), and so is
//! what writing one is held to: the definition goes through `add`'s own
//! path ([`lifecycle::add`], [`Start::Definition`]), so a name beside
//! another, a file already where it goes and whatever `validate` would
//! refuse are judged as for every server added. This lays out what was
//! found and what would be written, and says what was not.
//!
//! Nothing here starts a server, nor asks anything of one: a definition
//! is only text until `install` gives it to a project's agents.

use std::fmt::Write as _;
use std::path::Path;

use mindweaver_core::create::Start;
use mindweaver_core::http::{Cache, Client};
use mindweaver_core::lifecycle;
use mindweaver_core::mcp::Runs;
use mindweaver_core::mcp_registry::{self, Entry, Imported, Listed, Listing, Registry, Summary};
use mindweaver_core::{Kind, WeaverWorkspace};

use super::artifact::{recorded, stale};
use super::gather::scrubbed;
use super::update::listed as words_listed;
use super::{reading_workspace, workspace_here};
use crate::render::{json_or_text, printable, table};
use crate::{CliError, Failure, Lookup, Outcome, RegistryCommand, Warning};

/// What a dry run ends with.
const UNWRITTEN: &str = "nothing written: add --yes to go ahead\n";

/// `mindweaver gather mcp search` and `mindweaver gather mcp add`.
///
/// A search is a read, and so is an add without `--yes`: neither makes the
/// workspace, and what the registry answers is kept in its cache only once
/// it is made. An add told yes writes, and makes it as any write does.
pub(crate) fn run_registry(
    command: &RegistryCommand,
    directory: &Path,
    lookup: &Lookup,
) -> Result<Outcome, Failure> {
    match command {
        RegistryCommand::Search {
            query,
            registry,
            json,
        } => {
            let mut found = None;
            let text = reading_workspace(directory, lookup, |workspace| {
                let listed = registry_at(registry, workspace)?.search(query);
                let (outcome, listing) = searched(listed)?;
                found = Some(listing);
                Ok(outcome)
            })?;
            Ok(json_or_text(found.filter(|_| *json), text))
        }
        RegistryCommand::Add {
            server,
            version,
            name,
            way,
            yes,
            registry,
        } => {
            let asked = Asked {
                server,
                version: version.as_deref(),
                name: name.as_deref(),
                way: way.map(|way| way as usize - 1),
                registry,
            };
            if *yes {
                let workspace = workspace_here(directory, lookup)?;
                write(&workspace, &asked)
            } else {
                reading_workspace(directory, lookup, |workspace| foresee(workspace, &asked))
            }
        }
    }
}

/// What `gather mcp add` was asked: the server by the registry's name, the
/// version, the name to write it under, the way, from 0, and the registry.
struct Asked<'a> {
    server: &'a str,
    version: Option<&'a str>,
    name: Option<&'a str>,
    way: Option<usize>,
    registry: &'a str,
}

/// The registry at `url`, read through a client keeping what it answers in
/// `workspace`'s cache once the workspace is made: a read makes nothing in
/// a home, and the cache's folder would be the first thing.
fn registry_at(url: &str, workspace: &WeaverWorkspace) -> Result<Registry, CliError> {
    let client = if workspace.is_made() {
        Client::new().caching(Cache::of(workspace, mcp_registry::FRESH_FOR))
    } else {
        Client::new()
    };
    Ok(Registry::at(url, client)?)
}

/// What a search found, a row each ([`rows`]), or that it found nothing,
/// and the listing, for `--json` to print in its place; what was read
/// past and why the listing stopped short each a warning with its code,
/// which fails the search, as a failure collected does.
///
/// Refused outright when it stopped before it read anything, a query too
/// short among it: there is no listing to show beside why.
fn searched(listed: Listed) -> Result<(Outcome, Listing), Failure> {
    let Listed {
        listing,
        failure,
        warnings,
        ..
    } = listed;
    let mut stderr: Vec<Warning> = warnings
        .iter()
        .map(|warning| Warning::coded(warning.code(), format!("warning: {warning}")))
        .collect();
    match failure {
        Some(failure) if listing.entries.is_empty() => {
            return Err(Failure {
                error: Box::new(failure.into()),
                warnings: stderr,
            });
        }
        Some(failure) => {
            stderr.push(Warning::coded(
                failure.code(),
                format!("warning: {failure}"),
            ));
        }
        None => {}
    }
    let servers: Vec<Summary> = listing.entries.iter().map(Entry::summary).collect();
    let stdout = if servers.is_empty() {
        format!(
            "the registry lists no server whose name holds `{}`\n",
            printable(&listing.query)
        )
    } else {
        table(&rows(&servers))
    };
    let ok = stderr.iter().all(|line| line.code.is_none());
    Ok((Outcome { stdout, stderr, ok }, listing))
}

/// Each server a search found as a row: the name the registry gives it,
/// which `gather mcp add` takes, its version, its status, and each way it
/// is started or reached by its word, in the registry's order, which
/// `--way` counts from 1. What the command prints and the TUI's results
/// draw, so the two say a server in the same words.
pub(crate) fn rows(servers: &[Summary]) -> Vec<Vec<String>> {
    servers
        .iter()
        .map(|server| {
            vec![
                printable(&server.name),
                printable(&server.version),
                printable(server.status.slug()),
                printable(&server.ways.join(", ")),
            ]
        })
        .collect()
}

/// `gather mcp add` without `--yes`: what would be written, and where,
/// held to every check the write makes before it writes, and nothing
/// written.
fn foresee(workspace: &WeaverWorkspace, asked: &Asked<'_>) -> Result<Outcome, Failure> {
    let (entry, imported, said) = imported(workspace, asked)?;
    let path = lifecycle::would_add(
        &workspace.own(),
        Kind::Mcp,
        &imported.name,
        &imported.definition.description,
        Start::Definition(&imported.text),
    )
    .map_err(|error| said.refusing(error.into()))?;
    let mut stdout = described(&entry, &imported);
    let _ = writeln!(
        stdout,
        "would create {} {} at {}",
        Kind::Mcp,
        imported.name,
        path.display()
    );
    stdout.push_str(UNWRITTEN);
    Ok(said.beside(stdout))
}

/// `gather mcp add --yes`: the definition written into the library through
/// `add`'s path, and what that dropped of an old record under its name.
fn write(workspace: &WeaverWorkspace, asked: &Asked<'_>) -> Result<Outcome, Failure> {
    let (entry, imported, said) = imported(workspace, asked)?;
    let added = lifecycle::add(
        Some(workspace),
        &workspace.own(),
        Kind::Mcp,
        &imported.name,
        &imported.definition.description,
        Start::Definition(&imported.text),
    )
    .map_err(|error| said.refusing(error.into()))?;
    let mut stdout = described(&entry, &imported);
    let _ = writeln!(
        stdout,
        "created {} {} at {}",
        added.artifact.kind(),
        added.artifact.name(),
        added.artifact.file().display()
    );
    let mut outcome = said.beside(stdout);
    recorded(&mut outcome, added.stale, stale);
    Ok(scrubbed(&added.scrub).before(outcome))
}

/// What is said beside a server added: what the cache could not do on the
/// way, which fails the command, as a warning collected does, and what the
/// import says of the definition it made, which fails nothing; each line
/// by the code of what it says.
struct Said {
    warned: Vec<Warning>,
    notes: Vec<Warning>,
}

impl Said {
    /// `stdout`, with what was said beside it.
    fn beside(self, stdout: String) -> Outcome {
        let ok = self.warned.is_empty();
        let mut stderr = self.warned;
        stderr.extend(self.notes);
        Outcome { stdout, stderr, ok }
    }

    /// A refusal for `error`, with what was said before it.
    fn refusing(&self, error: CliError) -> Failure {
        Failure {
            error: Box::new(error),
            warnings: self.warned.iter().chain(&self.notes).cloned().collect(),
        }
    }
}

/// The server `asked` names, from its registry, and the definition made of
/// it, with what was said on the way.
fn imported(
    workspace: &WeaverWorkspace,
    asked: &Asked<'_>,
) -> Result<(Entry, Imported, Said), Failure> {
    let registry = registry_at(asked.registry, workspace)?;
    let fetched = registry
        .server(asked.server, asked.version)
        .map_err(CliError::from)?;
    let mut said = Said {
        warned: fetched
            .warnings
            .iter()
            .map(|warning| Warning::coded(warning.code(), format!("warning: {warning}")))
            .collect(),
        notes: Vec::new(),
    };
    let entry = fetched.value;
    let imported = registry
        .import(&entry, asked.way, asked.name)
        .map_err(|error| said.refusing(error.into()))?;
    said.notes = imported
        .notes
        .iter()
        .map(|note| Warning::coded(note.code(), format!("note: {note}")))
        .collect();
    Ok((entry, imported, said))
}

/// What is said of a server before the line saying what was, or would be,
/// written: what the registry names it, its version and its status, the
/// way written when it lists more than one, what an agent would run or
/// reach as the definition writes it, and each variable it reads.
fn described(entry: &Entry, imported: &Imported) -> String {
    let mut text = format!(
        "{} {}, {}\n",
        printable(&entry.name),
        printable(&entry.version),
        printable(entry.status.slug())
    );
    let ways = entry.ways();
    if ways.len() > 1 {
        let others: Vec<String> = ways
            .iter()
            .enumerate()
            .filter(|(at, _)| *at != imported.way)
            .map(|(at, way)| format!("{} {}", at + 1, printable(way.word())))
            .collect();
        let _ = writeln!(
            text,
            "  way {} of {} ({}); --way writes another: {}",
            imported.way + 1,
            ways.len(),
            printable(ways[imported.way].word()),
            others.join(", ")
        );
    }
    let runs = Runs::of_transport(&imported.definition.transport);
    let _ = match &runs {
        Runs::Program { .. } => writeln!(text, "  runs: {}", printable(&runs.to_string())),
        Runs::Address { .. } => writeln!(text, "  reaches: {}", printable(&runs.to_string())),
    };
    let read: Vec<String> = imported
        .definition
        .inputs
        .iter()
        .filter_map(|input| {
            let variable = input.env_var.as_ref()?;
            Some(if input.secret {
                format!("${} (a secret)", printable(variable))
            } else {
                format!("${}", printable(variable))
            })
        })
        .collect();
    if !read.is_empty() {
        let read: Vec<&str> = read.iter().map(String::as_str).collect();
        let _ = writeln!(text, "  reads {} where the agents run", words_listed(&read));
    }
    text
}
