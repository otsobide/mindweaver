//! The sources a workspace gathers from, kept once they are gathered (#100):
//! listed, called something else, gathered again, and forgotten.
//!
//! A gather registers a source ([`crate::gather`]); everything after that is
//! here, each by the name a source is called ([`SourceName`], D-13), which is
//! what `mindweaver gather sources` lists and every command here takes. What
//! each is listed with is the snapshot's own row of it
//! ([`ShelfSource::read`](crate::view::ShelfSource::read)), so a listing and
//! a front end reading the snapshot say one thing of it.
//!
//! Forgetting one takes away what a gather of it made, and nothing else: its
//! clone in the cache and its folder on each kind's shelf, by the folder name
//! the ledger records it under, and its rows. What was installed from it is
//! no folder of its, and stays where it is, recorded, orphaned now that
//! nothing offers it, for `uninstall --orphaned` to take out when somebody
//! asks ([`Removal::installed`]).

use std::fs;
use std::io;
use std::path::PathBuf;

use thiserror::Error;

use crate::code::codes;
use crate::gather::{self, GatherError, Report, Request};
use crate::install;
use crate::kind::Kind;
use crate::ledger::{Action, FromSource, Ledger, LedgerError, Outcome, Source};
use crate::lock::LockError;
use crate::url::{carries_credentials, redacted};
use crate::value::{Origin, SourceName};
use crate::view::KindCount;
use crate::workspace::WeaverWorkspace;

/// The source called `typed`, refused as one that carries a credential
/// before it is looked for, since every refusal repeats what was typed.
fn named(ledger: &Ledger, typed: &str) -> Result<Source, SourceError> {
    unsaid(typed)?;
    let found = match SourceName::parse(typed) {
        Some(name) => ledger.source_named(&name)?,
        // No source can be called what no name is, which is the answer
        // somebody who typed it needs, rather than the name's rule.
        None => None,
    };
    found.ok_or_else(|| SourceError::Unknown {
        name: typed.to_owned(),
    })
}

/// Refuse `typed` when it carries a credential, a URL typed where a name
/// goes, as a gather refuses one typed as its `--name`: by the words and the
/// code it does, the text said without what it carried.
fn unsaid(typed: &str) -> Result<(), SourceError> {
    if carries_credentials(typed) {
        return Err(SourceError::Credentials {
            text: redacted(typed).into_owned(),
        });
    }
    Ok(())
}

/// The sources `names` call, in the order given, each once, or every source
/// the ledger knows when `names` is empty, in the order it lists them: what
/// `mindweaver gather update` gathers again.
///
/// Refused whole, before anything is fetched, for a name no source has: a
/// name mistyped among several is somebody expecting it gathered, which a
/// command that gathered the rest and said so last would bury.
pub fn chosen(ledger: &Ledger, names: &[String]) -> Result<Vec<Source>, SourceError> {
    if names.is_empty() {
        return Ok(ledger.sources()?);
    }
    let mut chosen: Vec<Source> = Vec::with_capacity(names.len());
    for typed in names {
        let source = named(ledger, typed)?;
        if !chosen.iter().any(|seen| seen.id == source.id) {
            chosen.push(source);
        }
    }
    Ok(chosen)
}

/// What gathering a source again came to ([`update`]).
#[derive(Debug)]
pub enum Again {
    /// Gathered, and what the gather did, what it took off the shelf among it.
    Gathered(Report),
    /// It could not be, for this reason; its shelf and its row are as they
    /// were, as every failed gather leaves them.
    Failed(GatherError),
    /// Not tried, since one before it was cancelled: a cancel stops the
    /// update, not one source of it.
    Unreached,
}

/// One source an update gathered again, as it was before, and what came of
/// it.
#[derive(Debug)]
pub struct Regathered {
    pub source: Source,
    pub again: Again,
}

/// Gather each of `sources` again (`mindweaver gather update`), in turn, each
/// from what `request` makes of it: [`Request::again`] of it, followed by
/// the front end's watch and handed whatever token it reads for it, or why
/// it cannot be gathered, a token for a URL no token goes to among it.
///
/// One that fails is said beside the rest, which are gathered all the same,
/// as one skill that cannot be taken costs none beside it; one cancelled
/// stops the update there, and those after it are not tried. Each gather
/// holds the workspace for itself, as `gather git` does, so another command
/// can write between two of them, and none of them waits for the network of
/// all the others.
pub fn update(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    sources: Vec<Source>,
    mut request: impl FnMut(&Source) -> Result<Request, GatherError>,
) -> Vec<Regathered> {
    let mut cancelled = false;
    sources
        .into_iter()
        .map(|source| {
            if cancelled {
                return Regathered {
                    source,
                    again: Again::Unreached,
                };
            }
            let again = match request(&source)
                .and_then(|asked| gather::gather(workspace, ledger, &asked))
            {
                Ok(report) => Again::Gathered(report),
                Err(error) => {
                    cancelled = matches!(error, GatherError::Cancelled);
                    Again::Failed(error)
                }
            };
            Regathered { source, again }
        })
        .collect()
}

/// What renaming a source did: the name it had, and the source as it is
/// called now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renamed {
    pub was: SourceName,
    pub source: Source,
}

/// Call the source `from` `to` from now on (`mindweaver gather rename`), by
/// the rules `gather git --name` holds a name to, and refused as it refuses
/// one: what no source can be called, and a name another source has.
///
/// The origin each installation from it records is its URL, which a name
/// is not part of, so what was installed from it is still found there, and
/// `--from` takes the new name at once. Renamed to the name it has, it is
/// left as it is. The workspace is held while the name is changed, as a
/// gather that names its source holds it.
pub fn rename(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    from: &str,
    to: &str,
) -> Result<Renamed, SourceError> {
    unsaid(from)?;
    unsaid(to)?;
    let _lock = workspace.lock()?;
    let mut source = named(ledger, from)?;
    let name = SourceName::parse(to).ok_or_else(|| SourceError::NameInvalid {
        name: to.to_owned(),
    })?;
    let was = source.name.clone();
    if name == was {
        return Ok(Renamed { was, source });
    }
    if let Some(holder) = ledger.source_named(&name)? {
        return Err(SourceError::NameTaken {
            name,
            origin: holder.origin(),
        });
    }
    ledger.rename_source(source.id, &name)?;
    source.name = name;
    Ok(Renamed { was, source })
}

/// What forgetting a source takes away, and what it leaves orphaned: what
/// `mindweaver gather remove` says, without `--yes` as what it would do.
#[derive(Debug)]
pub struct Removal {
    pub source: Source,
    /// Its clone in the cache, when there is one to take away.
    pub clone: Option<PathBuf>,
    /// Its folder on each kind's shelf there is one of, in report order.
    pub shelves: Vec<(Kind, PathBuf)>,
    /// How many of its artifacts the shelf holds, of each kind it holds
    /// any of, in report order, as a listing of the sources counts them.
    pub shelved: Vec<KindCount>,
    /// Every installation of what it supplied, which is left where it is,
    /// recorded, and offered by nothing once it is forgotten.
    pub installed: Vec<FromSource>,
}

/// What forgetting the source called `name` would take away, and leave
/// orphaned, with nothing written: the dry run of [`remove`].
///
/// Refused as [`remove`] would refuse it, so a dry run that goes well is
/// one the removal follows: a source whose folder the ledger names as no
/// one folder, or whose folder in the cache or on a shelf is a link, where
/// a removal would reach somebody's folder through it.
pub fn removal(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    name: &str,
) -> Result<Removal, SourceError> {
    let source = named(ledger, name)?;
    planned(workspace, ledger, source)
}

/// What forgetting `source` takes away, each folder it takes held to being
/// the one the ledger records as the source's.
fn planned(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    source: Source,
) -> Result<Removal, SourceError> {
    // One folder name, so each folder is one level down from the cache and
    // from a kind's shelf, which is all a gather ever put there; anything
    // else was not written by one, and is no source's to take away.
    if !install::is_folder_name(&source.directory) {
        return Err(SourceError::NotOneFolder {
            name: source.name.clone(),
            directory: source.directory.clone(),
        });
    }
    let there = |path: PathBuf| -> Result<Option<PathBuf>, SourceError> {
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => Err(SourceError::Linked {
                name: source.name.clone(),
                path,
            }),
            Ok(_) => Ok(Some(path)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(SourceError::Unread {
                path,
                source: error,
            }),
        }
    };
    let clone = there(workspace.cache_dir().join(&source.directory))?;
    let mut shelves = Vec::new();
    for kind in Kind::ALL {
        if let Some(path) = there(workspace.gathered_dir(kind).join(&source.directory))? {
            shelves.push((kind, path));
        }
    }
    let gathered = ledger.gathered()?;
    let shelved = Kind::ALL
        .into_iter()
        .map(|kind| KindCount {
            kind,
            count: gathered
                .iter()
                .filter(|artifact| artifact.source.id == source.id && artifact.kind == kind)
                .count(),
        })
        .filter(|counted| counted.count > 0)
        .collect();
    let installed = ledger.installed_from(&source)?;
    Ok(Removal {
        source,
        clone,
        shelves,
        shelved,
        installed,
    })
}

/// Forget the source called `name` (`mindweaver gather remove --yes`): its
/// clone and its folder on each shelf taken away, then its rows, what it
/// gathered with it, and the line in the log that says so. What it hands
/// back is what was taken, as [`removal`] would have said it.
///
/// The workspace is held from before the source is read until its rows are
/// gone, since a gather of it beside this would be cloning into the folder
/// being taken away. The folders go before the rows, so a folder that will
/// not go leaves the source as it was, said, for the next removal to take
/// what is left of it: rows taken first would leave a folder no source
/// names, which the shelf, read from its folders, would offer as nobody's.
pub fn remove(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    name: &str,
) -> Result<Removal, SourceError> {
    unsaid(name)?;
    let _lock = workspace.lock()?;
    let removal = planned(workspace, ledger, named(ledger, name)?)?;
    let folders = removal
        .clone
        .iter()
        .chain(removal.shelves.iter().map(|(_, path)| path));
    for path in folders {
        match fs::remove_dir_all(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(SourceError::Unremoved {
                    name: removal.source.name.clone(),
                    path: path.clone(),
                    source: error,
                })
            }
        }
    }
    ledger.forget_source(removal.source.id)?;
    ledger.log(
        Action::Forget,
        Some(removal.source.url.as_str()),
        Outcome::Ok,
        Some(removal.source.name.as_str()),
    )?;
    Ok(removal)
}

/// Why a source could not be renamed, chosen or forgotten.
///
/// Each refusal a gather also makes is said in words of its own, about the
/// command that made it, and given the code the gather gives it: a script
/// reads one condition by one code, whichever command it typed.
#[derive(Debug, Error)]
pub enum SourceError {
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Lock(#[from] LockError),
    /// No source is called that.
    #[error("no source is called `{name}`: `mindweaver gather sources` lists them by name")]
    Unknown { name: String },
    /// What no source can be called ([`SourceName::parse`]).
    #[error(
        "`{name}` cannot name a source: it has to be letters, digits, `.`, `_` and `-`, not \
         starting with `-`, and neither `.`, `..` nor `workspace`"
    )]
    NameInvalid { name: String },
    /// Another source is called that already, at `origin`.
    #[error("`{name}` is the name of the source at {origin} already: give this one another")]
    NameTaken { name: SourceName, origin: Origin },
    /// A name typed with a password or a token in it, a URL where a name
    /// goes: said without it.
    #[error(
        "{text}: a password, a token or, over http(s), a user name is refused where a source's \
         name goes, because what is typed is said back: name the source as `mindweaver gather \
         sources` lists it"
    )]
    Credentials { text: String },
    /// The ledger records the source's folder as something no gather names
    /// one, a route or `..`: nothing of it is taken away, since what that
    /// leads to is no folder a gather made.
    #[error(
        "the source `{name}` is recorded as gathered into `{directory}`, which is not one folder \
         name, so nothing of it is removed"
    )]
    NotOneFolder { name: SourceName, directory: String },
    /// Its folder in the cache or on a shelf is a link, which a removal
    /// would have to reach through, to whatever it leads to.
    #[error(
        "{path}: a link where the source `{name}` keeps its folder, so nothing of it is removed"
    )]
    Linked { name: SourceName, path: PathBuf },
    /// Whether its folder is there could not be read.
    #[error("{path}: cannot be read: {source}")]
    Unread {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// A folder of it would not go: the source is kept, and what went with
    /// the folders before it, for a removal to take what is left.
    #[error("{path}: cannot be removed, so the source `{name}` is kept until it can: {source}")]
    Unremoved {
        name: SourceName,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

codes!(SourceError {
    Self::Unknown { .. } => "source_unknown",
    Self::NotOneFolder { .. } => "source_folder_unrecorded",
    Self::Linked { .. } => "source_folder_linked",
    Self::Unread { .. } => "source_folder_unread",
    Self::Unremoved { .. } => "source_unremoved",
} carried {
    Self::Ledger(error) => error,
    Self::Lock(error) => error,
} echoed {
    Self::NameInvalid { .. } => "source_name_invalid",
    Self::NameTaken { .. } => "source_name_taken",
    Self::Credentials { .. } => "credentials_in_url",
});

#[cfg(test)]
#[path = "sources_test.rs"]
mod tests;
