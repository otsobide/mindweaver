//! The commands that offer other workspaces' own skills to this one's
//! projects: `load` and `unload`, at either level.
//!
//! What a loaded workspace cannot offer is said wherever loads are read, by
//! `install` and by both screens too, so those warnings are built here once.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use mindweaver_core::{
    paths, resolve, resolve_except, Intent, Kind, Registration, WeaverWorkspace, WorkspaceError,
};

use super::{all_or_nothing, reading, targets};
use crate::render::{as_stored, counted_kind, printable, table};
use crate::{CliError, Failure, Lookup, Outcome, Warning};

/// `load`, at either level: the paths are typed from `directory`, and the
/// workspace they go into is resolved from `start` (the directory itself
/// for `mindweaver`, the project for `mind`). With no paths it lists what that
/// workspace loads, as the other reads do: of the default one before it is
/// made too, which loads nothing, rather than making it to say so.
pub(crate) fn load_command(
    directory: &Path,
    start: &Path,
    workspaces: &[PathBuf],
    lookup: &Lookup,
) -> Result<Outcome, Failure> {
    let home = lookup.home.as_deref();
    if workspaces.is_empty() {
        let workspace = resolve(&lookup.inputs(start, Intent::Read))?
            .map(|resolved| resolved.workspace)
            .ok_or_else(|| CliError::NowhereToLoad {
                from: start.to_path_buf(),
                verb: "list the loads of",
                named: false,
            })?;
        return reading(&workspace, |workspace| Ok(loaded_listing(workspace, home)));
    }
    let sources = targets(directory, workspaces)?;
    // Every source opened before anything is found or made: a typo must not
    // leave a default workspace behind in your home.
    let mut opened = Vec::new();
    let mut refused = Vec::new();
    for source in &sources {
        match WeaverWorkspace::open(source) {
            Ok(source) => opened.push(source),
            Err(error) => refused.push(CliError::from(error)),
        }
    }
    if let Some(failure) = all_or_nothing(refused) {
        return Err(failure);
    }
    // Passing over the ones being loaded, since a workspace cannot load
    // itself: from inside one, the load goes into the workspace above it.
    let mut workspace = resolve_except(&lookup.inputs(start, Intent::Write), &sources)?
        .map(|resolved| resolved.workspace)
        .ok_or_else(|| CliError::NowhereToLoad {
            from: start.to_path_buf(),
            verb: "load into",
            named: true,
        })?;
    // In one rewrite, refused whole when one source is the workspace itself:
    // one named with `-W` is not passed over for being among them.
    let mut text = String::new();
    for (source, (entry, registration)) in opened.iter().zip(workspace.load_all(&opened)?) {
        text.push_str(&loaded(&workspace, source.name(), &entry, registration));
    }
    text.push_str(&acted_on("into", &workspace, home));
    let stderr = load_warnings(&workspace);
    Ok(Outcome {
        stdout: text,
        ok: stderr.is_empty(),
        stderr,
    })
}

/// `unload`, at either level: all or nothing, in one rewrite of the marker,
/// and never making the default workspace only to find it loads nothing; it
/// is resolved as a read, and only a made workspace has anything to unload.
pub(crate) fn unload_command(
    directory: &Path,
    start: &Path,
    workspaces: &[PathBuf],
    lookup: &Lookup,
) -> Result<Outcome, Failure> {
    let home = lookup.home.as_deref();
    let sources = targets(directory, workspaces)?;
    let mut workspace = resolve_except(&lookup.inputs(start, Intent::Read), &sources)?
        .map(|resolved| resolved.workspace)
        .filter(WeaverWorkspace::is_made)
        .ok_or_else(|| CliError::NowhereToLoad {
            from: start.to_path_buf(),
            verb: "unload from",
            named: true,
        })?;
    let refused = sources
        .iter()
        .filter(|source| !workspace.is_loaded(source))
        .map(|source| {
            CliError::Workspace(WorkspaceError::NotLoaded {
                path: source.clone(),
                workspace: workspace.root().to_path_buf(),
            })
        })
        .collect();
    if let Some(failure) = all_or_nothing(refused) {
        return Err(failure);
    }
    let mut text = String::new();
    for entry in workspace.unload_all(&sources)? {
        let _ = writeln!(text, "unloaded {}", as_stored(&entry));
    }
    text.push_str(&acted_on("from", &workspace, home));
    Ok(Outcome::plain(text))
}

/// The line naming the workspace a load or an unload changed, because that is
/// not always the one you are standing in, and never when you are standing in
/// the one being loaded.
fn acted_on(preposition: &str, workspace: &WeaverWorkspace, home: Option<&Path>) -> String {
    let default = if workspace.is_default(home) {
        ", the default one"
    } else {
        ""
    };
    format!(
        "  {preposition} the workspace at {}{default}\n",
        workspace.root().display()
    )
}

/// `mindweaver load` on its own: each loaded workspace, what it offers or why it
/// cannot be read, and which workspace loads them.
fn loaded_listing(workspace: &WeaverWorkspace, home: Option<&Path>) -> Outcome {
    let loads = workspace.loads();
    if loads.is_empty() {
        return Outcome::plain(format!(
            "nothing loaded\n  offer another workspace's own with `mindweaver load <path>`\n{}",
            acted_on("in", workspace, home)
        ));
    }
    let rows: Vec<Vec<String>> = loads
        .iter()
        .enumerate()
        .map(|(index, load)| {
            let entry = as_stored(&load.entry);
            // Two spellings of one workspace are one source, offered once,
            // under the first.
            let earlier = loads[..index]
                .iter()
                .find(|earlier| same_directory(&earlier.root, &load.root));
            match (&load.workspace, earlier) {
                (Ok(source), Some(earlier)) => vec![
                    printable(&entry),
                    printable(source.name()),
                    format!("the same as {}", printable(&as_stored(&earlier.entry))),
                ],
                (Ok(source), None) => vec![
                    printable(&entry),
                    printable(source.name()),
                    offering(workspace, &entry),
                ],
                (Err(_), _) => vec![
                    printable(&entry),
                    String::from("-"),
                    String::from("cannot be opened"),
                ],
            }
        })
        .collect();
    let mut stdout = table(&rows);
    stdout.push_str(&acted_on("in", workspace, home));
    let stderr = load_warnings(workspace);
    Outcome {
        ok: stderr.is_empty(),
        stdout,
        stderr,
    }
}

/// Whether two paths name one directory: as written, or on disk.
pub(crate) fn same_directory(a: &Path, b: &Path) -> bool {
    paths::normalize(a) == paths::normalize(b)
        || matches!(
            (mindweaver_core::paths::canonical(a), mindweaver_core::paths::canonical(b)),
            (Ok(a), Ok(b)) if a == b
        )
}

/// What `load` says it did, with what the loaded workspace has to install.
fn loaded(
    workspace: &WeaverWorkspace,
    name: &str,
    entry: &Path,
    registration: Registration,
) -> String {
    let entry = as_stored(entry);
    match registration {
        Registration::Added => format!(
            "loaded {name} as {entry}: {}\n",
            offering(workspace, &entry)
        ),
        Registration::AlreadyRegistered => format!("{name} is already loaded as {entry}\n"),
    }
}

/// "2 skills to install", or that there is nothing yet, counted from what
/// the install machinery itself offers through that entry, so the number is
/// what `mindweaver install` will show.
fn offering(workspace: &WeaverWorkspace, entry: &str) -> String {
    use mindweaver_core::install::{offered_by_loads, Offer};
    let offers = offered_by_loads(workspace).offers;
    let counts: Vec<String> = Kind::ALL
        .into_iter()
        .filter_map(|kind| {
            let count = offers
                .iter()
                .filter(|offer| {
                    offer.kind() == kind
                        && matches!(offer, Offer::Loaded { entry: from, .. } if from == entry)
                })
                .count();
            (count > 0).then(|| counted_kind(count, kind))
        })
        .collect();
    if counts.is_empty() {
        String::from("nothing to install yet")
    } else {
        format!("{} to install", counts.join(" and "))
    }
}

/// What the workspaces `workspace` loads could not offer, as `warning:` lines.
pub(crate) fn load_warnings(workspace: &WeaverWorkspace) -> Vec<Warning> {
    mindweaver_core::install::offered_by_loads(workspace)
        .failures
        .iter()
        .map(|failure| Warning::coded(failure.code(), format!("warning: {failure}")))
        .collect()
}
