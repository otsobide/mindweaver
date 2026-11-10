//! What a request from outside may name: the path policy every task the
//! panel's browser sends passes before a line of it runs (#67, D-18).
//!
//! The panel turns a task into command lines and runs them as a person at a
//! terminal would, which is the point: nothing it does is something a
//! command could not. But a person typing a path knows what they meant, and
//! a browser is a stranger whose request reached the server with the token,
//! from a page that may not be the panel's. So a task is held to less than
//! a command line is:
//!
//! - **A project or a loaded workspace is named by its entry**, spelled as
//!   the workspace's marker spells it, which is how a snapshot lists it
//!   ([`linked`], [`loaded`]). Never by a name, nor by a path the front end
//!   joins itself: the front end turns the entry into the path the registry
//!   resolves, and names the project to the command by that.
//! - **A folder that is not linked yet is named by a path that reads the same
//!   from anywhere** ([`path`]): absolute, spelled as normalizing it would
//!   spell it, so that what the preview shows is the folder the command
//!   acts on, and outside every `.mindweaver` (and `.mindflayer`), where a workspace keeps its
//!   marker, its ledger, its library and the clones it gathered, none of
//!   which is a project to make or link. A path that leads into one through
//!   a link is refused as one inside it.
//! - **A gather's source is an address or such a folder** ([`source`]): an
//!   address is the gather's to judge, as one typed at a terminal is, and a
//!   folder on this machine, as a path or a `file://` URL, is held as a
//!   folder to link is.
//! - **A workspace to switch to is named by its path as a list of core's
//!   spells it** ([`switched`]): one of the workspaces this machine knows,
//!   or the one the front end was launched on ([`crate::known::switchable`]),
//!   matched character for character against that list and never opened as
//!   given, so a front end that stays on one workspace goes to no other.
//!
//! Each refusal has a code, as every error core gives does
//! ([`crate::code`]), and the front end runs nothing of a task one of its
//! values is refused for. Here rather than in the panel's backend, because
//! a rule about which paths may be named is one a second front end would
//! otherwise write again, and could write differently.

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use crate::code::codes;
use crate::create::{self, CreateError};
use crate::install;
use crate::kind::Kind;
use crate::known::Switchable;
use crate::mcp::Given;
use crate::names::Names;
use crate::paths;
use crate::resolve::Reason;
use crate::value::Redacted;
use crate::workspace::{Loaded, Member, WeaverWorkspace, WorkspaceError};

/// Why a value a request holds is not one it may name.
#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("`{given}` is a URL, not a folder: name a folder by its absolute path")]
    Url { given: String },
    #[error("`{given}` is not an absolute path: name a folder by its absolute path, which reads the same from anywhere")]
    Relative { given: String },
    #[error("`{given}` is not spelled as the folder it names, `{normal}`: no `.` or `..` part, and no doubled or trailing separator")]
    Unnormalized { given: String, normal: String },
    /// Inside a `.mindweaver`, or the `.mindflayer` of a workspace a
    /// Mindflayer build made: `folder` is the one it is in. Still the code
    /// it was given under the old name, as every code is kept.
    #[error("`{given}` is inside a `{folder}`, which holds a workspace's own files: name a folder outside it")]
    InWeaverDir { given: String, folder: &'static str },
    /// A workspace to switch to that no list of what may be switched to
    /// holds, by the path it spells: a typo, a list read before it changed,
    /// or a path somebody put together, none of which is gone to.
    #[error("`{given}` is none of the workspaces this machine knows, nor the one the panel was launched on: only one of those is switched to")]
    Unlisted { given: String },
    /// A yes sent with an install that waits for none: anything but an
    /// MCP server from elsewhere than the library, by the place it comes
    /// from as `--from` takes it, or from no place named, which is not
    /// told until the line runs (`install::asks_first`). The one yes a
    /// request may carry is to such a server, once somebody has been shown
    /// what each agent would run of it. The place as `--from` took it, a
    /// source's address among them, without any credential it was typed
    /// with, as an install's refusal says one: it goes back to whoever
    /// sent it, and into whatever logs that.
    #[error("{} {name}{}: nothing is asked before it is installed, so no yes is sent with it", .kind.noun(), .from.as_ref().map_or_else(|| String::from(" from no place named"), |from| format!(" from {from}")))]
    YesUnasked {
        kind: Kind,
        name: String,
        from: Option<Redacted>,
    },
    /// A server's options a line cannot say as given, a name beside a
    /// value that holds `=`, refused by `add`'s own check, and by its code
    /// ([`sayable`]).
    #[error(transparent)]
    Server(CreateError),
    /// An entry no registry holds, or a path no entry points at: the
    /// workspace's own error for an unlink or an unload of one, since it is
    /// the condition they meet too.
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
}

codes!(PolicyError {
    Self::Url { .. } => "path_url",
    Self::Relative { .. } => "path_relative",
    Self::Unnormalized { .. } => "path_unnormalized",
    Self::InWeaverDir { .. } => "path_in_mindflayer",
    Self::Unlisted { .. } => "switch_unlisted",
    Self::YesUnasked { .. } => "install_yes_unasked",
} carried {
    Self::Server(error) => error,
    Self::Workspace(error) => error,
});

/// A yes a request sends with an install of `kind` `name` from `from`, as
/// `--from` takes it: one only for an install that waits for one
/// ([`install::asks_first`]), an MCP server from elsewhere than the
/// library, named by where it comes from.
///
/// Held here, with every value a request may name, rather than passed on
/// for the line to ignore: a yes is a front end saying somebody was shown
/// what each agent would run, and one sent where nothing asks it says that
/// of nothing, which a request that could carry it anywhere would come to
/// mean of everything.
pub fn confirmed(kind: Kind, name: &str, from: Option<&str>) -> Result<(), PolicyError> {
    if from.is_some_and(|from| install::asks_first(kind, from)) {
        return Ok(());
    }
    Err(PolicyError::YesUnasked {
        kind,
        name: name.to_owned(),
        from: from.map(Redacted::new),
    })
}

/// A server's options a request sends with an `add` or an `edit`, `given`:
/// refused when a line cannot carry one of its names as given, one beside
/// a value that holds `=`, which the line's option would end at the first
/// one (`create::sayable`).
///
/// Held here, before a line is built, as every value a request sends is:
/// the line would write a pair other than the one its draft was judged
/// as, and the command would never be told. Refused by the code `add`'s
/// check gives the draft, so the two say one thing.
pub fn sayable(given: &Given) -> Result<(), PolicyError> {
    create::sayable(given).map_err(PolicyError::Server)
}

/// `given`, as a path a request may name: absolute, spelled exactly as
/// [`paths::normalize`] would spell it, and outside every `.mindweaver` and
/// every `.mindflayer` a Mindflayer build made, both as given and once its
/// links are followed.
///
/// Spelled as given is spelled normalized, rather than normalized for it, so
/// the path a preview shows is the one the command is given, character for
/// character. A path that does not exist yet is followed as far as it does
/// exist, the rest appended ([`install::resolved`]): `mind init` makes a
/// project in a folder that may be new, and one new under a link into a
/// `.mindweaver` would be made inside it.
pub fn path(given: &str) -> Result<PathBuf, PolicyError> {
    let path = Path::new(given);
    if !path.is_absolute() {
        // A URL is not absolute either, but says so in its own words: a
        // `file://` one names a folder, and is still not how one is named.
        return Err(if given.contains("://") {
            PolicyError::Url {
                given: given.to_owned(),
            }
        } else {
            PolicyError::Relative {
                given: given.to_owned(),
            }
        });
    }
    let normal = paths::normalize(path);
    if normal.as_os_str() != path.as_os_str() {
        return Err(PolicyError::Unnormalized {
            given: given.to_owned(),
            normal: normal.display().to_string(),
        });
    }
    let followed = install::resolved(path);
    if let Some(folder) = [Some(path), followed.as_deref()]
        .into_iter()
        .flatten()
        .find_map(in_weaver_dir)
    {
        return Err(PolicyError::InWeaverDir {
            given: given.to_owned(),
            folder,
        });
    }
    Ok(normal)
}

/// `given`, as the source of a gather a request may name: an address, or a
/// folder on this machine named as a folder to link is.
///
/// An address, a URL whose scheme is not `file` (`https://`, `ssh://`,
/// `git://`) or git's `host:path`, a colon before any slash, is a value,
/// which the gather holds to its own rules as it holds one typed at a
/// terminal: one carrying a credential is refused there, by its code and
/// with nothing of the credential said, and no prompt is ever made for
/// one. A folder, a path or a `file://` URL naming one, is held to
/// [`path`]: absolute, spelled as it is normalized, and outside every
/// `.mindweaver`, so the preview shows the repository the gather reads, a
/// relative one read from wherever the line runs is never guessed at, and
/// no workspace's own folder, its clones among them, is gathered from.
///
/// Here rather than left to the gather, as every value a request names on
/// the disk is held here: a gather typed at a terminal takes a folder
/// relative to where it is typed, which a browser has no place for.
pub fn source(given: &str) -> Result<(), PolicyError> {
    if let Some(folder) = given
        .get(..FILE.len())
        .filter(|scheme| scheme.eq_ignore_ascii_case(FILE))
        .map(|_| &given[FILE.len()..])
    {
        return path(folder).map(drop);
    }
    let scp_like = given
        .find(':')
        .is_some_and(|colon| colon > 0 && !given[..colon].contains(['/', '\\']));
    if given.contains("://") || (scp_like && !Path::new(given).is_absolute()) {
        return Ok(());
    }
    path(given).map(drop)
}

/// What a `file://` URL starts with, its path after it.
const FILE: &str = "file://";

/// The entry of `workspace`'s registry spelled `entry`, as a snapshot lists
/// it, with where it points and the project there, or why there is none.
///
/// By its spelling and nothing else: a project's name, which two projects
/// can declare, or another path to the same folder, would each be a second
/// way to name one, which a request has no need of. An entry whose project
/// has gone is still an entry, and the command it reaches says why it
/// cannot act on it.
pub fn linked(workspace: &WeaverWorkspace, entry: &str) -> Result<Member, PolicyError> {
    workspace
        .members()
        .into_iter()
        .find(|member| paths::as_route(&member.entry) == entry)
        .ok_or_else(|| {
            WorkspaceError::NotRegistered {
                path: PathBuf::from(entry),
                workspace: workspace.root().to_path_buf(),
            }
            .into()
        })
}

/// The entry of `workspace`'s `loaded` list spelled `entry`, as a snapshot
/// lists it, matched as [`linked`] matches a project's.
pub fn loaded(workspace: &WeaverWorkspace, entry: &str) -> Result<Loaded, PolicyError> {
    workspace
        .loads()
        .into_iter()
        .find(|loaded| paths::as_route(&loaded.entry) == entry)
        .ok_or_else(|| {
            WorkspaceError::NotLoaded {
                path: PathBuf::from(entry),
                workspace: workspace.root().to_path_buf(),
            }
            .into()
        })
}

/// `given`, a [`path`] some entry of `workspace`'s registry points at: how a
/// command that takes linked projects by their paths, `mindweaver unlink` from
/// the minds screen's marks, is given one.
pub fn linked_path(workspace: &WeaverWorkspace, given: &str) -> Result<PathBuf, PolicyError> {
    let path = path(given)?;
    if !workspace.is_linked(&path) {
        return Err(WorkspaceError::NotRegistered {
            path,
            workspace: workspace.root().to_path_buf(),
        }
        .into());
    }
    Ok(path)
}

/// `given`, a [`path`] some entry of `workspace`'s `loaded` list points at:
/// how `mind unload` is given the workspace it takes out.
pub fn loaded_path(workspace: &WeaverWorkspace, given: &str) -> Result<PathBuf, PolicyError> {
    let path = path(given)?;
    if !workspace.is_loaded(&path) {
        return Err(WorkspaceError::NotLoaded {
            path,
            workspace: workspace.root().to_path_buf(),
        }
        .into());
    }
    Ok(path)
}

/// The workspace `given` names among `listed`, what [`crate::known::switchable`]
/// lists, opened, with the rule it is chosen by once switched to: what a
/// front end that stays on one workspace goes to when asked to go to
/// another.
///
/// By the path the list spells it by, character for character, rather than
/// by where `given` leads: a path a request sent is never opened, nor
/// followed through a link, so nothing but a listed workspace is reached,
/// and `given` names nothing else for being another spelling of it. One
/// listed that does not open is refused with why it does not, never made:
/// a workspace is made by `mindweaver init`, and a list read a moment ago may
/// name one that has gone since. The one in use is taken, a switch to it
/// changing nothing, as a load of one loaded already changes nothing.
pub fn switched(
    listed: &[Switchable],
    given: &str,
) -> Result<(WeaverWorkspace, Reason), PolicyError> {
    let row = listed
        .iter()
        .find(|row| row.workspace.path.to_str() == Some(given))
        .ok_or_else(|| PolicyError::Unlisted {
            given: given.to_owned(),
        })?;
    Ok((WeaverWorkspace::open(&row.workspace.path)?, row.reason))
}

/// `path` as a command line's words hold it: a path the registry resolved,
/// which a front end hands a command in place of what it was asked.
///
/// Refused when it is not UTF-8, which no word of a command line can be,
/// rather than spelled lossily, which would name another folder.
pub fn spelled(path: &Path) -> Result<String, PolicyError> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        WorkspaceError::NonUtf8Path {
            path: path.to_path_buf(),
        }
        .into()
    })
}

/// Which workspace's folder some part of `path` is, under either names:
/// `.mindweaver`, or a Mindflayer build's `.mindflayer`.
fn in_weaver_dir(path: &Path) -> Option<&'static str> {
    Names::ALL
        .into_iter()
        .map(Names::workspace_dir)
        .find(|folder| {
            path.components()
                .any(|component| component == Component::Normal(folder.as_ref()))
        })
}

#[cfg(test)]
#[path = "policy_test.rs"]
mod tests;
