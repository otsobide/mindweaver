//! Why a command could not run at all.
//!
//! One enum for every command rather than one per family: a
//! [`Failure`](crate::Failure) carries whichever it is to the binary, and the
//! binary reports them all the same way.

use std::io;
use std::path::{Path, PathBuf};

use mindweaver_core::activate::{self, ActivateError, WORKSPACE_VARIABLE};
use mindweaver_core::adopt::AdoptError;
use mindweaver_core::backup::BackupError;
use mindweaver_core::everywhere::EverywhereError;
use mindweaver_core::export::ExportError;
use mindweaver_core::gather::GatherError;
use mindweaver_core::ledger::LedgerError;
use mindweaver_core::sources::SourceError;
use mindweaver_core::{
    codes, ArtifactError, CreateError, Kind, LifecycleError, Reason, ResolveError, TemplateError,
    WorkspaceError,
};
use thiserror::Error;

use crate::homes::HOME_VARIABLE;
use crate::render::plural;
use crate::task::Refusal;

/// Every type the CLI gives codes of its own to, by the module it is
/// defined in, with those codes: what reaches the panel beside core's
/// (`mindweaver_core::code::ALL`), and what a front end listing every code
/// starts from, as it does from core's.
///
/// Held to core's rules by a test: no code here is given twice, here or in
/// core, since the panel is handed a code without the type it came from.
pub const CODES: &[(&str, &[&str])] = &[
    ("error::CliError", CliError::CODES),
    ("task::Refusal", Refusal::CODES),
];

/// Every code a front end is handed for what went wrong, as one type a
/// description of what it is handed names: core's
/// (`mindweaver_core::code::CoreCode`), the CLI's own ([`CODES`]) and the
/// local server's (`mindweaver_server::Condition::CODES`).
///
/// A type with nothing in it, since what it stands for is the lists, and
/// here, beside them, rather than beside one description of them.
#[cfg(any(feature = "ts", feature = "schema"))]
pub struct Code;

/// Every code the CLI gives of its own ([`CODES`]), as one type a
/// description names: beside core's, what the CLI's backend refuses a
/// request with, which a description of the API lists, and part of
/// [`Code`].
#[cfg(feature = "schema")]
pub struct CliCode;

/// Why a command could not run at all, as opposed to running and finding
/// problems: those are in the [`Outcome`](crate::Outcome).
#[derive(Debug, Error)]
pub enum CliError {
    #[error("cannot determine the current directory: {0}")]
    CurrentDirectory(#[source] io::Error),
    #[error("{path}: cannot be resolved: {source}")]
    Resolve {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error("{} names a workspace that cannot be used: {source}", named_by(.reason))]
    Chosen {
        reason: Reason,
        source: WorkspaceError,
    },
    #[error("{0} is not inside a mind project (run `mind init` to create one here)")]
    NotInProject(PathBuf),
    #[error("{0} is not inside a weaver workspace (run `mindweaver init` to create one here)")]
    NotInWorkspace(PathBuf),
    #[error("no weaver workspace{} at or above {from} to {verb}, and no default one", if *named { " but the ones named" } else { "" })]
    NowhereToLoad {
        from: PathBuf,
        verb: &'static str,
        named: bool,
    },
    #[error("nothing named `{0}` here")]
    UnknownArtifact(String),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
    #[error(transparent)]
    Gather(#[from] GatherError),
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Install(#[from] mindweaver_core::install::InstallError),
    #[error(transparent)]
    Create(#[from] CreateError),
    #[error(transparent)]
    Lifecycle(#[from] LifecycleError),
    #[error(transparent)]
    Everywhere(#[from] EverywhereError),
    #[error(transparent)]
    Adopt(#[from] AdoptError),
    #[error(transparent)]
    Export(#[from] ExportError),
    #[error(transparent)]
    Backup(#[from] BackupError),
    #[error(
        "${0} holds no token: set it for this command, or leave --token-env out to log in as git does"
    )]
    TokenUnset(String),
    /// `--gh`, and no `gh` on the PATH to ask.
    #[error(
        "--gh asks GitHub's `gh` for the token, and there is no `gh` on the PATH: install it and \
         log in with `gh auth login`, or name a variable holding a token with --token-env"
    )]
    GhMissing,
    /// `--gh`, and `gh` there but not started, for why the system says.
    #[error("--gh asks GitHub's `gh` for the token, and it could not be run: {0}")]
    GhUnrun(String),
    /// `--gh`, and `gh auth token` with no token for the host: logged out,
    /// or never logged in there.
    #[error(
        "`gh` has no token for {0}: log in with `gh auth login --hostname {0}`, or name a \
         variable holding a token with --token-env"
    )]
    GhLoggedOut(String),
    #[error(transparent)]
    Template(#[from] TemplateError),
    #[error(transparent)]
    Activate(#[from] ActivateError),
    #[error(
        "$SHELL is not set; say which shell to write for with --shell ({})",
        activate::expected()
    )]
    NoShell,
    #[error(
        "cannot tell which shell `{program}` is; say which to write for with --shell ({})",
        activate::expected()
    )]
    UnknownShell { program: String },
    #[error("nothing removed: this deletes {}; add --yes to go ahead", deleting(.path, .kind, .files))]
    Unconfirmed {
        kind: Kind,
        path: PathBuf,
        files: usize,
    },
    #[error("nothing moved: {0}")]
    Refused(mindweaver_core::migrate::Refusal),
    #[error(transparent)]
    Migrate(#[from] mindweaver_core::migrate::MigrateError),
    #[error("nothing moved: what the workspace records of the project decides which of its copies move, and it cannot be read: {0}")]
    MigrateUnrecorded(Box<CliError>),
    #[error("say what to migrate: --library moves the workspace's own into its library, -p <project> moves a project it links from its mind.toml or settings.json to a settings.toml, and --all-projects moves every one")]
    MigrateWhat,
    #[error(transparent)]
    Reapply(#[from] mindweaver_core::ReapplyError),
    #[error(transparent)]
    Relink(#[from] mindweaver_core::relink::RelinkError),
    #[error(transparent)]
    Update(#[from] mindweaver_core::update::UpdateError),
    #[error("say which projects to update: -p <project> for each, or --all for every one the workspace links")]
    UpdateWhere,
    #[error("say which project's agents to change (`mindweaver agents -p <project> <agent>...`); the workspace's own reach agents by being installed")]
    AgentsWhere,
    #[error("say which project's targets to change (`mindweaver targets -p <project> set <kind> <dir>...`); the workspace's own reach agents by being installed")]
    TargetsWhere,
    #[error("`{editor}` could not be started: {source}")]
    Editor {
        editor: String,
        #[source]
        source: io::Error,
    },
    #[error("`{editor}` failed ({status}), so the file is reported on no further")]
    EditorFailed { editor: String, status: String },
    /// What `edit --contents` names could not be read, so nothing is
    /// replaced: `from` is the file, or standard input.
    #[error("{from}: cannot be read, so nothing was replaced: {source}")]
    ContentsUnread {
        from: String,
        #[source]
        source: io::Error,
    },
    /// The text a front end was to hand `edit --contents` could not be put
    /// in the workspace's scratch folder for the line to read, so the line
    /// was not run: the folder could not be made, or something other than
    /// that text is where it goes.
    #[error("{path}: the text to save cannot be put here for `edit --contents` to read, so nothing was run: {source}")]
    Scratch {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("no weaver workspace above {0}; make one with `mindweaver init` in the directory that holds your projects")]
    NoWorkspaceAbove(PathBuf),
    #[error("{0}: not known, since there is no default workspace to remember it (--home or ${HOME_VARIABLE} names none)")]
    NoDefault(PathBuf),
    #[error("{project}: not managed by the workspace at {workspace}; `mind link` adds it")]
    NotManaged {
        project: PathBuf,
        workspace: PathBuf,
    },
    #[error("no project called `{wanted}` in this workspace{}", known_projects(.known))]
    UnknownProject { wanted: String, known: Vec<String> },
    #[error("`{wanted}` names more than one project ({}), so name it by its path", .matches.join(", "))]
    AmbiguousProject {
        wanted: String,
        matches: Vec<String>,
    },
    #[error("say which projects to install into: -p <project> for each, or --all-projects for every one the workspace links; or run `mindweaver install` alone for the screen")]
    InstallWhere,
    #[error("say which skill or rule to install (`mindweaver install -p <project> <name>`), or run `mindweaver install` alone for the screen")]
    InstallWhat,
    #[error("say which projects to take it out of: -p <project> for each, or --all-projects for every one the workspace links")]
    UninstallWhere,
    #[error("{project} has no {name} installed, so there is nothing to take out")]
    NotInstalled { name: String, project: String },
    #[error("no project asked has {name} installed, so there is nothing to take out")]
    NotInstalledAnywhere { name: String },
    #[error("`{command}` needs a terminal to draw on, and this is not one: {source}")]
    NoTerminal {
        command: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("`mindweaver` on its own opens a screen, and this is not a terminal; `mindweaver --help` lists the commands")]
    NotATerminal,
    #[error("the screen could not be drawn: {source}")]
    Screen {
        #[source]
        source: io::Error,
    },
    /// A line a front end handed over that neither parser takes, in the
    /// parser's words: a value that leaves out what a command needs, as an
    /// `agents` line naming none.
    #[error("{0}")]
    Unparsed(String),
    /// The port `ui` was to serve the panel on could not be taken: one
    /// somebody else holds, say. `dev` is `--dev`'s, which changes what to
    /// do about it ([`unbound_hint`]).
    #[error("the panel cannot listen on 127.0.0.1:{port}: {source}{}", unbound_hint(*port, *dev))]
    Unbound {
        port: u16,
        dev: bool,
        #[source]
        source: io::Error,
    },
    /// `ui` could not go on serving the panel, or start to.
    #[error("the panel could not be served: {source}")]
    Unserved {
        #[source]
        source: io::Error,
    },
    /// Something is where `ui --token-file` was to write: a file, a folder
    /// or a link, which `create_new` refuses alike, so nothing is written
    /// over it or through it.
    #[error("{0}: already exists, and is left alone, so the panel was not served: --token-file writes a file of its own, never over one")]
    TokenFileExists(PathBuf),
    /// The folder `ui --token-file` was to write in is not there.
    #[error(
        "{0}: its folder does not exist, so the panel was not served: --token-file makes no folder"
    )]
    TokenFileFolderMissing(PathBuf),
    /// `ui --token-file` could not write there for any other reason: a
    /// file on the way where a folder should be, or a folder it may not
    /// write in.
    #[error("{path}: the API's address and token cannot be written there, so the panel was not served: {source}")]
    TokenFileUnwritable {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// What `ui --token-file` wrote could not be taken away once the panel
    /// stopped, nor looked at to tell whether it still held that: said as a
    /// warning, since the panel was served all the same.
    #[error("{path}: could not be taken away once the panel stopped: {source}")]
    TokenFileUnremoved {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

// A variant that carries core's error takes its code, as one of core's does;
// one that says a condition core names in words of the CLI's, with the
// command that would put it right, echoes the code core gives it, so the
// panel meets one code for one condition whichever way it was reached.
codes!(CliError {
    Self::CurrentDirectory(_) => "current_directory_unreadable",
    Self::Resolve { .. } => "path_unresolved",
    Self::NotInProject(_) => "not_in_project",
    Self::NotInWorkspace(_) => "not_in_workspace",
    Self::NowhereToLoad { .. } => "nowhere_to_load",
    Self::NoShell => "shell_unset",
    Self::Unconfirmed { .. } => "remove_unconfirmed",
    Self::MigrateWhat => "migrate_nothing_named",
    Self::UpdateWhere => "update_no_projects",
    Self::AgentsWhere => "agents_no_project",
    Self::TargetsWhere => "targets_no_project",
    Self::Editor { .. } => "editor_unstarted",
    Self::EditorFailed { .. } => "editor_failed",
    Self::ContentsUnread { .. } => "edit_contents_unread",
    Self::Scratch { .. } => "scratch_unwritable",
    Self::NoWorkspaceAbove(_) => "no_workspace_above",
    Self::NoDefault(_) => "no_default_workspace",
    Self::InstallWhere => "install_no_projects",
    Self::InstallWhat => "install_nothing_named",
    Self::UninstallWhere => "uninstall_no_projects",
    Self::NoTerminal { .. } => "terminal_unusable",
    Self::NotATerminal => "not_a_terminal",
    Self::Screen { .. } => "screen_failed",
    Self::Unparsed(_) => "line_unparsed",
    Self::Unbound { .. } => "panel_unbound",
    Self::Unserved { .. } => "panel_failed",
    Self::TokenFileExists(_) => "token_file_exists",
    Self::TokenFileFolderMissing(_) => "token_file_folder_missing",
    Self::TokenFileUnwritable { .. } => "token_file_unwritable",
    Self::TokenFileUnremoved { .. } => "token_file_unremoved",
    Self::TokenUnset(_) => "token_env_unset",
    Self::GhMissing => "gh_missing",
    Self::GhUnrun(_) => "gh_unrun",
    Self::GhLoggedOut(_) => "gh_token_missing",
} carried {
    Self::Workspace(error) => error,
    Self::Chosen { source, .. } => source,
    Self::Artifact(error) => error,
    Self::Gather(error) => error,
    Self::Source(error) => error,
    Self::Ledger(error) => error,
    Self::Install(error) => error,
    Self::Create(error) => error,
    Self::Lifecycle(error) => error,
    Self::Everywhere(error) => error,
    Self::Adopt(error) => error,
    Self::Export(error) => error,
    Self::Backup(error) => error,
    Self::Template(error) => error,
    Self::Activate(error) => error,
    Self::Refused(refusal) => refusal,
    Self::Migrate(error) => error,
    Self::MigrateUnrecorded(error) => error,
    Self::Reapply(error) => error,
    Self::Relink(error) => error,
    Self::Update(error) => error,
} echoed {
    Self::UnknownArtifact(_) => "not_found",
    Self::UnknownShell { .. } => "unknown_shell",
    Self::NotManaged { .. } => "not_managed",
    Self::UnknownProject { .. } => "unknown_project",
    Self::AmbiguousProject { .. } => "ambiguous_project",
    Self::NotInstalled { .. } => "not_installed",
    Self::NotInstalledAnywhere { .. } => "not_installed",
});

/// Core's answer to which workspace, or which of its projects, rendered as
/// the CLI says it: with the command that would put it right.
impl From<ResolveError> for CliError {
    fn from(error: ResolveError) -> Self {
        match error {
            ResolveError::Chosen { reason, source } => CliError::Chosen { reason, source },
            ResolveError::Workspace(source) => CliError::Workspace(source),
            ResolveError::NotManaged { project, workspace } => {
                CliError::NotManaged { project, workspace }
            }
            ResolveError::UnknownProject { wanted, known } => {
                CliError::UnknownProject { wanted, known }
            }
            ResolveError::AmbiguousProject { wanted, matches } => {
                CliError::AmbiguousProject { wanted, matches }
            }
        }
    }
}

/// What to do about a port `ui` could not take.
///
/// Under `--dev` another port is no way out on its own, and 0 none at all:
/// the dev server's proxy sends the panel's requests, and the launch link,
/// to the one port `SERVER` names in apps/web/vite.config.ts, so a server on
/// any other is one the panel never reaches.
fn unbound_hint(port: u16, dev: bool) -> &'static str {
    match (port, dev) {
        // The system's own pick failing is nothing another port mends.
        (0, _) => "",
        (_, true) => {
            "; the panel's dev server reaches only the port `SERVER` names in \
             apps/web/vite.config.ts, so stop what holds it, or name another there \
             and with --port"
        }
        (_, false) => "; name another with --port, or 0 for one the system picks",
    }
}

/// What chose a workspace that cannot be used, as it was spelled.
fn named_by(reason: &Reason) -> String {
    match reason {
        Reason::Activated => format!("${WORKSPACE_VARIABLE}"),
        // Core refuses only what was chosen, so the other two never get here.
        Reason::Explicit | Reason::Above | Reason::Home => String::from("-W"),
    }
}

/// "; it manages alpha, beta", or what to do when it manages nothing.
fn known_projects(known: &[String]) -> String {
    if known.is_empty() {
        String::from("; it manages none yet: `mindweaver link <path>` adds one")
    } else {
        format!("; it manages {}", known.join(", "))
    }
}

/// What `mind remove` would delete, as a sentence.
pub(crate) fn deleting(path: &Path, kind: &Kind, files: &usize) -> String {
    match kind.layout() {
        mindweaver_core::Layout::Directory { .. } => {
            format!(
                "{} and the {} in it",
                path.display(),
                plural(*files, "file")
            )
        }
        mindweaver_core::Layout::Files { .. } | mindweaver_core::Layout::Flat { .. } => {
            path.display().to_string()
        }
    }
}

#[cfg(test)]
#[path = "error_test.rs"]
mod tests;
