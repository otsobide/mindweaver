//! `mindweaver export`: the workspace's own written into a folder laid out as
//! a workspace of their own, as core plans and does it
//! ([`mindweaver_core::export`]). Here it is only said.
//!
//! No `mind` form: what it reads is the workspace's library, which `mind`
//! reaches nowhere, as `mindweaver adopt` has none.

use std::fmt::Write as _;
use std::path::Path;

use mindweaver_core::export::{self, Change, Plan};
use mindweaver_core::{Kind, WeaverWorkspace};

use crate::render::{indent, plural, printable, table};
use crate::{CliError, Failure, Outcome};

/// What a dry run ends with.
const UNEXPORTED: &str = "nothing exported: add --yes to go ahead\n";

/// `mindweaver export <into>`: the library's `kinds`, every kind when none,
/// written into `into`, or, when that would change or remove what is there
/// and `yes` was not given, what it would do, nothing written.
pub(crate) fn export_into(
    workspace: &WeaverWorkspace,
    into: &Path,
    kinds: &[Kind],
    yes: bool,
) -> Result<Outcome, Failure> {
    let exported = export::export(workspace, into, kinds, yes).map_err(CliError::from)?;
    let mut outcome = said(&exported.plan, exported.written);
    if !exported.written {
        outcome.stdout.push_str(UNEXPORTED);
    }
    Ok(outcome)
}

/// What `plan` does, or did once `done`: the folder, then what
/// [`listed`] says.
fn said(plan: &Plan, done: bool) -> Outcome {
    let mut stdout = String::new();
    let made = if plan.made {
        format!(", a workspace named {} made there", printable(&plan.name))
    } else {
        String::new()
    };
    let _ = writeln!(stdout, "{}{made}", plan.into.display());
    stdout.push_str(&listed(plan, done, "to export"));
    Outcome::plain(stdout)
}

/// A row for each artifact `plan` reaches, what moved first, as a gather
/// says it, and the count; or a line saying the library holds nothing
/// `for_what`. What `mindweaver backup` says of the export it commits too.
pub(crate) fn listed(plan: &Plan, done: bool, for_what: &str) -> String {
    let mut stdout = String::new();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for change in [
        Change::Added,
        Change::Updated,
        Change::Removed,
        Change::Unchanged,
    ] {
        for step in plan.artifacts.iter().filter(|step| step.change == change) {
            rows.push(vec![
                word(change, done).to_owned(),
                step.kind.slug().to_owned(),
                printable(step.name.as_ref()),
            ]);
        }
    }
    if rows.is_empty() {
        let _ = writeln!(stdout, "  nothing in the library {for_what}");
    } else {
        stdout.push_str(&indent(&table(&rows)));
        let counts: Vec<String> = [
            Change::Added,
            Change::Updated,
            Change::Removed,
            Change::Unchanged,
        ]
        .into_iter()
        .map(|change| format!("{} {}", plan.count(change), counted(change, done)))
        .collect();
        let _ = writeln!(
            stdout,
            "\n{}: {}",
            plural(plan.artifacts.len(), "artifact"),
            counts.join(", ")
        );
    }
    stdout
}

/// The words the count says `change` with: as a row says it once done, and
/// what is still to do in a dry run.
fn counted(change: Change, done: bool) -> &'static str {
    match (change, done) {
        (change, true) => change.done(),
        (Change::Added, false) => "to add",
        (Change::Updated, false) => "to update",
        (Change::Removed, false) => "to remove",
        (Change::Unchanged, false) => "unchanged",
    }
}

/// The word a row says `change` with: in the past tense once done, and as
/// what would be done in a dry run.
fn word(change: Change, done: bool) -> &'static str {
    match (change, done) {
        (change, true) => change.done(),
        (Change::Added, false) => "would add",
        (Change::Updated, false) => "would update",
        (Change::Removed, false) => "would remove",
        (Change::Unchanged, false) => "unchanged",
    }
}
