//! The workspace's record of what it gathered, from where, and what it did.
//!
//! Everything else Mindweaver writes is a file a person is meant to open: the
//! marker files are TOML with comments explaining themselves. This is the one
//! thing that is not, because it answers questions a file cannot: which of two
//! identically named skills came from which repository, at which revision, and
//! what happened the last four times a gather was run. It sits beside
//! `weaver.toml` in `.mindweaver/`, so it travels with the workspace it
//! describes and two workspaces never share a history.
//!
//! Nothing here decides *what* is worth gathering. It records what
//! [`crate::gather`] did.
//!
//! What it records is asked of a store through a port ([`LedgerStore`]):
//! SQLite unless another is handed over ([`Ledger::new`]), the adapter in
//! [`crate::adapter::sqlite`] holding every query and the schema. The types
//! here are what a store hands back, the same whichever one it is.
//!
//! Nor does it write a credential. Every URL is [`redacted`] on the way in,
//! whichever caller is writing, and again on the way out, for a row a
//! Mindweaver from before the redaction wrote, which stays in the file until
//! a command that writes scrubs it: the ledger is the one place everything
//! stored passes through, so it is where the guarantee is cheapest to keep.

use std::fmt;
use std::io;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::adapter::sqlite::Sqlite;
use crate::agent::Agent;
use crate::code::codes;
use crate::kind::Kind;
use crate::port::{Clock, LedgerStore};
use crate::settings::ProjectId;
// What the docs link to, which no code here calls since the store redacts.
#[cfg(doc)]
use crate::url::redacted;
use crate::value::{ArtifactName, ContentHash, Origin, Redacted, Revision, SourceName};

/// The schema version this build writes, and the copy kept of an older
/// one before a step: the SQLite store's, named here where they always were.
pub use crate::adapter::sqlite::{backup_path, SCHEMA_VERSION};

/// The file name, under `.mindweaver/`.
pub const LEDGER_FILE: &str = "mindweaver.db";
/// [`LEDGER_FILE`] as a Mindflayer build named it, under the `.mindflayer/`
/// of a workspace one made ([`crate::names::Names::Mindflayer`]): the same
/// schema, opened and migrated as it is, under its own name until the
/// workspace is moved.
pub const FLAYER_LEDGER_FILE: &str = "mindflayer.db";

/// What kind of place an artifact was gathered from.
///
/// A closed enum, like [`Kind`]: every source ships in this crate, so adding
/// the next one is a variant the compiler then asks about everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// A git repository, cloned into the workspace cache.
    Git,
}

impl SourceKind {
    /// How it is written in the database and in a report.
    pub const fn slug(self) -> &'static str {
        match self {
            SourceKind::Git => "git",
        }
    }

    pub(crate) fn parse(slug: &str) -> Option<Self> {
        match slug {
            "git" => Some(SourceKind::Git),
            _ => None,
        }
    }
}

/// By its slug, for the reason a [`Kind`] is: the word the database and a
/// report already hold, which a renamed variant cannot change.
impl Serialize for SourceKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.slug())
    }
}

#[cfg(any(feature = "ts", feature = "schema"))]
crate::described::words! {
    /// What kind of place a source is.
    SourceKind by slug { Git }
}

/// A place artifacts have been gathered from.
///
/// What it serializes to is a contract, as a status report's is: each field
/// by its name, but the row id, and the URL only through [`redacted`]. Its
/// `name` was added after the rest (#98), which moved no version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Source {
    /// Its row id, which is what gathered artifacts point at.
    ///
    /// Not serialized: it is the ledger's, numbered as rows were written, so
    /// no front end can name a source by it on a command line, and the URL
    /// and the branch already tell one source from another.
    #[serde(skip)]
    pub id: i64,
    pub kind: SourceKind,
    /// The URL as the user typed it, without the credentials [`redacted`]
    /// takes out.
    pub url: Redacted,
    /// The branch or tag asked for, or `None` for whatever the remote's HEAD
    /// points at.
    pub reference: Option<String>,
    /// The folder inside the source that was harvested, `skills` by default.
    pub subdirectory: String,
    /// The commit last gathered from, once one has been resolved.
    pub revision: Option<Revision>,
    /// The folder this source owns under `.mindweaver/<kind folder>/`.
    ///
    /// Stored rather than derived: two URLs can reduce to the same readable
    /// name, and where a source's files went is a fact about the past that
    /// must not move when the naming rule changes.
    pub directory: String,
    /// What it is called (D-13), one name a source: `owner-repo` unless
    /// somebody named it ([`SourceName::derived`]), what a listing shows
    /// and `--from` takes.
    pub name: SourceName,
}

impl Source {
    /// Where what it supplied came from, as one string: its address, with
    /// the branch after a `#` when one was asked for. What `--from` takes
    /// for it, and what an install from it records as its origin
    /// ([`crate::install::Offer::origin`]), written here once so a listing
    /// of the sources spells what an install of one of theirs does.
    pub fn origin(&self) -> Origin {
        match &self.reference {
            Some(reference) => Origin::from(format!("{}#{reference}", self.url)),
            None => Origin::from(self.url.as_str()),
        }
    }
}

/// A source the step to schema 7 made one with another it found to be the
/// same place: one repository's URL spelled two ways
/// ([`crate::url::canonical`]), gathered at one branch from one folder.
///
/// The row that went, as it was, beside the one it is one with now: what it
/// gathered went with it, and its folders are nobody's
/// ([`crate::workspace::WeaverWorkspace::ledger_to_write`] removes them and
/// says so). Every installation from it is from `into` now, its origin
/// rewritten to `into`'s, so an update finds what it came from there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    /// The URL it was gathered from, without the credentials [`redacted`]
    /// takes out: what `--from` named it by, which still names `into`
    /// ([`crate::install::Offer::comes_from`]).
    pub url: Redacted,
    /// The branch or tag it was asked at, `into`'s too.
    pub reference: Option<String>,
    /// The folder it had under `cache/` and each kind's shelf.
    pub directory: String,
    /// The source it is one with: the one whose shelf was filled most
    /// recently, since its folder holds the newer files.
    pub into: Source,
}

/// One gathered artifact, and the source it came from.
///
/// What it serializes to is a contract, as a status report's is: each field
/// by its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Gathered {
    pub kind: Kind,
    pub name: ArtifactName,
    /// Where it sits, relative to the workspace root, `/` separated.
    pub path: String,
    /// The one line a listing shows, as it read when it was gathered.
    pub summary: Option<String>,
    /// The revision it was taken at.
    pub revision: Option<Revision>,
    /// When it was gathered, in seconds since the Unix epoch.
    pub gathered_at: i64,
    pub source: Source,
}

/// Something the user asked for, whether or not it worked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Harvesting artifacts from a source into the workspace.
    Gather,
    /// Copying one from the workspace shelf into a mind project.
    Install,
    /// Taking one back out of a project.
    Uninstall,
    /// Following a project the workspace links to where it moved.
    Relink,
    /// Bringing a skill somebody kept in a project into the library, its
    /// copies recorded as installed from there.
    Adopt,
}

impl Action {
    pub const fn slug(self) -> &'static str {
        match self {
            Action::Gather => "gather",
            Action::Install => "install",
            Action::Uninstall => "uninstall",
            Action::Relink => "relink",
            Action::Adopt => "adopt",
        }
    }
}

/// How an action ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Failed,
}

impl Outcome {
    pub const fn slug(self) -> &'static str {
        match self {
            Outcome::Ok => "ok",
            Outcome::Failed => "failed",
        }
    }
}

/// What the ledger records of one installation into a project: what went
/// in, from where, and every copy or entry it wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRecord {
    pub kind: Kind,
    pub name: ArtifactName,
    /// The shelf source it was copied from, while the ledger still has it;
    /// `None` for the workspace's own and a loaded workspace's.
    pub source_id: Option<i64>,
    /// Where it came from, spelled as `--from` takes it
    /// ([`crate::install::Offer::origin`]), without the credentials
    /// [`redacted`] takes out. `None` only in a record from before schema 3
    /// of something that came from no source.
    pub origin: Option<Origin>,
    /// [`crate::hash::tree`] of what was written, which every target holds.
    /// `None` in a record from before schema 3, which D-04 reads as current
    /// only while the copy matches its offer byte for byte.
    pub content_hash: Option<ContentHash>,
    /// Every copy it wrote, by path.
    pub targets: Vec<Target>,
    /// Every entry it wrote into an agent's config file, one an agent: an
    /// MCP server's, which is written as entries rather than copies (D-10).
    /// None for a skill's or a rule's.
    pub entries: Vec<EntryTarget>,
}

/// One copy an installation wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The one agent the copy is for, or `None` for a copy no one agent owns:
    /// the folder a v2 project's marker names, or `.agents/skills`, which two
    /// agents read.
    pub agent: Option<String>,
    /// Where the copy is, relative to the project, `/` separated.
    pub path: String,
    /// What this copy holds, when it is not what the installation's
    /// `content_hash` names: a rule's section in `AGENTS.md` and its file
    /// for Claude hold different bytes, each with a hash of its own. `None`
    /// for a skill's copies, every one of which holds the tree the
    /// installation's hash names.
    pub content_hash: Option<ContentHash>,
}

/// One entry an MCP server's installation wrote into an agent's config file
/// (D-10): what a copy is to a skill's.
///
/// Apart from [`Target`], which is a copy found by its path, one path each:
/// an entry is found by its key in a file every server of the project
/// shares, so what tells Mindweaver's from somebody's is the key in that
/// file, and what tells it untouched is the fingerprint of what the key
/// holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryTarget {
    /// The agent whose config file it is in.
    pub agent: Agent,
    /// The file, as a route from the project, `/` separated, as
    /// [`Agent::mcp_file`] named it when the entry was written.
    pub file: String,
    /// The key the entry is under in that file: the server's name.
    pub name: ArtifactName,
    /// [`crate::hash::fingerprint`] of the entry as it was written.
    pub fingerprint: ContentHash,
}

/// Where one installation's copies are once its project has moved from a
/// `mind.toml` to its settings ([`migrated`](LedgerStore::migrated)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub kind: Kind,
    pub name: ArtifactName,
    /// Every copy it has now, in place of those it recorded; none forgets it.
    pub targets: Vec<Target>,
    /// What every one of them holds ([`crate::hash::tree`]), for a record
    /// from before hashes were kept, which the migration found current by
    /// its offer: kept only where the record has none.
    pub content_hash: Option<ContentHash>,
}

/// What [`uninstalled_copy`](LedgerStore::uninstalled_copy) forgot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forgotten {
    /// No installation of that name records a copy there.
    Nothing,
    /// The copy, from an installation that still records others.
    Copy,
    /// The installation, whose last copy it was, or which recorded none.
    Installation,
}

/// What [`renamed_copy`](LedgerStore::renamed_copy) did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carried {
    /// No installation of that name records a copy there.
    Nothing,
    /// The copy is recorded under its new name and at its new place.
    Moved,
    /// Another installation holds the new name, so the copy is recorded by
    /// neither, and its installation goes with it when it was the last.
    Forgotten,
}

/// One line of the action log.
///
/// What it serializes to is a contract, as a status report's is: each field
/// by its name, and the two that can quote a URL as a [`Redacted`], which
/// holds none. A row an older build wrote before the scrub still holds one
/// in the file until a command that writes opens the ledger, and is
/// redacted as it is read ([`history`](LedgerStore::history)).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Entry {
    /// When it happened, in seconds since the Unix epoch.
    pub at: i64,
    pub action: String,
    /// What the action was about: a URL, a name. A URL is [`redacted`].
    pub target: Option<Redacted>,
    pub outcome: String,
    /// What it said, successful or not, with every URL in it [`redacted`].
    pub detail: Option<Redacted>,
}

/// The workspace's database: what a workspace records, asked in core's
/// words, whatever stores it.
///
/// What every caller holds, as it held the SQLite store this was before it
/// became a port: each question and each write is a method of the store it
/// holds ([`LedgerStore`]), reached through `Deref`, so a caller writes
/// `ledger.records(..)` and names no store. Every constructor here opens the
/// SQLite one ([`Sqlite`]); another store is handed over whole
/// ([`Ledger::new`]), and nothing behind the port changes for it.
pub struct Ledger(Box<dyn LedgerStore>);

impl Ledger {
    /// A ledger over `store`, a store other than SQLite.
    pub fn new(store: Box<dyn LedgerStore>) -> Self {
        Ledger(store)
    }

    /// Open the ledger at `path`, creating and migrating it if needed.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, LedgerError> {
        Sqlite::open(path).map(Self::over)
    }

    /// [`open`](Self::open), with `known` asked, only when a step to schema 5
    /// is taken, for the id each project it may hold records of holds and
    /// the route the ledger names it by: what a workspace's linked projects
    /// say ([`crate::workspace::WeaverWorkspace::ledger`]), which no SQL in
    /// the file can know.
    pub fn open_knowing(
        path: impl Into<PathBuf>,
        known: impl FnOnce() -> Vec<(ProjectId, String)>,
    ) -> Result<Self, LedgerError> {
        Sqlite::open_knowing(path, known).map(Self::over)
    }

    /// Open the ledger at `path` only if it is there: for a command that only
    /// reads it, which is no reason to create a database, or the folder it
    /// would sit in.
    ///
    /// One that is there is still brought up to [`SCHEMA_VERSION`], as
    /// [`open`](Self::open) brings it. A build reads its own schema and no
    /// other, so declining to migrate would leave an older ledger unreadable
    /// rather than untouched: the step is upkeep of Mindweaver's own file, not
    /// a change anybody asked for, and the one write a read may make.
    pub fn open_existing(path: impl Into<PathBuf>) -> Result<Option<Self>, LedgerError> {
        Ok(Sqlite::open_existing(path)?.map(Self::over))
    }

    /// [`open_existing`](Self::open_existing), with `known` asked as
    /// [`open_knowing`](Self::open_knowing) asks it.
    pub fn open_existing_knowing(
        path: impl Into<PathBuf>,
        known: impl FnOnce() -> Vec<(ProjectId, String)>,
    ) -> Result<Option<Self>, LedgerError> {
        Ok(Sqlite::open_existing_knowing(path, known)?.map(Self::over))
    }

    /// An in-memory ledger, for tests and for anything that must not persist.
    pub fn in_memory() -> Result<Self, LedgerError> {
        Sqlite::in_memory().map(Self::over)
    }

    /// Stamp what it writes by `clock` rather than
    /// [`SystemClock`](crate::adapter::SystemClock): the port the ledger
    /// reads the time through, as a fixture pins it.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.0.stamp_by(clock);
        self
    }

    /// Record that the project the ledger knew at the route `old` is at
    /// `new` now, holding `id`: every installation and file made recorded
    /// there moves with it, and `id` is known there. `write`, the change to
    /// the workspace's marker that goes with it, runs inside the same
    /// transaction, which is kept only when it succeeds.
    ///
    /// So the entry and the records move together or not at all: an entry
    /// moved without them would leave them under a route no entry names,
    /// and records moved without it would name a directory the workspace
    /// does not link. What was recorded at `new` of some other project,
    /// one linked there once and unlinked, goes first: it names copies in a
    /// directory that holds `id`'s project now, and the records moving in
    /// would collide with it, name for name.
    ///
    /// Which project `old` was, and whether `new` holds it, is the caller's
    /// to have checked ([`crate::relink`]); this only moves the rows. Taken
    /// before anything is read, as [`knew`](LedgerStore::knew) is, and logged. Hands
    /// back what `write` did and how many installations moved, or what it
    /// failed with, nothing moved.
    pub fn relinked<T, E>(
        &self,
        old: &str,
        new: &str,
        id: &ProjectId,
        write: impl FnOnce() -> Result<T, E>,
    ) -> Result<Result<(T, usize), E>, LedgerError> {
        let mut write = Some(write);
        let mut written = None;
        let moved = self.0.relinked_by(old, new, id, &mut || {
            let result = write.take().map(|write| write());
            let kept = matches!(result, Some(Ok(_)));
            written = result;
            kept
        })?;
        Ok(match written {
            Some(Ok(written)) => Ok((
                written,
                moved.expect("a store keeps what it moved once the write it ran succeeded"),
            )),
            Some(Err(error)) => Err(error),
            None => {
                unreachable!("a store that moved a project's records ran the write it was handed")
            }
        })
    }

    fn over(store: Sqlite) -> Self {
        Ledger(Box::new(store))
    }
}

impl Deref for Ledger {
    type Target = dyn LedgerStore;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl fmt::Debug for Ledger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Ledger").field(&self.0).finish()
    }
}

/// Why the ledger could not be opened, read or written.
#[derive(Debug, Error)]
pub enum LedgerError {
    /// The workspace could not be had to scrub clones in, opening its ledger
    /// to write ([`crate::workspace::WeaverWorkspace::ledger_to_write`]).
    #[error(transparent)]
    Lock(#[from] crate::lock::LockError),
    #[error("{path}: cannot be created: {source}")]
    Create {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// `source` is what the store said, boxed so the port names no store's
    /// error type: SQLite's, from the adapter there is.
    #[error("{path}: cannot be opened: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("{path}: written by a newer Mindweaver (schema version {found}, this one reads {SCHEMA_VERSION})")]
    Version { path: PathBuf, found: u32 },
    #[error("{path}: cannot be copied to {backup} before its schema changes, so it is left as it is: {source}")]
    Backup {
        path: PathBuf,
        backup: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: no workspace has been made here yet, so there is no ledger to write to")]
    Unmade { path: PathBuf },
    /// What the store said, boxed as an opening's is.
    #[error("{path}: {doing} failed: {source}")]
    Query {
        path: PathBuf,
        doing: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

codes!(LedgerError {
    Self::Create { .. } => "ledger_create_failed",
    Self::Open { .. } => "ledger_open_failed",
    Self::Version { .. } => "ledger_too_new",
    Self::Backup { .. } => "ledger_backup_failed",
    Self::Unmade { .. } => "ledger_unmade",
    Self::Query { .. } => "ledger_query_failed",
} carried {
    Self::Lock(error) => error,
});

impl LedgerError {
    /// The file the failure is about.
    pub fn path(&self) -> &Path {
        match self {
            LedgerError::Lock(error) => error.path(),
            LedgerError::Create { path, .. }
            | LedgerError::Open { path, .. }
            | LedgerError::Version { path, .. }
            | LedgerError::Backup { path, .. }
            | LedgerError::Unmade { path }
            | LedgerError::Query { path, .. } => path,
        }
    }
}

#[cfg(test)]
#[path = "ledger_test.rs"]
mod tests;
