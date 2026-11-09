//! The ports: what core needs of the world beyond the files it reads and
//! writes, each as a trait core's rules are written against, with the
//! adapter a build runs with in [`crate::adapter`].
//!
//! The architecture is hexagonal. Core's rules (what a skill is, what an
//! install may overwrite, which copy is edited) sit in the middle and depend
//! on nothing outside them; whatever they need from outside is a port here,
//! a trait in core's own words; and an adapter on the edge implements it with
//! a library (`gix` for a [`Remote`], SQLite for a [`LedgerStore`], the
//! system clock for a [`Clock`]). The
//! front ends, the CLI, the TUI and the panel's backend, are the other side's
//! adapters: they drive core through its functions and the command lines
//! every one of them runs (`commands::run_line`), and add nothing core has to
//! know. So a new store, transport or clock is an adapter written against a
//! port, and a new front end is one more driver of the same commands: neither
//! changes a rule.
//!
//! This is the seam D-14 chose for HTTP, kept for every port: an injectable
//! trait, a default adapter built in, and a fake in a test. A port speaks only
//! core's types. What an adapter's library says is turned into those at the
//! edge (an error into text, a commit into a [`Revision`]), rather than
//! carried through, which would tie core's public API to the library's
//! version and let a rule depend on how one library happens to work.
//!
//! The files themselves are not behind a port. What core promises about them
//! is how they are written (`create_new`, a temporary renamed into place, a
//! link never followed out of its holder), and those promises are made of the
//! operating system's own calls; an adapter that only seemed to keep them is
//! the failure a port here would invite (docs/architecture.md, "Values,
//! ports and adapters").

use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;

use crate::code::codes;
use crate::gather::Watch;
use crate::kind::Kind;
use crate::ledger::{
    Action, Carried, Entry, Forgotten, Gathered, InstallRecord, LedgerError, Merged, Moved,
    Outcome, Source, SourceKind, Target,
};
use crate::settings::ProjectId;
use crate::value::{Revision, SourceName};
use crate::workspace::WorkspaceError;

/// Where a source's artifacts are fetched from: a repository a gather clones
/// into the workspace's cache, and later scrubs of anything a credential left
/// in it.
///
/// One method a step of a gather, rather than one that gathers: what a gather
/// does with what was fetched (reading it, holding it to the rules, placing
/// it on the shelf) is core's, and the same whichever remote fetched it.
pub trait Remote: Debug + Send + Sync {
    /// Fetch `url`, at `reference` when one is given, into `into`, replacing
    /// whatever was there, telling `watch` how it goes and stopping when it
    /// is cancelled ([`GitError::Cancelled`]).
    ///
    /// What is left at `into` holds no credential once this returns, whoever
    /// asked: a caller that forgot to refuse one is not trusted to.
    fn fetch(
        &self,
        url: &str,
        reference: Option<&str>,
        into: &Path,
        watch: &Watch,
    ) -> Result<Cloned, GitError>;

    /// Take out of the clone at `clone` every credential an older build left
    /// in what it wrote there, returning whether there were any.
    fn scrub(&self, clone: &Path) -> Result<bool, GitError>;
}

/// The time, for what the ledger stamps an action or a gather with.
///
/// A port rather than a call to the system clock where the time is read, so a
/// fixture can pin it and a test can tell one moment from the next without
/// waiting for a second to pass.
pub trait Clock: Debug + Send + Sync {
    /// Seconds since the Unix epoch.
    fn now(&self) -> i64;
}

/// A clone that is on disk and ready to be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cloned {
    /// The working tree.
    pub root: PathBuf,
    /// The commit that was checked out, when it could be resolved.
    pub revision: Option<Revision>,
}

/// Why a repository could not be cloned.
///
/// Every variant carries the failure as text rather than as the error of the
/// library an adapter fetched with. Those types are large and would become
/// part of this crate's public API, tying its version to the library's; what a
/// caller does with them is print them. Every URL in that text, the one typed
/// included, is [`redacted`](crate::url::redacted): this is what the gather
/// log and the screen are given.
#[derive(Debug, Error)]
pub enum GitError {
    #[error("{url}: not a URL git understands: {detail}")]
    Url { url: String, detail: String },
    #[error("`{reference}` is not a usable branch or tag name: {detail}")]
    Reference { reference: String, detail: String },
    #[error("{path}: the cache directory cannot be prepared: {source}")]
    Cache {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{url}: cannot be fetched: {detail}")]
    Fetch { url: String, detail: String },
    /// The server asked who is fetching, and nothing that may answer had a
    /// credential it would take. `url` is shown without its credentials.
    #[error(
        "{url}: needs credentials, and gathering never prompts for them: \
         configure a git credential helper, or use ssh-agent for an ssh URL"
    )]
    Credentials { url: String },
    #[error("{url}: cloned, but its files could not be checked out: {detail}")]
    Checkout { url: String, detail: String },
    /// The [`Watch`] was cancelled before the clone was finished. What there
    /// was of it is gone.
    #[error("the clone was cancelled before it was finished")]
    Cancelled,
    /// A file in the clone that names the URL it was made from could not be
    /// read, or rewritten without the credentials in it.
    #[error(transparent)]
    Scrub(#[from] WorkspaceError),
}

codes!(GitError {
    Self::Url { .. } => "git_url_invalid",
    Self::Reference { .. } => "git_reference_invalid",
    Self::Cache { .. } => "git_cache_failed",
    Self::Fetch { .. } => "git_fetch_failed",
    Self::Credentials { .. } => "git_credentials_needed",
    Self::Checkout { .. } => "git_checkout_failed",
    Self::Cancelled => "git_clone_cancelled",
} carried {
    Self::Scrub(error) => error,
});

/// Where a workspace's records are kept: the gathered sources and what each
/// supplied, every installation and its copies, the projects it knew, and
/// the action log.
///
/// Every method is a question or a change in core's words, never a query:
/// no caller writes SQL, and a store of another kind implements the same
/// methods ([`crate::ledger::Ledger`] holds it). Each write is whole or not
/// at all, as a transaction makes it; each URL a store is handed it keeps
/// [`redacted`](crate::url::redacted), and hands back as a `Redacted`, as
/// [`crate::ledger`] promises. `Send`, so a front end can read on a thread
/// of its own.
pub trait LedgerStore: Debug + Send {
    /// Stamp what it writes from now on by `clock`.
    fn stamp_by(&mut self, clock: Arc<dyn Clock>);

    /// Where it is kept, which every error it gives names: a file, or what a
    /// store kept nowhere says in its place (`:memory:` for SQLite's).
    fn path(&self) -> &Path;

    /// The source at this coordinate, if it has been gathered before: the
    /// one whose URL is `url` however it was spelled
    /// ([`crate::url::canonical`]), at the same branch, or at the remote's
    /// HEAD when `reference` is `None`, and from the same folder.
    fn source(
        &self,
        kind: SourceKind,
        url: &str,
        reference: Option<&str>,
        subdirectory: &str,
    ) -> Result<Option<Source>, LedgerError>;

    /// The source for this coordinate, registering it the first time, as
    /// `url` was typed, under the name its URL gives it
    /// ([`SourceName::derived`]), numbered when another source has that.
    ///
    /// Its `directory` is chosen once, here, and never recomputed: it is where
    /// files already are.
    fn source_for(
        &self,
        kind: SourceKind,
        url: &str,
        reference: Option<&str>,
        subdirectory: &str,
    ) -> Result<Source, LedgerError>;

    /// The name and the folder a source registered from `url` now would be
    /// given ([`source_for`](Self::source_for)), registering nothing: what a
    /// gather's dry run says of a source it has not met
    /// ([`crate::gather::dry_run`]).
    fn would_register(&self, url: &str) -> Result<(SourceName, String), LedgerError>;

    /// The source called `name`, if one is.
    fn source_named(&self, name: &SourceName) -> Result<Option<Source>, LedgerError>;

    /// Call the source `id` `name` from now on. Whether no other source has
    /// it is the caller's to have asked ([`source_named`](Self::source_named)):
    /// a store refuses a name two sources would hold as a failure.
    fn rename_source(&self, id: i64, name: &SourceName) -> Result<(), LedgerError>;

    /// Take the source `id` from the folder `subdirectory` of its repository
    /// from now on: one an older build registered from `skills`, its default
    /// folder then, which a gather naming no folder takes the whole
    /// repository of since #101 ([`crate::gather::OLDER_DEFAULT_SUBDIRECTORY`]).
    /// Whether no other source is at the coordinate it moves to is the
    /// caller's to have asked ([`source`](Self::source)): a store refuses one
    /// two sources would hold as a failure.
    fn set_source_folder(&self, id: i64, subdirectory: &str) -> Result<(), LedgerError>;

    /// The sources the step to schema 7 made one with another ([`Merged`])
    /// whose folders are not gone yet, in the order the step made them one.
    ///
    /// Kept from the step until then, rather than handed back by the opening
    /// that took it: a read takes the step as often as a write does, the
    /// first command after an upgrade being a `status` or a `list` as often
    /// as not, and says nothing of it, so the first command that writes is
    /// the one to say each merge and clear its folders
    /// ([`crate::gather::scrub`]), whichever opened the ledger first. None
    /// once that is done, and none from a store that never had an older
    /// schema to step from.
    fn merged(&self) -> Result<Vec<Merged>, LedgerError>;

    /// Forget the merge [`merged`](Self::merged) hands back with the folders
    /// called `directory`, once they are gone: a folder name a source may be
    /// given again from then on, and a merge said for the last time.
    fn cleared_merge(&self, directory: &str) -> Result<(), LedgerError>;

    /// Note which revision a source was last gathered at.
    fn saw_source(&self, id: i64, revision: Option<&str>) -> Result<(), LedgerError>;

    /// Forget a source, and with it what it brought; an installation from it
    /// keeps its `origin` but no longer points at it (see the schema's notes
    /// on what a forgotten source takes with it). Whether there was one to
    /// forget.
    ///
    /// For a gather that registered a source and then was cancelled, which
    /// leaves the ledger as it found it: a source nothing came from would
    /// otherwise hold on to its folder name. Its files are the caller's.
    fn forget_source(&self, id: i64) -> Result<bool, LedgerError>;

    /// Record one gathered artifact, replacing what that source held under the
    /// same name.
    ///
    /// The same name from a *different* source is a different row, on purpose:
    /// two repositories offering `commit-style` are two things to choose
    /// between, and this is what remembers which is which.
    fn record(
        &self,
        source: &Source,
        kind: Kind,
        name: &str,
        path: &str,
        summary: Option<&str>,
        revision: Option<&str>,
    ) -> Result<(), LedgerError>;

    /// Everything gathered, ordered the way a listing reads it: by kind, then
    /// name, then source.
    fn gathered(&self) -> Result<Vec<Gathered>, LedgerError>;

    /// Record that every artifact gathered at a path starting with `from`,
    /// a route from the workspace's root, is at the same path starting with
    /// `to` now, and how many were: for a shelf moved with the folder it is
    /// in, as a workspace a Mindflayer build made is moved to Mindweaver's
    /// names (`migrate --names`). One transaction, so a listing never finds
    /// some of them moved; what it was gathered from, and when, stay.
    fn shelf_moved(&self, from: &str, to: &str) -> Result<usize, LedgerError>;

    /// Every source anything was gathered from, or tried to be, ordered the
    /// way a listing reads them: by URL, then branch, then folder.
    ///
    /// Its own query rather than the sources of [`gathered`](Self::gathered):
    /// a source whose gather failed before it took anything, or whose every
    /// artifact has gone since, has no row there to be found by, and it is
    /// still a place the shelf is filled from. A row naming a kind this build
    /// has never heard of is skipped, as `gathered` skips it.
    ///
    /// Sorted once the URLs are redacted rather than by SQL, so a URL an
    /// older build stored with a credential is listed where the URL it is
    /// shown as goes.
    fn sources(&self) -> Result<Vec<Source>, LedgerError>;

    /// Record that an artifact was put into a project, replacing whatever was
    /// recorded of it before.
    ///
    /// Its targets are replaced as a whole rather than added to: they are the
    /// copies this install wrote, and a target an earlier install had and this
    /// one did not is no longer Mindweaver's to overwrite or delete. All of it
    /// or none, so a record never names some of an install's copies.
    fn installed(&self, project: &str, installation: &InstallRecord) -> Result<(), LedgerError>;

    /// Record what a render of a project's `kind` leaves it: each of
    /// `records` with its hash and copies in place of what was recorded of
    /// it, every other installation of `kind` in the project forgotten, and
    /// for each of `files`, a route from the project, whether Mindweaver made
    /// it ([`set_created`](Self::set_created)).
    ///
    /// For a kind the project is given as one render of all of it, as its
    /// rules are a block of one file: what is recorded of each is what that
    /// render wrote, so the whole is written at once, in one transaction, and
    /// a render's record is never half there. An installation keeps the time
    /// it was first recorded at, and its origin is taken in as every URL is.
    fn rendered(
        &self,
        project: &str,
        kind: Kind,
        records: &[InstallRecord],
        files: &[(&str, bool)],
    ) -> Result<(), LedgerError>;

    /// What the ledger records of installing `name` into a project, if it
    /// records anything.
    fn installation(
        &self,
        project: &str,
        kind: Kind,
        name: &str,
    ) -> Result<Option<InstallRecord>, LedgerError>;

    /// Forget an installation, returning whether there was one.
    fn uninstalled(&self, project: &str, kind: Kind, name: &str) -> Result<bool, LedgerError>;

    /// Forget the copy an installation of `name` wrote at `path`, and the
    /// installation once it records no other, saying which went.
    ///
    /// For a change that reached one copy: the others are still what was
    /// installed, and forgetting the whole installation would leave them with
    /// no record, never updated or removed again. A skill's installation
    /// recording no copy at all, which only a build from before installations
    /// had targets wrote, is forgotten whole, as [`uninstalled`](Self::uninstalled)
    /// would. A rule's is one a build writes, for a rule that reaches no
    /// agent the project names (`install::rules`), never one from before
    /// targets, since rules were installed only after: a change at some other
    /// path leaves it what was installed, and it stays. So does an MCP
    /// server's, which records entries and never a copy (D-10). One
    /// transaction, so an install of the same name at the same moment never
    /// finds half of this done.
    fn uninstalled_copy(
        &self,
        project: &str,
        kind: Kind,
        name: &str,
        path: &str,
    ) -> Result<Forgotten, LedgerError>;

    /// Record that the copy an installation of `name` wrote at `path` is
    /// called `to_name` now, at `to_path`, saying what became of its record.
    ///
    /// For a rename that reached one copy: it is what was installed, renamed,
    /// and stays Mindweaver's to tell apart (D-04), as an edit does, from
    /// where it came, with the hash of what was written. An installation it
    /// was the last copy of takes the new name whole, keeping when it was
    /// installed; one that records others keeps them under the old name,
    /// still what was installed, and the copy is recorded under the new one
    /// as an installation of its own from the same place. A name is one
    /// installation a project, so where another holds the new name already
    /// the copy is forgotten instead, as [`uninstalled_copy`](Self::uninstalled_copy) would.
    /// A skill's installation recording no copy at all, which only a build
    /// from before installations had targets wrote, is forgotten whole as
    /// there, and a rule's or an MCP server's stays as there. One
    /// transaction, so an install of either name at the same moment never
    /// finds half of this done.
    fn renamed_copy(
        &self,
        project: &str,
        kind: Kind,
        name: &str,
        path: &str,
        to_name: &str,
        to_path: &str,
    ) -> Result<Carried, LedgerError>;

    /// Record that the installation of `name` has its copies at `targets`
    /// now, in place of those it recorded, and forget it when `targets` is
    /// empty. Whether there was one.
    ///
    /// For copies that moved without the installation changing: a project
    /// that names another agent is given a copy where that agent reads, and
    /// one that stops naming an agent loses the copy there, while what was
    /// installed, from where, its hash and when stay what they were. Writing
    /// the whole record again ([`installed`](Self::installed)) would say it was
    /// installed just now. One transaction, so an install of the same name
    /// at the same moment never finds half of this done.
    fn retargeted(
        &self,
        project: &str,
        kind: Kind,
        name: &str,
        targets: &[Target],
    ) -> Result<bool, LedgerError>;

    /// Record where each of a project's installations has its copies once
    /// the project has moved from a `mind.toml` to its settings, a hash for
    /// each record that had none and was found current, the `id` its
    /// settings hold now, as [`knew`](Self::knew) records one, and `log`, each
    /// line a log entry about the project.
    ///
    /// All of it in one transaction, where [`retargeted`](Self::retargeted) takes one
    /// an installation: a migration is all or nothing for its project, and
    /// one that fails here puts the project's `mind.toml` back, so a ledger
    /// left with some installations moved would name copies where that
    /// marker installs none. An installation it does not record any more is
    /// passed over, as `retargeted` finds nothing to do for one.
    fn migrated(
        &self,
        project: &str,
        id: &ProjectId,
        moved: &[Moved],
        log: &[(Action, String)],
    ) -> Result<(), LedgerError>;

    /// The names this ledger says Mindweaver put into a project.
    ///
    /// What separates an artifact this tool installed from one somebody wrote
    /// by hand, which is what stops an uninstall from deleting the second.
    fn installations(&self, project: &str, kind: Kind) -> Result<Vec<String>, LedgerError>;

    /// Every installation into a project of `kind`, each read as
    /// [`installation`](Self::installation) reads one, in name order.
    ///
    /// For a report on everything a project was given, which has to name
    /// what was installed even where no copy of it is left to find, and asks
    /// once rather than once a name.
    fn records(&self, project: &str, kind: Kind) -> Result<Vec<InstallRecord>, LedgerError>;

    /// Every copy this ledger says Mindweaver wrote into a project for
    /// `kind`, whichever installation wrote it, by path relative to the
    /// project, `/` separated, in path order, each once: the rules of a
    /// project share its `AGENTS.md`, a section each.
    ///
    /// By copy rather than by name, for a listing that has to say which of
    /// the folders holding a skill hold Mindweaver's copy: a name alone
    /// would claim a copy somebody keeps in one agent's folder because
    /// Mindweaver installed one of that name into another's. One query
    /// rather than one per installation, since a listing asks it of every
    /// project.
    fn copies(&self, project: &str, kind: Kind) -> Result<Vec<String>, LedgerError>;

    /// Whether Mindweaver made the file at `path`, a route from the project,
    /// rather than wrote into one that was there: what a render was told
    /// when it made it ([`crate::agents_md::Written::Created`]), kept until
    /// the file stops being Mindweaver's.
    fn created(&self, project: &str, path: &str) -> Result<bool, LedgerError>;

    /// Record whether Mindweaver made the file at `path`, a route from the
    /// project: kept when it did, forgotten when it did not or the file has
    /// stopped being Mindweaver's. A file noted twice keeps the time it was
    /// first noted.
    fn set_created(&self, project: &str, path: &str, created: bool) -> Result<(), LedgerError>;

    /// The id the ledger last saw the project at `route` hold, if it saw one
    /// there: a route from the workspace, spelled as a record spells its
    /// project.
    ///
    /// An id somebody edited into a form no project holds is no id, rather
    /// than a failure: nothing can hold it, so nothing is found by it.
    fn project_at(&self, route: &str) -> Result<Option<ProjectId>, LedgerError>;

    /// Every id the ledger knows, with the route it last saw it at, in route
    /// order: what a scan matches the projects it finds against.
    fn known_projects(&self) -> Result<Vec<(ProjectId, String)>, LedgerError>;

    /// Whether anything is recorded of the project at `route`: an
    /// installation, or a file Mindweaver made there. What a project the
    /// ledger knows no id of has to hold nothing of for its entry to be
    /// handed to another directory ([`relinked`](crate::ledger::Ledger::relinked)), since nothing
    /// says which project those records were of.
    fn holds(&self, route: &str) -> Result<bool, LedgerError>;

    /// Record that each project of `seen` holds its id at its route, and
    /// say how many of them the ledger did not know there already.
    ///
    /// For what a workspace's projects say now, read whenever it is opened
    /// to write ([`crate::workspace::WeaverWorkspace::ledger_to_write`]): the
    /// ledger follows them, so it knows each linked project by the id it
    /// holds. An id it last saw at another route moves here, unless that
    /// route is one of `stale`, the entries the workspace links that open
    /// no project any more: an id seen there is what finds that entry's
    /// project once it has moved, and only `mindweaver relink` takes it from
    /// there ([`relinked`](crate::ledger::Ledger::relinked)). Whatever id the ledger knew at the
    /// route goes, as the project there holds another now, and the records
    /// there are the new one's: they name copies in the directory it holds.
    ///
    /// One transaction, taken before anything is read, so a relink at the
    /// same moment comes wholly before this or wholly after it.
    fn knew(&self, seen: &[(ProjectId, String)], stale: &[String]) -> Result<usize, LedgerError>;

    /// Move every record of the project at `old` to `new`, as the project
    /// whose id is `id`, running `write` inside the same transaction: what it
    /// moved, by the count of installations, once `write` returns true, and
    /// nothing at all when it returns false, everything undone. The ledger's
    /// [`relinked`](crate::ledger::Ledger::relinked) is this with a write of
    /// any type; a store is handed it erased, so the port stays a trait
    /// object.
    fn relinked_by(
        &self,
        old: &str,
        new: &str,
        id: &ProjectId,
        write: &mut dyn FnMut() -> bool,
    ) -> Result<Option<usize>, LedgerError>;

    /// The sources whose stored URL still carries credentials, which only a
    /// Mindweaver from before they were refused could have stored.
    ///
    /// Each is handed back with the URL [`scrub_source`](Self::scrub_source) will give it,
    /// never the one it has: nothing the ledger returns carries a credential.
    fn tainted_sources(&self) -> Result<Vec<Source>, LedgerError>;

    /// Store a source's URL without the credentials in it.
    ///
    /// Two rows it would make one are one already: the URL a source is
    /// known by holds no credential ([`crate::url::canonical`]), so a row
    /// whose URL only a credential told from another's was made one with it
    /// by the step to schema 7, which kept the one whose shelf was filled
    /// most recently ([`Merged`]). Before that step the scrub made them one
    /// itself, by the same rule.
    ///
    /// An installation's origin is rewritten with the URL it spells: schema 3
    /// filled it in from the URL as it was stored, credentials and all, and
    /// nothing else would take them out of it.
    fn scrub_source(&self, id: i64) -> Result<(), LedgerError>;

    /// Rewrite every action log entry without the credentials of any URL in
    /// it, returning how many there were.
    fn scrub_log(&self) -> Result<usize, LedgerError>;

    /// Append to the action log.
    ///
    /// Failures are logged too. A log that only records what worked cannot
    /// answer the question anybody actually opens it with.
    fn log(
        &self,
        action: Action,
        target: Option<&str>,
        outcome: Outcome,
        detail: Option<&str>,
    ) -> Result<(), LedgerError>;

    /// The most recent actions, newest first.
    fn history(&self, limit: usize) -> Result<Vec<Entry>, LedgerError>;
}

#[cfg(test)]
#[path = "port_test.rs"]
mod tests;
