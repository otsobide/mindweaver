//! Writing the workspace's own artifacts into a folder of their own, laid out
//! as a workspace of their own: `mindweaver export`, and what
//! `mindweaver backup` commits.
//!
//! The folder is a workspace as `mindweaver init` makes one, named after the
//! one exported and linking nothing, its library holding the kinds asked
//! for. So the way back is one that exists already: `mindweaver load` of the
//! folder, or of a clone of the repository it was committed to, offers its
//! own to every project, and `mindweaver gather git --path
//! .mindweaver/library/skills` takes its skills. A format of its own would
//! have needed a reader of its own, and a second way for a skill to arrive.
//!
//! Each kind is the library's folder of it as it stands, copied whole
//! ([`copy::replace`]), whatever it holds besides artifacts, so a second
//! export mirrors the library: what was removed from it goes from the export
//! too. That takes away what an earlier export wrote, which somebody may
//! have changed since, so [`export`] does it only when told `yes`, and
//! otherwise hands back the [`Plan`] of what each artifact would come to,
//! written nowhere. Adding to a folder takes nothing, and needs no yes.
//!
//! A folder an export did not make is never written into blindly. One whose
//! workspace is in use, with a ledger, a project linked, a workspace loaded
//! or others known, is somebody's, and refused; so is one whose marker keeps
//! a kind anywhere but its library, which no export writes, a folder of the
//! workspace's own, where the export would copy into what it copies from,
//! and a link on the way into the library, which a clone of a repository
//! can hold and which would lead the replacement anywhere. An MCP server's
//! definition holding what looks like a token is refused too, and one that
//! does not read, where whether it holds one cannot be told: an export goes
//! wherever the folder goes, a public repository included, and a definition
//! holds no secret (D-11, in docs/architecture.md).
//!
//! It reads the workspace and writes only the folder, so it takes no lock:
//! a reader never waits, and nothing it writes is the workspace's.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use crate::artifact::ValidationIssue;
use crate::catalog::Catalog;
use crate::code::codes;
use crate::copy;
use crate::kind::{Kind, Layout};
use crate::names::Names;
use crate::value::ArtifactName;
use crate::workspace::{absolute, same_directory, Directories, WeaverWorkspace, WorkspaceError};

/// What exporting the library into a folder would do: [`plan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The folder exported into, absolute.
    pub into: PathBuf,
    /// Whether there is no workspace there yet, so one is made, named
    /// [`name`](Self::name).
    pub made: bool,
    /// The name of the workspace exported, which one made there is given.
    pub name: String,
    /// The kinds exported, in report order.
    pub kinds: Vec<Kind>,
    /// Each artifact of those kinds in the library or in the export, by kind
    /// in report order and then by name, with what the export does to it.
    pub artifacts: Vec<Step>,
}

impl Plan {
    /// Whether it changes or removes what is in the folder already, which
    /// [`export`] does only when told yes.
    pub fn takes_away(&self) -> bool {
        self.artifacts
            .iter()
            .any(|step| matches!(step.change, Change::Updated | Change::Removed))
    }

    /// How many artifacts come to `change`.
    pub fn count(&self, change: Change) -> usize {
        self.artifacts
            .iter()
            .filter(|step| step.change == change)
            .count()
    }
}

/// One artifact an export reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub kind: Kind,
    pub name: ArtifactName,
    pub change: Change,
}

/// What an export does to one artifact, compared with what the folder held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// In the library, and not in the folder.
    Added,
    /// In both, holding something else in the folder.
    Updated,
    /// In both, holding the same.
    Unchanged,
    /// In the folder, and no longer in the library.
    Removed,
}

impl Change {
    /// The word a report says it with, in the past tense.
    pub const fn done(self) -> &'static str {
        match self {
            Change::Added => "added",
            Change::Updated => "updated",
            Change::Unchanged => "unchanged",
            Change::Removed => "removed",
        }
    }
}

/// What [`export`] came to: its plan, and whether it was carried out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exported {
    pub plan: Plan,
    /// False when the plan takes something away and no yes was given:
    /// nothing was written.
    pub written: bool,
}

/// Why nothing was exported.
#[derive(Debug, Error)]
pub enum ExportError {
    #[error(
        "{} is inside a workspace's own folder, where nothing but that workspace writes",
        into.display()
    )]
    WorkspaceFolder { into: PathBuf },
    #[error(
        "{} is where this workspace keeps what it would export: export into a folder of its own",
        into.display()
    )]
    Itself { into: PathBuf },
    #[error("{}: not a folder", into.display())]
    NotAFolder { into: PathBuf },
    #[error(
        "{} is a workspace in use ({why}), not an export: export into an empty folder, or one an export made",
        into.display()
    )]
    InUse { into: PathBuf, why: &'static str },
    #[error(
        "{} keeps its {} in {}, where no export writes them: export into an empty folder, or one an export made",
        into.display(),
        kind.plural(),
        directory.display()
    )]
    Elsewhere {
        into: PathBuf,
        kind: Kind,
        directory: PathBuf,
    },
    #[error(
        "{} holds a {folder} with no marker in it, which no export leaves: move it out of the way first",
        into.display()
    )]
    Unmarked { into: PathBuf, folder: &'static str },
    #[error("{}: a link, and an export writes through none", path.display())]
    Linked { path: PathBuf },
    /// Where, and what it starts with, never the value, as `validate` says
    /// it: this goes wherever an error is read.
    #[error(
        "{}: `{at}` holds what looks like a token, starting `{prefix}`, which an export would carry \
         wherever it goes: declare a secret input read from an environment variable instead",
        path.display()
    )]
    Secret {
        path: PathBuf,
        at: String,
        prefix: String,
    },
    #[error(
        "{}: does not read as an MCP server's definition, so whether it holds a token cannot be \
         told: fix it (`mindweaver validate`) or move it out of the library",
        path.display()
    )]
    Unread { path: PathBuf },
    #[error("{}: cannot be read: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{}: cannot be written: {source}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
}

codes!(ExportError {
    Self::WorkspaceFolder { .. } => "export_into_workspace_folder",
    Self::Itself { .. } => "export_into_itself",
    Self::NotAFolder { .. } => "export_not_a_folder",
    Self::InUse { .. } => "export_into_workspace_in_use",
    Self::Elsewhere { .. } => "export_directory_elsewhere",
    Self::Unmarked { .. } => "export_into_unmarked",
    Self::Linked { .. } => "export_through_link",
    Self::Secret { .. } => "export_secret_literal",
    Self::Unread { .. } => "export_mcp_unread",
    Self::Read { .. } => "export_read_failed",
    Self::Write { .. } => "export_write_failed",
} carried {
    Self::Workspace(error) => error,
});

/// What exporting `workspace`'s own of `kinds` into `into` would do, nothing
/// written. No kind is every kind the workspace keeps its own of
/// ([`WeaverWorkspace::own_kinds`]).
pub fn plan(workspace: &WeaverWorkspace, into: &Path, kinds: &[Kind]) -> Result<Plan, ExportError> {
    plan_at(workspace, into, kinds, Place::Named)
}

/// Where an export writes, which decides whether it may be inside a
/// workspace's own folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    /// A folder somebody named, held outside every workspace's own.
    Named,
    /// A backup's check-out, in the workspace's scratch folder
    /// ([`crate::backup`]): inside its `.mindweaver` by design, made and
    /// taken away by the backup, and named by nobody.
    Scratch,
}

/// [`plan`], into a folder `place` says how to hold.
fn plan_at(
    workspace: &WeaverWorkspace,
    into: &Path,
    kinds: &[Kind],
    place: Place,
) -> Result<Plan, ExportError> {
    let into = absolute(into)?;
    refuse_place(workspace, &into, place)?;
    let target = target(&into)?;
    let names = target
        .as_ref()
        .map_or(Names::Mindweaver, WeaverWorkspace::names);
    let kinds = asked(workspace, kinds);

    let own = workspace.own();
    let mut artifacts = Vec::new();
    for &kind in &kinds {
        let to = into.join(Directories::library_under(names, kind));
        refuse_links(&into, &to)?;
        if let Some(target) = &target {
            let kept = target.own().directory_for(kind);
            if kept.as_deref() != Some(to.as_path()) {
                return Err(ExportError::Elsewhere {
                    into,
                    kind,
                    directory: kept.unwrap_or_default(),
                });
            }
        }
        let library = Catalog::discover_kinds(std::slice::from_ref(&own), &[kind]);
        if kind == Kind::Mcp {
            refuse_secrets(&library)?;
        }
        let exported = match &target {
            Some(target) => Catalog::discover_kinds(&[target.own()], &[kind]),
            None => Catalog::default(),
        };
        artifacts.extend(compared(kind, &library, &exported)?);
    }

    Ok(Plan {
        into,
        made: target.is_none(),
        name: workspace.name().to_owned(),
        kinds,
        artifacts,
    })
}

/// Export `workspace`'s own of `kinds` into `into`, as [`plan`] says, when
/// that takes nothing away or `yes` says to take it. Otherwise the plan,
/// nothing written.
///
/// `into` is made when it is not there, its parent never: a parent mistyped
/// is a reason to stop rather than a tree of folders to make.
pub fn export(
    workspace: &WeaverWorkspace,
    into: &Path,
    kinds: &[Kind],
    yes: bool,
) -> Result<Exported, ExportError> {
    export_at(workspace, into, kinds, yes, Place::Named)
}

/// [`export`], into a folder `place` says how to hold.
pub(crate) fn export_at(
    workspace: &WeaverWorkspace,
    into: &Path,
    kinds: &[Kind],
    yes: bool,
    place: Place,
) -> Result<Exported, ExportError> {
    let plan = plan_at(workspace, into, kinds, place)?;
    if plan.takes_away() && !yes {
        return Ok(Exported {
            plan,
            written: false,
        });
    }

    let into = &plan.into;
    if !into.exists() {
        fs::create_dir(into).map_err(|source| ExportError::Write {
            path: into.clone(),
            source,
        })?;
    }
    let names = if plan.made {
        WeaverWorkspace::init_named(into, &plan.name)?.0.names()
    } else {
        WeaverWorkspace::open(into)?.names()
    };
    let own = workspace.own();
    for &kind in &plan.kinds {
        let from = own.directory_for(kind).unwrap_or_default();
        let to = into.join(Directories::library_under(names, kind));
        if from.is_dir() {
            let library = to.parent().unwrap_or(into);
            fs::create_dir_all(library).map_err(|source| ExportError::Write {
                path: library.to_path_buf(),
                source,
            })?;
            // Links followed only inside the kind's folder, as an adoption
            // copies from the folder it adopts: one leading out of the
            // library is how a key in `~/.ssh` would reach a repository.
            copy::replace(&from, &to, &from).map_err(|source| ExportError::Write {
                path: to.clone(),
                source,
            })?;
        } else if to.exists() {
            fs::remove_dir_all(&to).map_err(|source| ExportError::Write {
                path: to.clone(),
                source,
            })?;
        }
    }

    Ok(Exported {
        plan,
        written: true,
    })
}

/// The kinds asked for that the workspace keeps its own of, in report order:
/// every one of those when none is asked for.
fn asked(workspace: &WeaverWorkspace, kinds: &[Kind]) -> Vec<Kind> {
    workspace
        .own_kinds()
        .into_iter()
        .filter(|kind| kinds.is_empty() || kinds.contains(kind))
        .collect()
}

/// Refuse a folder an export must not write into, whatever it holds: one
/// inside a workspace's own folder, under either names, and the workspace's
/// own root or a folder it keeps its own in, where the copy would be made
/// into what it is made from.
fn refuse_place(workspace: &WeaverWorkspace, into: &Path, place: Place) -> Result<(), ExportError> {
    let reserved = Names::ALL.map(Names::workspace_dir);
    let inside = into.components().any(|component| {
        matches!(component, Component::Normal(name) if reserved.iter().any(|dir| name == *dir))
    });
    if inside && place == Place::Named {
        return Err(ExportError::WorkspaceFolder {
            into: into.to_path_buf(),
        });
    }
    let own = workspace.own();
    let itself = same_directory(into, workspace.root())
        || Kind::ALL.into_iter().any(|kind| {
            own.directory_for(kind)
                .is_some_and(|folder| into.starts_with(&folder) || same_directory(into, &folder))
        });
    if itself {
        return Err(ExportError::Itself {
            into: into.to_path_buf(),
        });
    }
    Ok(())
}

/// The workspace already in `into`, if any, refused when it is one in use;
/// `into` refused when it is a file, or holds a workspace's folder with no
/// marker in it, whose library would be written over unseen.
fn target(into: &Path) -> Result<Option<WeaverWorkspace>, ExportError> {
    match fs::metadata(into) {
        Ok(meta) if !meta.is_dir() => {
            return Err(ExportError::NotAFolder {
                into: into.to_path_buf(),
            })
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ExportError::Read {
                path: into.to_path_buf(),
                source,
            })
        }
    }
    if !WeaverWorkspace::is_marked(into) {
        for names in Names::ALL {
            let folder = names.workspace_dir();
            if fs::symlink_metadata(into.join(folder)).is_ok() {
                return Err(ExportError::Unmarked {
                    into: into.to_path_buf(),
                    folder,
                });
            }
        }
        return Ok(None);
    }
    let workspace = WeaverWorkspace::open(into)?;
    let config = workspace.config();
    let why = if fs::symlink_metadata(workspace.ledger_path()).is_ok() {
        Some("it has a ledger")
    } else if !config.projects.is_empty() {
        Some("it links projects")
    } else if !config.loaded.is_empty() {
        Some("it loads workspaces")
    } else if !config.known.is_empty() {
        Some("it knows other workspaces")
    } else {
        None
    };
    match why {
        Some(why) => Err(ExportError::InUse {
            into: into.to_path_buf(),
            why,
        }),
        None => Ok(Some(workspace)),
    }
}

/// Refuse a link anywhere from `into` down to `to`, a kind's folder in its
/// library: the replacement is made beside `to` and renamed onto it, so a
/// link on the way would put it, and take what it replaces, wherever the
/// link leads.
fn refuse_links(into: &Path, to: &Path) -> Result<(), ExportError> {
    let Ok(below) = to.strip_prefix(into) else {
        return Ok(());
    };
    let mut path = into.to_path_buf();
    for component in below.components() {
        path.push(component);
        if fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err(ExportError::Linked { path });
        }
    }
    Ok(())
}

/// Refuse an MCP server's definition holding what looks like a token, the
/// first found, and one that does not read, where that cannot be told.
fn refuse_secrets(library: &Catalog) -> Result<(), ExportError> {
    if let Some(failure) = library.failures().first() {
        return Err(ExportError::Unread {
            path: failure.path().to_path_buf(),
        });
    }
    for artifact in library.artifacts() {
        for issue in artifact.validate() {
            if let ValidationIssue::SecretLiteral { at, prefix } = issue {
                return Err(ExportError::Secret {
                    path: artifact.file(),
                    at,
                    prefix,
                });
            }
        }
    }
    Ok(())
}

/// What an export of `kind` does to each artifact, `library`'s against what
/// `exported` holds, by name.
fn compared(kind: Kind, library: &Catalog, exported: &Catalog) -> Result<Vec<Step>, ExportError> {
    let mut steps = Vec::new();
    for artifact in library.artifacts() {
        let there = exported
            .artifacts()
            .iter()
            .find(|other| other.name() == artifact.name());
        let change = match there {
            None => Change::Added,
            Some(there) if same(kind, artifact.path(), there.path())? => Change::Unchanged,
            Some(_) => Change::Updated,
        };
        steps.push(Step {
            kind,
            name: artifact.name().clone(),
            change,
        });
    }
    for artifact in exported.artifacts() {
        let kept = library
            .artifacts()
            .iter()
            .any(|other| other.name() == artifact.name());
        if !kept {
            steps.push(Step {
                kind,
                name: artifact.name().clone(),
                change: Change::Removed,
            });
        }
    }
    steps.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(steps)
}

/// Whether two of one kind hold the same: a folder's names and bytes, or a
/// file's bytes.
fn same(kind: Kind, a: &Path, b: &Path) -> Result<bool, ExportError> {
    let read = |path: &Path| {
        fs::read(path).map_err(|source| ExportError::Read {
            path: path.to_path_buf(),
            source,
        })
    };
    match kind.layout() {
        Layout::Directory { .. } => copy::same(a, b).map_err(|source| ExportError::Read {
            path: a.to_path_buf(),
            source,
        }),
        Layout::Files { .. } | Layout::Flat { .. } => Ok(read(a)? == read(b)?),
    }
}

#[cfg(test)]
#[path = "export_test.rs"]
mod tests;
