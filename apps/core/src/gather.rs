//! Harvesting artifacts from somewhere else into a weaver workspace.
//!
//! Gathering fills the workspace's own shelf and stops there. Nothing is
//! written into a mind project: which of the gathered skills a project should
//! carry is a separate decision, made later and by somebody, and doing it here
//! would mean a `git clone` silently editing repositories.
//!
//! What lands where: a source's clone goes to
//! `.mindweaver/cache/<source>/`, and each artifact it yields is copied to
//! `.mindweaver/<kind folder>/<source>/<name>/`. Namespacing by source is what
//! lets two repositories both offer `commit-style`; [`crate::ledger`] is what
//! remembers which is which.
//!
//! Where in a source its skills are is read from the layouts repositories
//! use ([`LAYOUTS`]), and what was typed for it from the spellings `gh skill`
//! and `npx skills` take ([`crate::url::located`]), both #101's.

mod watch;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::ser::{SerializeStruct, Serializer};
use serde::Serialize;
use thiserror::Error;

use crate::adapter::gix::Gix;
use crate::artifact::{Artifact, ArtifactError, ValidationIssue};
use crate::code::codes;
use crate::copy::{self, Change};
use crate::hash;
use crate::install::{self, LeftOut};
use crate::kind::{Kind, Layout};
use crate::ledger::{self, Action, Ledger, LedgerError, Merged, Outcome, SourceKind, Taken};
use crate::licence::Licence;
use crate::lock::LockError;
use crate::paths;
use crate::port::{Cloned, GitError, Remote, Token};
use crate::url::{carries_credentials, located, over_ssh, redacted};
use crate::value::{ArtifactName, ContentHash, Origin, Pin, Redacted, Revision, SourceName};
use crate::workspace::WeaverWorkspace;

pub use watch::{Progress, Watch};

/// The folder inside a source that is harvested unless another is named:
/// its root, where every one of [`LAYOUTS`] is looked for.
///
/// `.`, the route the root is, rather than an empty one, so a source
/// shows a folder wherever its folder is shown, and `--path .` is the
/// same source as no `--path`.
pub const DEFAULT_SUBDIRECTORY: &str = ".";

/// What a gather takes unless it is told another kind
/// ([`Request::taking`]): skills, all a gather took before rules (#111).
pub const DEFAULT_KIND: Kind = Kind::Skill;

/// The folder a gather of `kind` looks in when none is named: the
/// repository's root for skills, which are found wherever in it they are
/// kept ([`LAYOUTS`]), and none for rules, whose folder is named (#111).
///
/// None for rules because every markdown file under the folder looked in
/// is one, named by its route from there: a whole repository read as rules
/// would take whatever it documents itself with, a changelog and a guide
/// for contributors among them, and name each rule after the folder its
/// collection is kept in. `--path .` still names the root, when that is
/// where a repository keeps them.
///
/// Matched on the kind, so a third one has to say where it is looked for.
pub fn default_folder(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Skill => Some(DEFAULT_SUBDIRECTORY),
        Kind::Rule | Kind::Mcp => None,
    }
}

/// The folder an older build harvested unless another was named, before a
/// gather looked through the whole repository (#101): a source registered
/// from it, at the URL and branch a gather naming no folder is of, is that
/// gather's source ([`LedgerStore::set_source_folder`]).
///
/// [`LedgerStore::set_source_folder`]: crate::port::LedgerStore::set_source_folder
pub const OLDER_DEFAULT_SUBDIRECTORY: &str = "skills";

/// Where to gather from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A git repository, at a branch or tag, or at whatever its HEAD names.
    Git {
        url: String,
        reference: Option<String>,
    },
}

/// As an object holding its kind, as the ledger files it, and what that kind
/// is given: for a git repository its URL, only through [`redacted`], and
/// the branch or tag asked for.
///
/// By hand rather than derived: a derive would tag it with the variant's Rust
/// name, where the kind's slug is the word the ledger and a report already
/// hold. The URL is redacted here even though [`gather`] refuses one that
/// carries a credential, because a request is built before it is refused.
impl Serialize for Source {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Source::Git { url, reference } => {
                let mut source = serializer.serialize_struct("Source", 3)?;
                source.serialize_field("kind", &self.kind())?;
                source.serialize_field("url", &redacted(url))?;
                source.serialize_field("reference", reference)?;
                source.end()
            }
        }
    }
}

impl Source {
    /// Which kind of source this is, as the ledger files it.
    pub fn kind(&self) -> SourceKind {
        match self {
            Source::Git { .. } => SourceKind::Git,
        }
    }

    /// The address, as the user typed it: credentials included, which is why
    /// [`gather`] refuses one that carries any before it is stored or shown.
    pub fn url(&self) -> &str {
        match self {
            Source::Git { url, .. } => url,
        }
    }

    /// The branch or tag asked for, if any.
    pub fn reference(&self) -> Option<&str> {
        match self {
            Source::Git { reference, .. } => reference.as_deref(),
        }
    }
}

/// One gather: what to take, from where, and out of which folder, and who is
/// following it.
#[derive(Debug, Clone)]
pub struct Request {
    pub source: Source,
    /// The folder inside the source to harvest, as somebody named it: a
    /// route from its root, or `None` for the one a gather of its kind
    /// looks in when none is named ([`default_folder`]), which a kind may
    /// have none of. A URL that names one says it instead
    /// ([`crate::url::located`]).
    pub subdirectory: Option<String>,
    /// What to look for there: skills unless told otherwise
    /// ([`taking`](Self::taking)).
    pub kind: Kind,
    /// Told how the gather is going, and able to stop it. A request carries
    /// it, rather than [`gather`] taking it beside one, so a front end that
    /// builds requests in one place and runs them in another hands over both
    /// together.
    pub watch: Watch,
    /// What fetches the source: [`Gix`] unless another [`Remote`] is handed
    /// over ([`through`](Self::through)), as a test hands a fake.
    pub remote: Arc<dyn Remote>,
    /// What to call the source, as somebody typed it: `owner-repo` when
    /// none is given ([`SourceName::derived`]), and a source gathered
    /// before called this from now on when one is.
    pub name: Option<String>,
    /// The commit to take, as somebody typed it, in place of whatever the
    /// branch is at (#99): held to a commit in full ([`Pin::parse`]) before
    /// anything is fetched, and what the source is pinned to once it is
    /// gathered, as none is when it is gathered without one.
    pub pin: Option<String>,
    /// A token the front end was handed for this gather (D-16), which the
    /// remote offers the repository's host as the login, before git
    /// config's credential helpers; refused for a URL it would not be sent
    /// over ([`token_host`]). `None` logs in as the helpers and ssh do.
    pub token: Option<Token>,
}

impl Request {
    /// Gather skills from a git repository, wherever in it they are kept.
    pub fn git(url: impl Into<String>) -> Self {
        Self {
            source: Source::Git {
                url: url.into(),
                reference: None,
            },
            subdirectory: None,
            kind: DEFAULT_KIND,
            watch: Watch::new(),
            remote: Arc::new(Gix),
            name: None,
            pin: None,
            token: None,
        }
    }

    /// Take only what is in `subdirectory`, a route from the source's root,
    /// rather than from the whole of it.
    pub fn from_subdirectory(mut self, subdirectory: impl Into<String>) -> Self {
        self.subdirectory = Some(subdirectory.into());
        self
    }

    /// Take artifacts of `kind` rather than skills: rules, each a markdown
    /// file in the folder named, at any depth (#111).
    pub fn taking(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }

    /// Clone that branch or tag rather than the remote's HEAD.
    pub fn at(mut self, reference: Option<impl Into<String>>) -> Self {
        let reference = reference.map(Into::into);
        match &mut self.source {
            Source::Git { reference: at, .. } => *at = reference,
        }
        self
    }

    /// Call the source `name`, when one is given, rather than what its URL
    /// names it.
    pub fn named(mut self, name: Option<impl Into<String>>) -> Self {
        self.name = name.map(Into::into);
        self
    }

    /// Take the commit `pin`, when one is given, rather than whatever the
    /// branch is at now.
    pub fn pinned(mut self, pin: Option<impl Into<String>>) -> Self {
        self.pin = pin.map(Into::into);
        self
    }

    /// Report to `watch`, and stop when it is cancelled.
    pub fn watched_by(mut self, watch: Watch) -> Self {
        self.watch = watch;
        self
    }

    /// Fetch through `remote` rather than [`Gix`]: the port a gather's one
    /// step outside core goes through, everything after it the same.
    pub fn through(mut self, remote: Arc<dyn Remote>) -> Self {
        self.remote = remote;
        self
    }

    /// Log in with `token`, when one is given, rather than as the
    /// credential helpers and ssh do.
    pub fn with_token(mut self, token: Option<Token>) -> Self {
        self.token = token;
        self
    }

    /// Gather the artifacts of `kind` of `source` again, as the ledger
    /// records it (#100): its URL as it was stored, at its branch, from its
    /// folder, and at its pin while it has one, so a pinned source takes its
    /// commit again rather than whatever its branch is at, which only
    /// `gather git` without `--pin` asks for. Its name is the one it has, so
    /// none is given.
    ///
    /// Found again by the same coordinate ([`known`]), so it gathers into
    /// the source it is, its folder on the shelf and its name kept. Its
    /// folder is always named, the one it was registered from, so a source
    /// of rules from the root is gathered from there again rather than
    /// refused for naming none.
    ///
    /// A marketplace (#103) is its catalog's repository at its branch, which
    /// is all a gather of one fetches before it reads the catalog: it is
    /// gathered again through [`crate::marketplace`], which takes from this
    /// how its front end fetches, its watch, its remote and its token, and
    /// so is a plugin it lists, through its marketplace.
    pub fn again(source: &ledger::Source, kind: Kind) -> Self {
        match source.kind {
            SourceKind::Git => Request::git(source.url.as_str())
                .taking(kind)
                .at(source.reference.clone())
                .from_subdirectory(source.subdirectory.clone())
                .pinned(source.pin.as_ref().map(|pin| pin.as_str().to_owned())),
            SourceKind::Marketplace => {
                Request::git(source.url.as_str()).at(source.reference.clone())
            }
        }
    }
}

/// The host a token for a gather of `typed` goes to, as a front end asks
/// before it fetches one for that host alone (`gh auth token --hostname`):
/// the host of the repository [`located`] reads `typed` as, refused when
/// that is fetched over nothing a token goes over
/// ([`token_host`](crate::url::token_host)), and when `typed` carries a
/// credential of its own, which a gather refuses first whatever it is
/// handed.
///
/// The same refusal a gather handed a token makes, so a front end that
/// asks here first runs nothing to fetch a token no gather would take.
pub fn token_host(typed: &str) -> Result<String, GatherError> {
    if carries_credentials(typed) {
        return Err(GatherError::CredentialsInUrl {
            url: redacted(typed).into_owned(),
        });
    }
    sent_to(&located(typed).url)
}

/// The host a token for a fetch of `url`, as [`located`] reads it, goes
/// to, or why it goes to none.
fn sent_to(url: &str) -> Result<String, GatherError> {
    crate::url::token_host(url).ok_or_else(|| {
        let url = redacted(url).into_owned();
        if over_ssh(&url) {
            GatherError::TokenOverSsh { url }
        } else {
            GatherError::TokenNotHttps { url }
        }
    })
}

/// One artifact that reached the workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Harvested {
    /// The name it declares, which is what it will be asked for by.
    pub name: ArtifactName,
    /// The name of the folder it came from inside the source, which is the
    /// folder it kept; for a skill at the source's root, which has no
    /// folder of its own there, the name it declares. For a rule, its
    /// file's route from the folder gathered from, which the shelf files it
    /// by (#111).
    pub directory: String,
    /// The one line a listing shows.
    pub summary: Option<String>,
    /// Where it now sits in the workspace.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// Where it is in the source: its folder's route from the source's
    /// root, `/` separated, `.` for the root itself, or a rule's file's.
    /// Added after the rest (#101), since a source keeps its skills in more
    /// than one place.
    pub route: String,
}

/// Something a skill holds that its copy on the shelf was taken without,
/// and why ([`LeftOut`]): its `.claude-plugin`, which an install leaves out
/// of every copy too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unshelved {
    /// The skill, by the name it declares.
    pub name: ArtifactName,
    /// What it holds that was left out, where it is in the source.
    pub left: LeftOut,
}

/// What one gather did.
///
/// Split three ways rather than counted once, because "3 skills" hides the
/// only question worth asking after a second gather: what changed.
///
/// What it serializes to is a contract, as a status report's is: each field
/// by its name.
#[derive(Debug, Serialize)]
pub struct Report {
    /// The revision that was gathered from, when there was one.
    pub revision: Option<Revision>,
    /// The folder under `.mindweaver/<kind>/` this source owns.
    pub directory: String,
    pub added: Vec<Harvested>,
    pub updated: Vec<Harvested>,
    pub unchanged: Vec<Harvested>,
    /// Artifacts in the source that could not be taken. One broken skill must
    /// not cost the forty next to it.
    ///
    /// Not serialized, nor `warnings`: a front end says both as its
    /// warnings, which in JSON are the envelope's, as a scan's failures are.
    #[serde(skip)]
    pub failures: Vec<Failure>,
    /// What is wrong with artifacts that were taken anyway.
    #[serde(skip)]
    pub warnings: Vec<Warning>,
    /// What the source is called, which `--from` takes for what it
    /// supplied: added after the rest (#98).
    pub name: SourceName,
    /// What the shelf's copies were taken without (#101), which a front end
    /// says as a note: not serialized, as the warnings are not.
    #[serde(skip)]
    pub left_out: Vec<Unshelved>,
    /// The commit it was gathered at when that was a pin, which the source
    /// is pinned to now: added after the rest (#99).
    pub pin: Option<Pin>,
    /// What was gathered, skills or rules, which a front end counts them
    /// as: added after the rest (#111).
    pub kind: Kind,
    /// What the shelf held of the source that it offers no more, taken off
    /// the shelf (#100): each by the name it was gathered under, at the
    /// path it had there, and where in the source it was, empty for one a
    /// build from before #99 gathered. Added after the rest.
    pub removed: Vec<Harvested>,
}

impl Report {
    /// Everything that is now on the shelf because of this gather.
    pub fn total(&self) -> usize {
        self.added.len() + self.updated.len() + self.unchanged.len()
    }

    /// Whether the source yielded nothing at all.
    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

/// Gather what `request` asks for into `workspace`, recording it in `ledger`.
///
/// The ledger is passed in rather than opened here so a caller can hold one
/// open across several gathers, and so tests can use one that never touches a
/// disk.
pub fn gather(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    request: &Request,
) -> Result<Report, GatherError> {
    // Held for the whole gather, the clone included, until its line is in
    // the log: two gathers of one source at once would clone into one folder
    // and each replace the shelf copies the other was making. One that cannot
    // have it is not logged, because no gather was attempted.
    let _lock = workspace.lock()?;
    let outcome = harvest(workspace, ledger, request);
    // What was fetched, `owner/repo` read as the GitHub repository it names
    // and a folder's address on a forge as its repository.
    let url = located(request.source.url()).url;

    // The log records what was attempted, not only what worked: a gather that
    // failed is the entry somebody opens the log to find. A log write that
    // fails on the error path is swallowed, because the error it would replace
    // is the more useful of the two.
    match &outcome {
        Ok(report) => {
            // What was removed only when something was, so the line a gather
            // that removed nothing logs reads as it did before one could.
            let removed = match report.removed.len() {
                0 => String::new(),
                removed => format!(", {removed} removed"),
            };
            let detail = format!(
                "{} added, {} updated, {} unchanged{removed}, {} failed",
                report.added.len(),
                report.updated.len(),
                report.unchanged.len(),
                report.failures.len()
            );
            ledger.log(Action::Gather, Some(&url), Outcome::Ok, Some(&detail))?;
        }
        // Not logged: a cancel leaves the ledger as it found it, and the
        // person who cancelled knows already.
        Err(GatherError::Cancelled) => {}
        Err(error) => {
            let _ = ledger.log(
                Action::Gather,
                Some(&url),
                Outcome::Failed,
                Some(&error.to_string()),
            );
        }
    }
    outcome
}

/// What the scrub [`WeaverWorkspace::ledger_to_write`] runs took out.
#[derive(Debug, Default)]
pub struct Scrub {
    /// The sources whose URL carried credentials, as they are stored now.
    /// Gathering one again needs the credential from somewhere else.
    pub sources: Vec<ledger::Source>,
    /// How many action log entries named a URL with credentials.
    pub actions: usize,
    /// Clones that still hold them, because they could not be rewritten.
    /// Their source keeps its URL until they can, so the next scrub tries
    /// again rather than forgetting the clone needs it.
    pub failures: Vec<ScrubFailure>,
    /// Folders of a source that became one with another, which nothing
    /// points into any more and which could not be removed.
    pub leftovers: Vec<Leftover>,
    /// The sources the step to schema 7 made one with another ([`Merged`]):
    /// said by the first command that writes, whichever command took the
    /// step, as what the scrub takes out is, and again by each after it
    /// while a folder of one will not go ([`Self::leftovers`]).
    pub merged: Vec<Merged>,
}

impl Scrub {
    /// Whether there was nothing to take out, which is every time but the
    /// first after an older build stored a credential.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
            && self.actions == 0
            && self.failures.is_empty()
            && self.leftovers.is_empty()
            && self.merged.is_empty()
    }

    /// Add what a later scrub took out, for a change that opened the ledger
    /// to write more than once ([`crate::lifecycle::rename`]).
    ///
    /// A clone that could not be rewritten keeps its source's URL, so every
    /// scrub tries it again and fails again: one thing to tell, as two
    /// branches of one URL are one credential.
    pub(crate) fn absorb(&mut self, later: Scrub) {
        for source in later.sources {
            if !self.sources.iter().any(|seen| seen.url == source.url) {
                self.sources.push(source);
            }
        }
        self.actions += later.actions;
        for failure in later.failures {
            if !self.failures.iter().any(|seen| seen.url == failure.url) {
                self.failures.push(failure);
            }
        }
        self.leftovers.extend(later.leftovers);
        self.merged.extend(later.merged);
    }
}

/// Take out the credentials a Mindweaver from before they were refused
/// stored: in the URLs of sources, in the action log, and in the clones it
/// made from those URLs.
///
/// Run by [`WeaverWorkspace::ledger_to_write`], which is how a command that
/// writes opens the ledger, so it comes before anything is written. Every
/// time rather than once, as a schema migration would, because an older build
/// can keep writing to the same file after this one has scrubbed it, and the
/// schema steps are spoken for by the changes that need a new version. With
/// nothing to do it still scans the sources and the action log, and the first
/// time it finds something is the only time it says so. A read is left alone:
/// what it shows is redacted on the way out anyway, and reading should not
/// write.
///
/// First it removes the clone and the shelf folders of each source the step
/// to schema 7 made one with another
/// ([`LedgerStore::merged`](crate::port::LedgerStore::merged)), and hands
/// back each merge to be said. Removed rather than left, because the shelf
/// is read from its folders ([`crate::Catalog::discover_gathered`]): one
/// left behind would be listed as skills no source owns, and its clone
/// keeps whatever credential it was made with. Here, rather than by
/// whichever command took the step, because that is a read as often as
/// not, which says nothing and removes nothing: the ledger keeps each merge
/// until a command that writes has removed its folders, so a folder that
/// will not go is tried again, and said again with its merge, each time.
///
/// The workspace is held ([`WeaverWorkspace::lock_waiting`], up to `wait`)
/// while clones are rewritten or removed, since a gather beside it would be
/// cloning into those same folders. Only then: with no clone to rewrite or
/// remove, which is every time but the first after an older build stored a
/// credential or an upgrade made two sources one, the scrub writes nothing
/// but rows, which SQLite keeps apart, and a change to an artifact opening
/// the ledger to drop a record does not wait behind a gather to do it. The
/// sources are looked for again once it is held, since a scrub kept waiting
/// may find that another has done its work.
///
/// Each clone is scrubbed through `remote`, the adapter that knows which of
/// its files name the URL it was made from.
pub(crate) fn scrub(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    wait: Duration,
    remote: &dyn Remote,
) -> Result<Scrub, LedgerError> {
    let mut scrub = Scrub::default();
    let _lock = if ledger.tainted_sources()?.is_empty() && ledger.merged()?.is_empty() {
        None
    } else {
        Some(workspace.lock_waiting(wait)?)
    };
    for merged in ledger.merged()? {
        let left = scrub.leftovers.len();
        remove_folders_of(workspace, &merged.directory, &mut scrub.leftovers);
        if scrub.leftovers.len() == left {
            ledger.cleared_merge(&merged.directory)?;
        }
        scrub.merged.push(merged);
    }
    for source in ledger.tainted_sources()? {
        // The clone first and the row after it: a row rewritten first would
        // no longer say that its clone still needs the same.
        if let Err(error) = remote.scrub(&workspace.cache_dir().join(&source.directory)) {
            scrub.failures.push(ScrubFailure {
                url: source.url.clone(),
                source: error,
            });
            continue;
        }
        ledger.scrub_source(source.id)?;
        // Two branches or folders of one URL are one credential to tell
        // somebody about.
        if !scrub.sources.iter().any(|seen| seen.url == source.url) {
            scrub.sources.push(source);
        }
    }
    scrub.actions = ledger.scrub_log()?;
    Ok(scrub)
}

/// Remove the clone and the shelf folders of a source called `directory`
/// that the ledger no longer has, noting each that will not go.
///
/// A directory that is not one folder name was not named by this ledger, and
/// is left alone rather than followed out of the workspace.
fn remove_folders_of(workspace: &WeaverWorkspace, directory: &str, leftovers: &mut Vec<Leftover>) {
    if !crate::install::is_folder_name(directory) {
        return;
    }
    let shelves = Kind::ALL
        .iter()
        .map(|kind| workspace.gathered_dir(*kind).join(directory));
    for path in std::iter::once(workspace.cache_dir().join(directory)).chain(shelves) {
        match fs::remove_dir_all(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => leftovers.push(Leftover { path, source }),
        }
    }
}

/// A folder of a source that became one with another, which could not be
/// removed.
#[derive(Debug, Error)]
#[error(
    "{path}: left by a source that is now one with another at the same URL, and cannot be \
     removed: {source}"
)]
pub struct Leftover {
    pub path: PathBuf,
    #[source]
    pub source: io::Error,
}

codes!(Leftover => "source_leftover");

/// A clone made from a URL with credentials that could not be rewritten
/// without them.
#[derive(Debug, Error)]
#[error("{url}: its clone still holds the credentials it was gathered with: {source}")]
pub struct ScrubFailure {
    /// The source's URL, without them.
    pub url: Redacted,
    #[source]
    pub source: GitError,
}

codes!(ScrubFailure => "clone_unscrubbed");

/// What a gather would take into `workspace`, and what of it the shelf
/// holds already, with nothing written: not the shelf, not the ledger, and
/// not the source's clone in the cache (#101's `--dry-run`).
///
/// The source is fetched all the same, since what it offers now is what is
/// asked, into a folder of this run's own in the workspace's scratch folder
/// (`.mindweaver/tmp`), gone once this returns, whatever it returns. Each
/// skill is copied there as it would be onto the shelf, so a copy a gather
/// would refuse is refused here too, and compared with what the shelf holds.
/// `ledger` is only read: a front end hands over one opened for reading,
/// which a workspace that never gathered does not make. Nor is the
/// workspace held, since nothing a gather beside it writes is written here:
/// one that finishes while this runs can leave what it says of the shelf
/// out of date, as it can any read's.
///
/// What it hands back is what [`gather`] would: each skill under what a
/// gather would do with it, at the path it would be put at, with what is
/// wrong with it, and the source by the name it has or would be given. What
/// could not be read is named by its route in the source, since the clone it
/// was read from is gone.
pub fn dry_run(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    request: &Request,
) -> Result<Report, GatherError> {
    let (typed, finding) = typed(request)?;
    if request.watch.is_cancelled() {
        return Err(GatherError::Cancelled);
    }
    let known = known(ledger, request.source.kind(), &typed)?;
    if let Some(name) = &typed.name {
        taken(ledger, name, known.as_ref().map(|(source, _)| source))?;
    }
    let known = known.map(|(source, _)| source);
    let (name, directory) = match &known {
        Some(source) => (source.name.clone(), source.directory.clone()),
        None => ledger.would_register(&typed.url)?,
    };
    let name = typed.name.clone().unwrap_or(name);
    let scratch = workspace
        .scratch_dir()
        .join(format!("gather-{}", copy::unique_name()));
    let foreseen = Foreseen {
        typed: &typed,
        finding,
        scratch: &scratch,
        known: known.as_ref(),
    };
    let outcome = foresee(workspace, ledger, request, &foreseen, name, directory);
    // Whatever came of it: nothing of a dry run is kept. One that will not
    // go is left in a folder no agent reads, under a name no other run is
    // given.
    let _ = fs::remove_dir_all(&scratch);
    outcome
}

/// What a dry run is of, beside the request: what was typed for it, how a
/// gather finds its kind, the folder of its run's own it fetches into, and
/// the source it is of, when one was registered.
struct Foreseen<'a> {
    typed: &'a Typed,
    finding: Finding,
    scratch: &'a Path,
    known: Option<&'a ledger::Source>,
}

/// A dry run once its source is known: fetched into its scratch folder,
/// read, and each skill copied there and compared with the shelf's copy of
/// it, each rule's file compared with the shelf's as it reads, and what
/// the shelf holds of it that it offers no more found as a gather would
/// take it off ([`gone`]).
fn foresee(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    request: &Request,
    foreseen: &Foreseen,
    name: SourceName,
    directory: String,
) -> Result<Report, GatherError> {
    let Foreseen {
        typed,
        finding,
        scratch,
        known,
    } = *foreseen;
    let clone = fetch(request, typed, &scratch.join("clone"))?;
    let shelf = workspace.gathered_dir(request.kind).join(&directory);
    let mut report = Report {
        revision: clone.revision.clone(),
        directory,
        added: Vec::new(),
        updated: Vec::new(),
        unchanged: Vec::new(),
        failures: Vec::new(),
        warnings: Vec::new(),
        name,
        left_out: Vec::new(),
        pin: typed.pin.clone(),
        kind: request.kind,
        removed: Vec::new(),
    };
    let mut failures = Vec::new();
    let examined = examine(&clone, typed, finding, &request.watch, &mut failures);
    report.failures = failures
        .into_iter()
        .map(|failure| failure.routed(&clone.root))
        .collect();
    let Examined { found, filed } = examined?;
    for found in found {
        let destination = shelf.join(&found.filed);
        let change = match found.layout() {
            // Copied as a gather would put it on the shelf, so a copy a
            // gather would refuse is refused here too.
            Layout::Directory { .. } => {
                let copy = scratch.join("shelf").join(&found.filed);
                copy::replace_leaving(&found.from, &copy, &clone.root, &left_out_of_shelf())
                    .and_then(|_| copy::compared(&copy, &destination))
            }
            // Read as a gather reads it, which is all a gather does with it.
            Layout::Files { .. } | Layout::Flat { .. } => {
                copy::compared_file(&found.from, &destination)
            }
        };
        let change = match change {
            Ok(change) => change,
            Err(source) => {
                report.failures.push(Failure::Copy {
                    path: destination,
                    source,
                });
                continue;
            }
        };
        noted(&mut report, &found, &destination);
        let harvested = found.harvested(destination);
        match change {
            Change::Added => report.added.push(harvested),
            Change::Updated => report.updated.push(harvested),
            Change::Unchanged => report.unchanged.push(harvested),
        }
    }
    if let Some(source) = known {
        let before = shelved_of(ledger, source, request.kind)?;
        report.removed = gone(workspace, source, request.kind, &report, &filed, before)
            .into_iter()
            .filter_map(|gone| gone.harvested)
            .collect();
    }
    Ok(ordered(report))
}

/// `report` in the order it is read in: each skill by its name, then by
/// where it is in the source, rather than in the order the source was
/// walked, which a source keeping skills in several layouts mixes; and
/// what could not be taken, and what is wrong, by path.
fn ordered(mut report: Report) -> Report {
    for harvested in [
        &mut report.added,
        &mut report.updated,
        &mut report.unchanged,
        &mut report.removed,
    ] {
        harvested.sort_by(|a, b| (&a.name, &a.route).cmp(&(&b.name, &b.route)));
    }
    report.failures.sort_by(|a, b| a.path().cmp(b.path()));
    // Stable, so one file's warnings stay in the order `validate` gives them.
    report.warnings.sort_by(|a, b| a.path.cmp(&b.path));
    report
}

/// What a request comes to once what was typed for it is read: the
/// repository to fetch, the branch, the folder as a route from the
/// repository's root ([`route_of`]), and what to call the source, each held
/// to its rule, beside the kind it takes.
#[derive(Debug, Clone)]
struct Typed {
    /// What is fetched ([`located`]).
    url: String,
    kind: Kind,
    reference: Option<String>,
    path: String,
    name: Option<SourceName>,
    /// The commit to take in place of what the branch is at.
    pin: Option<Pin>,
}

/// Read what `request` was typed with, refusing what no gather takes before
/// anything is registered or fetched, beside how a gather finds its kind
/// ([`gatherable`]).
fn typed(request: &Request) -> Result<(Typed, Finding), GatherError> {
    // Refused before anything is registered or fetched, rather than used for
    // this one fetch and dropped from what is stored: a token on the command
    // line is already where `ps` and the shell history show it, and every
    // report names the URL. A git credential helper, or a token the front
    // end hands over ([`Request::token`]), keeps it off the command line
    // altogether.
    //
    // So is a URL typed where the branch, the folder or the name goes,
    // which the ledger keeps as that branch or folder, word for word, and an
    // error repeats: it is somebody's credential all the same.
    let given = [
        Some(request.source.url()),
        request.source.reference(),
        request.subdirectory.as_deref(),
        request.name.as_deref(),
        request.pin.as_deref(),
    ];
    if let Some(carrying) = given
        .into_iter()
        .flatten()
        .find(|text| carries_credentials(text))
    {
        return Err(GatherError::CredentialsInUrl {
            url: redacted(carrying).into_owned(),
        });
    }

    // What the URL says of the branch and the folder, beside what was given
    // for them: one of each, since which of two a gather is of is not a
    // thing to guess.
    let at = located(request.source.url());
    // A token is refused for a repository it would not be sent to, rather
    // than kept unused: one handed over for an ssh URL, or a path, says the
    // gather is not logging in the way somebody thinks it is.
    if request.token.is_some() {
        sent_to(&at.url)?;
    }
    let reference = match (at.reference, request.source.reference()) {
        (Some(named), Some(given)) if named != given => {
            return Err(GatherError::ReferenceTwice {
                named,
                given: given.to_owned(),
            })
        }
        (Some(named), _) => Some(named),
        (None, given) => given.map(str::to_owned),
    };
    // A kind no gather takes is refused before what was typed for its
    // folder is read, since no folder would hold it.
    let finding = gatherable(request.kind)?;
    let given = match &request.subdirectory {
        Some(typed) => Some(route_of(typed).ok_or_else(|| GatherError::PathOutside {
            path: typed.clone(),
        })?),
        None => None,
    };
    let path = match (at.path, given) {
        (Some(named), given) => {
            let route = route_of(&named).ok_or_else(|| GatherError::PathOutside {
                path: named.clone(),
            })?;
            if given.is_some_and(|given| given != DEFAULT_SUBDIRECTORY && given != route) {
                return Err(GatherError::PathTwice {
                    named,
                    given: request.subdirectory.clone().unwrap_or_default(),
                });
            }
            route
        }
        (None, Some(given)) => given,
        // Named by neither: the folder a gather of the kind looks in, or a
        // refusal for a kind that has none, before anything is fetched.
        (None, None) => default_folder(request.kind)
            .map(str::to_owned)
            .ok_or(GatherError::FolderUnnamed { kind: request.kind })?,
    };

    let name = match &request.name {
        Some(typed) => Some(
            SourceName::parse(typed).ok_or_else(|| GatherError::NameInvalid {
                name: typed.clone(),
            })?,
        ),
        None => None,
    };
    // A commit in full, or nothing is fetched: a short one, a branch or a
    // tag would be resolved to whatever it names on the day, which a pin
    // is there to stop.
    let pin = match &request.pin {
        Some(typed) => {
            Some(Pin::parse(typed).ok_or_else(|| GatherError::PinInvalid { pin: typed.clone() })?)
        }
        None => None,
    };

    Ok((
        Typed {
            url: at.url,
            kind: request.kind,
            reference,
            path,
            name,
            pin,
        },
        finding,
    ))
}

/// How a gather finds a kind in the folder it looks in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Finding {
    /// Each folder holding `manifest` where one of [`LAYOUTS`] looks, a
    /// folder with everything in it: a skill.
    Folders { manifest: &'static str },
    /// Each file ending in `extension` at any depth, named by its route
    /// from there as a project's are ([`crate::catalog::route`]): a rule
    /// (#111).
    Files { extension: &'static str },
}

/// How a gather finds `kind` in a source, as the kind is laid out: a skill
/// as a folder holding its manifest, a rule as a markdown file at any depth.
/// An MCP server is gathered from nowhere yet.
///
/// Matched on the kind rather than on its layout, so the compiler keeps
/// asking: a third kind has to answer this before it can be added.
fn gatherable(kind: Kind) -> Result<Finding, GatherError> {
    let refused = GatherError::NotGatherable { kind };
    match kind {
        Kind::Skill | Kind::Rule => match kind.layout() {
            Layout::Directory { manifest } => Ok(Finding::Folders { manifest }),
            Layout::Files { extension } => Ok(Finding::Files { extension }),
            Layout::Flat { .. } => Err(refused),
        },
        Kind::Mcp => Err(refused),
    }
}

/// `typed`, a folder in a repository, as the route from its root a source
/// is registered with: `/` separated, without a `.`, an empty part or a
/// trailing `/`, and [`DEFAULT_SUBDIRECTORY`] for the root itself. `None`
/// for one that climbs out of the repository with `..`, or starts at a root
/// of its own.
///
/// So `skills/`, `./skills` and `skills` are one source's folder, however a
/// folder's address on a forge or `--path` spells it, and a folder a gather
/// takes from is inside the clone, unless a link in the clone leads it out,
/// which is looked for once there is a clone ([`examine`]).
pub(crate) fn route_of(typed: &str) -> Option<String> {
    let mut parts = Vec::new();
    for component in Path::new(typed).components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(if parts.is_empty() {
        DEFAULT_SUBDIRECTORY.to_owned()
    } else {
        parts.join("/")
    })
}

/// The source a gather of `typed` is of, when one was registered: the one
/// at its coordinate, or, for a gather that names no folder, one an older
/// build registered from [`OLDER_DEFAULT_SUBDIRECTORY`] at the same URL and
/// branch, which `true` says.
///
/// That one rather than a second source of one repository: its shelf holds
/// what is under `skills/` already, filed as a gather of the whole
/// repository files it, and what was installed from it names it by an
/// origin that holds no folder, which two sources would share. Only for a
/// gather of skills, which is all an older build gathered: a gather of
/// rules from the root names its folder, and is no older gather's.
fn known(
    ledger: &Ledger,
    kind: SourceKind,
    typed: &Typed,
) -> Result<Option<(ledger::Source, bool)>, LedgerError> {
    let reference = typed.reference.as_deref();
    if let Some(found) = ledger.source(kind, &typed.url, reference, &typed.path)? {
        return Ok(Some((found, false)));
    }
    if typed.path != DEFAULT_SUBDIRECTORY || typed.kind != Kind::Skill {
        return Ok(None);
    }
    Ok(ledger
        .source(kind, &typed.url, reference, OLDER_DEFAULT_SUBDIRECTORY)?
        .map(|older| (older, true)))
}

/// Refuse `name` when a source other than `found` has it: one name is one
/// source, and which of two a `--from` named is not something to guess.
fn taken(
    ledger: &Ledger,
    name: &SourceName,
    found: Option<&ledger::Source>,
) -> Result<(), GatherError> {
    if let Some(holder) = ledger.source_named(name)? {
        if found.is_none_or(|found| found.id != holder.id) {
            return Err(GatherError::NameTaken {
                name: name.clone(),
                origin: holder.origin(),
            });
        }
    }
    Ok(())
}

/// The work itself, with the logging left to [`gather`].
fn harvest(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    request: &Request,
) -> Result<Report, GatherError> {
    let (typed, finding) = typed(request)?;

    // Before the source is registered, so a gather cancelled before it began
    // has nothing to undo.
    if request.watch.is_cancelled() {
        return Err(GatherError::Cancelled);
    }

    // Found by the URL every spelling of it is ([`crate::url::canonical`]),
    // so a second spelling gathers into the source the first registered,
    // and registered as fetched. A name is refused before anything is
    // registered.
    let kind = request.source.kind();
    let found = known(ledger, kind, &typed)?;
    if let Some(name) = &typed.name {
        taken(ledger, name, found.as_ref().map(|(source, _)| source))?;
    }
    let new = found.is_none();
    // A source an older build gathered from `skills`, its default then, is
    // this gather's from now on, and the whole repository is its folder.
    let refolded = match &found {
        Some((older, true)) => {
            ledger.set_source_folder(older.id, &typed.path)?;
            true
        }
        _ => false,
    };
    let reference = typed.reference.as_deref();
    let mut source = ledger.source_for(kind, &typed.url, reference, &typed.path)?;
    let was = source.name.clone();
    if let Some(name) = typed.name.clone().filter(|name| *name != source.name) {
        ledger.rename_source(source.id, &name)?;
        source.name = name;
    }

    let outcome = take(workspace, ledger, request, &typed, &source, finding);
    if !matches!(outcome, Err(GatherError::Cancelled)) {
        return outcome;
    }
    // What a cancel leaves is the ledger as it was: the source called what
    // it was, and taken from the folder it was.
    if !new && source.name != was {
        ledger.rename_source(source.id, &was)?;
    }
    if refolded {
        ledger.set_source_folder(source.id, OLDER_DEFAULT_SUBDIRECTORY)?;
    }
    if new {
        // Undone rather than never done: the clone needs the folder name that
        // registering gives it before anything is fetched. Holding the
        // registration in a transaction until the gather ends was the other
        // way, and is rejected: every other write to the ledger would wait
        // for as long as the network takes.
        ledger.forget_source(source.id)?;
        // Nothing points at its clone any more. One that will not go is left
        // for the next source given its folder name, whose clone replaces
        // whatever is there, as every clone does.
        let _ = fs::remove_dir_all(workspace.cache_dir().join(&source.directory));
    }
    outcome
}

/// Fetch what `typed` names into `into`, telling the request's watch.
///
/// From the URL typed this time rather than the one the source was first
/// registered with, though both are one repository's: the second spelling
/// is the one somebody asked for now, an ssh one where https needs a
/// credential the helpers lack, or the reverse.
fn fetch(request: &Request, typed: &Typed, into: &Path) -> Result<Cloned, GatherError> {
    request.watch.report(Progress::Connecting);
    match &request.source {
        Source::Git { .. } => request
            .remote
            .fetch(
                &typed.url,
                typed.reference.as_deref(),
                typed.pin.as_ref(),
                into,
                &request.watch,
                request.token.as_ref(),
            )
            .map_err(|error| match error {
                GitError::Cancelled => GatherError::Cancelled,
                error => error.into(),
            }),
    }
}

/// Fetch `source`, read what it offers, and put that on the shelf: the
/// gather, once its source is registered.
fn take(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    request: &Request,
    typed: &Typed,
    source: &ledger::Source,
    finding: Finding,
) -> Result<Report, GatherError> {
    let watch = &request.watch;
    let cache = workspace.cache_dir().join(&source.directory);
    let clone = fetch(request, typed, &cache)?;

    let shelf = workspace.gathered_dir(request.kind).join(&source.directory);
    let mut report = Report {
        revision: clone.revision.clone(),
        directory: source.directory.clone(),
        added: Vec::new(),
        updated: Vec::new(),
        unchanged: Vec::new(),
        failures: Vec::new(),
        warnings: Vec::new(),
        name: source.name.clone(),
        left_out: Vec::new(),
        pin: typed.pin.clone(),
        kind: request.kind,
        removed: Vec::new(),
    };

    // Everything is read before anything is placed, rather than each
    // artifact read and placed in turn, so that the shelf only starts to
    // change once all of it is known.
    let Examined { found, filed } = examine(&clone, typed, finding, watch, &mut report.failures)?;
    // What the shelf held of it before this gather records anything, which
    // is what it may take off the shelf once it has ([`gone`]).
    let before = shelved_of(ledger, source, request.kind)?;

    // The last moment a cancel is honoured, and the reason everything is read
    // first: past here the shelf and the ledger change, and a gather stopped
    // halfway through that would leave them neither as they were nor as the
    // source has them. `Progress::cancellable` says so of every step after.
    if watch.is_cancelled() {
        return Err(GatherError::Cancelled);
    }
    // Noted only now, and not by a gather that fails before it gets here,
    // since the revision a source names is the one its shelf holds, and so
    // is the pin, or the lack of one: a source is pinned while its shelf
    // holds what a pin took.
    ledger.saw_source(source.id, clone.revision.as_deref(), typed.pin.as_ref())?;

    let total = found.len();
    watch.report(Progress::Placing { done: 0, total });
    for (done, one) in found.into_iter().enumerate() {
        let destination = shelf.join(&one.filed);
        let placed = one.shelve(&destination, &clone.root);
        watch.report(Progress::Placing {
            done: done + 1,
            total,
        });
        let change = match placed {
            Ok(change) => change,
            Err(source) => {
                report.failures.push(Failure::Copy {
                    path: destination,
                    source,
                });
                continue;
            }
        };
        noted(&mut report, &one, &destination);

        // What was placed, as an install hashes what it wrote, so the two
        // are told by one recipe: the shelf's copy leaves out what every
        // install's copy does. One that cannot be read back is recorded
        // with no hash rather than not at all, and said.
        let content_hash = match one.hashed(&destination) {
            Ok(hash) => Some(hash),
            Err(source) => {
                report.failures.push(Failure::Unhashed {
                    path: destination.clone(),
                    source,
                });
                None
            }
        };
        ledger.record(
            source,
            &Taken {
                kind: request.kind,
                name: one.artifact.name().to_owned(),
                path: relative(workspace, &destination),
                summary: one.artifact.summary().map(str::to_owned),
                revision: clone.revision.clone(),
                upstream_path: one.route.clone(),
                content_hash,
                licence: one.licence(&destination),
            },
        )?;

        let harvested = one.harvested(destination);
        match change {
            Change::Added => report.added.push(harvested),
            Change::Updated => report.updated.push(harvested),
            Change::Unchanged => report.unchanged.push(harvested),
        }
    }

    // Once what it offers is placed, what it offers no more goes: a skill
    // whose folder upstream is gone would otherwise be offered from the
    // shelf for good, and what was installed from it never found orphaned.
    for gone in gone(workspace, source, request.kind, &report, &filed, before) {
        if let Some(at) = &gone.at {
            match unshelve(request.kind, at, &shelf) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                // Kept, row and all, so the shelf and the ledger still say
                // one thing of it, and the next gather tries again.
                Err(error) => {
                    report.failures.push(Failure::Unremoved {
                        path: at.clone(),
                        source: error,
                    });
                    continue;
                }
            }
        }
        if let Some(harvested) = gone.harvested {
            ledger.forget_gathered(source, request.kind, &harvested.name)?;
            report.removed.push(harvested);
        }
    }

    Ok(ordered(report))
}

/// Take what `kind` keeps at `at` off the shelf of a source, `shelf`: a
/// skill's folder whole, or a rule's file, and then each folder it was in
/// that it leaves empty, up to the source's own and not that, so a rule
/// gone leaves no folder of nothing behind.
fn unshelve(kind: Kind, at: &Path, shelf: &Path) -> io::Result<()> {
    match kind.layout() {
        Layout::Directory { .. } => fs::remove_dir_all(at),
        Layout::Files { .. } | Layout::Flat { .. } => {
            fs::remove_file(at)?;
            let mut folder = at.parent();
            while let Some(here) = folder.filter(|here| *here != shelf && here.starts_with(shelf)) {
                // One that still holds something stops the climb: what is
                // in it is another rule's.
                if fs::remove_dir(here).is_err() {
                    break;
                }
                folder = here.parent();
            }
            Ok(())
        }
    }
}

/// What the ledger records of `source`'s `kind` on the shelf.
fn shelved_of(
    ledger: &Ledger,
    source: &ledger::Source,
    kind: Kind,
) -> Result<Vec<ledger::Gathered>, LedgerError> {
    Ok(ledger
        .gathered()?
        .into_iter()
        .filter(|gathered| gathered.source.id == source.id && gathered.kind == kind)
        .collect())
}

/// What a gather of a source takes off the shelf of it ([`gone`]): an
/// artifact it offers no more, and what of it goes with it, or what is
/// filed alone, left behind by an artifact it still offers from elsewhere.
struct Gone {
    /// What the report says of it, which is forgotten: `None` for what is
    /// filed alone, whose artifact is offered still.
    harvested: Option<Harvested>,
    /// Where it is on the shelf, a skill's folder or a rule's file, taken
    /// away; `None` when it is to be forgotten and that kept: a place the
    /// gather put another in now, or one the ledger names somewhere other
    /// than below the source's own folder on the shelf, which is nothing a
    /// gather of it put there.
    at: Option<PathBuf>,
}

/// What the shelf held of `source`'s `kind` before `report`, a gather of
/// it, that the gather did not take and it offers no more, from `before`,
/// what the ledger recorded of it then.
///
/// Each is told by where the shelf files it below the source's folder, a
/// skill's folder name or a rule's route (`Found::filed`), read off the
/// path recorded. Gone is one the source no longer has there: none of
/// `filed`, where every one the gather found is filed, taken or not. One
/// the gather found and could not take, a read that failed or a name two
/// share, is left as it was, as its warning says, rather than taken for
/// gone. `filed` is `None` when one of them could not be told, a skill at
/// the root that would not read, and then nothing is gone but what the
/// next paragraph says.
///
/// Gone too, its place kept, is a name the gather took nothing of whose
/// place it put another in: a skill renamed where it is, whose old row
/// would otherwise name the new one's folder. And a skill the gather took
/// from another folder than the one it was in, moved upstream, leaves its
/// old folder behind, which goes alone, its row the new folder's now.
fn gone(
    workspace: &WeaverWorkspace,
    source: &ledger::Source,
    kind: Kind,
    report: &Report,
    filed: &Option<HashSet<String>>,
    before: Vec<ledger::Gathered>,
) -> Vec<Gone> {
    let taken: Vec<&Harvested> = report
        .added
        .iter()
        .chain(&report.updated)
        .chain(&report.unchanged)
        .collect();
    let shelf = workspace.gathered_dir(kind).join(&source.directory);
    let below = format!("{}/", relative(workspace, &shelf));
    let mut gone = Vec::new();
    for gathered in before {
        // A row naming somewhere else than below the source's folder is
        // nothing a gather of it filed, and is left as it is.
        let Some(place) = gathered.path.strip_prefix(&below).map(str::to_owned) else {
            continue;
        };
        let offered = taken.iter().any(|one| one.name == gathered.name);
        let placed = taken.iter().any(|one| one.directory == place);
        let still = filed.as_ref().is_none_or(|filed| filed.contains(&place));
        // Only below the source's own folder, by names that climb nowhere,
        // and for a skill one folder down, which is all a gather files.
        let plain = place
            .split('/')
            .all(|part| !matches!(part, "" | "." | ".."));
        let shaped = match kind.layout() {
            Layout::Directory { .. } => install::is_folder_name(&place),
            Layout::Files { .. } | Layout::Flat { .. } => true,
        };
        let own = (plain && shaped).then(|| shelf.join(&place));
        match (offered, placed || still) {
            (true, true) => {}
            (true, false) => gone.push(Gone {
                harvested: None,
                at: own,
            }),
            (false, true) if !placed => {}
            (false, _) => gone.push(Gone {
                at: own.filter(|_| !placed),
                harvested: Some(Harvested {
                    path: workspace.root().join(&gathered.path),
                    name: gathered.name,
                    directory: place,
                    summary: gathered.summary,
                    route: gathered.upstream_path.unwrap_or_default(),
                }),
            }),
        }
    }
    gone
}

/// Say in `report` what is wrong with `found`, put at `destination`, and
/// what it was put there without.
///
/// Lenient, as the Agent Skills client guide advises for skills written
/// for other agents: what is wrong with one is said, and it is taken
/// anyway. Installing it is where it is held to the rules. Judged as it is
/// filed, under the folder the shelf keeps it in, which for a skill at the
/// source's root is not the folder it was read from. A rule is judged as
/// `validate` judges one, its name aside, which no rule is taken without
/// ([`Failure::Unnamed`]).
fn noted(report: &mut Report, found: &Found, destination: &Path) {
    // The file what is wrong is in: a skill's manifest, a rule itself.
    let file = match found.layout() {
        Layout::Directory { manifest } => destination.join(manifest),
        Layout::Files { .. } | Layout::Flat { .. } => destination.to_path_buf(),
    };
    report.warnings.extend(
        found
            .artifact
            .validate_filed_as(&found.filed)
            .into_iter()
            .map(|issue| Warning {
                path: file.clone(),
                issue,
            }),
    );
    // Matched on the layout: what an install leaves out is at the top of a
    // folder, which a file has none of.
    match found.layout() {
        Layout::Directory { .. } => {
            report
                .left_out
                .extend(
                    install::left_out(&found.from)
                        .into_iter()
                        .map(|left| Unshelved {
                            name: found.artifact.name().to_owned(),
                            left,
                        }),
                )
        }
        Layout::Files { .. } | Layout::Flat { .. } => {}
    }
}

/// What no copy on the shelf holds from the top of a skill's folder: what an
/// install leaves out of every copy it makes ([`install::left_out`]), so
/// the shelf offers no project what an install would not give it, and
/// git's own folder, which a skill at the root of a source has beside it
/// and is the clone's rather than the skill's.
fn left_out_of_shelf() -> Vec<&'static str> {
    let mut names = install::left_out_names().to_vec();
    names.push(".git");
    names
}

/// An artifact found in a source: where it is in the clone, where the shelf
/// files it below the source's folder, its route from the source's root,
/// what it declares, and the folder the gather looked in.
///
/// A skill is a folder, filed under its name in the source; a rule is a
/// file, filed by its route from the folder looked in, as it is named.
struct Found {
    /// A skill's folder in the clone, or a rule's file.
    from: PathBuf,
    /// Below the source's folder on the shelf: a skill's folder name, a
    /// rule's file by its route from the folder looked in, as the source
    /// spells it (`python/style.md`).
    filed: String,
    route: String,
    artifact: Artifact,
    /// The folder looked in, in the clone, whose licence file a rule is
    /// under.
    looked_in: PathBuf,
}

impl Found {
    /// What the report says of it, once it is at `path`.
    fn harvested(self, path: PathBuf) -> Harvested {
        Harvested {
            name: self.artifact.name().to_owned(),
            directory: self.filed,
            summary: self.artifact.summary().map(str::to_owned),
            path,
            route: self.route,
        }
    }

    /// How it is laid out, which is how it is put on the shelf.
    fn layout(&self) -> Layout {
        self.artifact.kind().layout()
    }

    /// Put it on the shelf at `to`, replacing what was there: a skill's
    /// folder whole, without what no copy on the shelf holds
    /// ([`left_out_of_shelf`]), its links followed only inside `clone`; a
    /// rule's file as it reads, never written through a link at `to`.
    fn shelve(&self, to: &Path, clone: &Path) -> Result<Change, io::Error> {
        match self.layout() {
            Layout::Directory { .. } => {
                copy::replace_leaving(&self.from, to, clone, &left_out_of_shelf())
            }
            Layout::Files { .. } | Layout::Flat { .. } => copy::replace_file(&self.from, to),
        }
    }

    /// Its copy at `at`, hashed by the recipe an install keeps of what it
    /// writes there: a folder's tree ([`hash::tree`]), a file's bytes
    /// ([`hash::bytes`]), as a rule's install records each copy of it.
    fn hashed(&self, at: &Path) -> Result<ContentHash, io::Error> {
        match self.layout() {
            Layout::Directory { .. } => hash::tree(at),
            Layout::Files { .. } | Layout::Flat { .. } => {
                fs::read(at).map(|bytes| hash::bytes(&bytes))
            }
        }
    }

    /// The licence it says it is under, once it is on the shelf at `at`: a
    /// skill's, read from its folder there ([`Licence::of`]); a rule's,
    /// which says nothing of one itself, the licence file at the top of the
    /// folder it was gathered from, which no rule is taken from (#111).
    fn licence(&self, at: &Path) -> Licence {
        match self.layout() {
            Layout::Directory { .. } => Licence::of(&self.artifact, at),
            Layout::Files { .. } | Layout::Flat { .. } => {
                Licence::of(&self.artifact, &self.looked_in)
            }
        }
    }
}

/// What a source offers in the folder a gather looks in ([`examine`]).
struct Examined {
    /// Each skill or rule read, and not called as another is.
    found: Vec<Found>,
    /// Where the shelf files every one found below the source's folder,
    /// taken or not (`Found::filed`): what a gather takes for what the
    /// source still has ([`gone`]). `None` when one of them could not be
    /// told, a skill at the root that would not read, which is filed under
    /// the name it would declare.
    filed: Option<HashSet<String>>,
}

/// What `clone` offers in the folder `typed` names, each skill or rule
/// read, with what could not be among `failures`: everything a gather knows
/// before the shelf changes.
///
/// The folder is held to being inside the clone once links are followed,
/// since `--path` names one of the repository's, and found to hold one at
/// all: a source that offers nothing where a gather looks is a gather that
/// names the wrong thing, said rather than gathered as nothing, which would
/// leave the shelf holding a revision's skills under a revision that has
/// none.
fn examine(
    clone: &Cloned,
    typed: &Typed,
    finding: Finding,
    watch: &Watch,
    failures: &mut Vec<Failure>,
) -> Result<Examined, GatherError> {
    let root = match typed.path.as_str() {
        DEFAULT_SUBDIRECTORY => clone.root.clone(),
        path => clone.root.join(path),
    };
    if !root.is_dir() {
        return Err(GatherError::NoSuchFolder {
            subdirectory: typed.path.clone(),
            url: typed.url.clone(),
        });
    }
    let unread = |path: &Path| {
        let path = path.to_path_buf();
        move |source| GatherError::Read { path, source }
    };
    let within = fs::canonicalize(&clone.root).map_err(unread(&clone.root))?;
    let (_, inside) =
        copy::followed(&root, std::slice::from_ref(&within)).map_err(unread(&root))?;
    if !inside {
        return Err(GatherError::PathOutside {
            path: typed.path.clone(),
        });
    }

    let candidates = match finding {
        Finding::Folders { manifest } => candidates(&root, manifest)?,
        Finding::Files { extension } => rule_files(&root, extension)?,
    };
    if candidates.is_empty() {
        return Err(GatherError::NothingFound {
            url: typed.url.clone(),
            path: typed.path.clone(),
            kind: typed.kind,
        });
    }
    let total = candidates.len();
    watch.report(Progress::Harvesting { done: 0, total });
    let mut found = Vec::with_capacity(total);
    let mut filed = Some(HashSet::new());
    for (done, candidate) in candidates.iter().enumerate() {
        let read = match finding {
            Finding::Folders { .. } => read(candidate, &clone.root, failures),
            Finding::Files { .. } => read_rule(candidate, &root, &clone.root, failures),
        };
        match read {
            Some((from, place, artifact)) => {
                if let Some(filed) = &mut filed {
                    filed.insert(place.clone());
                }
                let route = paths::relative_to(&from, &clone.root)
                    .as_deref()
                    .and_then(paths::to_config_string)
                    .filter(|route| !route.is_empty())
                    .unwrap_or_else(|| DEFAULT_SUBDIRECTORY.to_owned());
                found.push(Found {
                    from,
                    filed: place,
                    route,
                    artifact,
                    looked_in: root.clone(),
                });
            }
            // Still the source's, though it could not be taken: a skill's
            // folder is the one it is in, but at the root, where it would be
            // the name it declares, which is what could not be read; a
            // rule's file is its route from the folder looked in.
            None => {
                let place = match finding {
                    Finding::Folders { .. } if *candidate == clone.root => None,
                    Finding::Folders { .. } => candidate
                        .file_name()
                        .and_then(|name| name.to_str())
                        .map(str::to_owned),
                    Finding::Files { .. } => candidate
                        .strip_prefix(&root)
                        .ok()
                        .and_then(paths::to_config_string),
                };
                match (place, &mut filed) {
                    (Some(place), Some(filed)) => {
                        filed.insert(place);
                    }
                    (None, _) => filed = None,
                    (Some(_), None) => {}
                }
            }
        }
        watch.report(Progress::Harvesting {
            done: done + 1,
            total,
        });
    }
    Ok(Examined {
        found: unclashed(found, failures),
        filed,
    })
}

/// `found` without every skill another in the same source is called as
/// too, by the folder the shelf would file it under or by the name it
/// declares, each of those a failure naming the others (#101). A rule's
/// file and its name are its route, so two rules clash only by a name the
/// extension's case alone tells apart (`style.md`, `style.MD`).
///
/// Neither is taken: which of them a name means is not a gather's to pick,
/// and taking the first would make which skill a project gets depend on
/// the order a folder is listed in. By the folder because the shelf keeps
/// one folder of a name a source, and by the name because the ledger
/// records one skill of a name a source, which a second would replace. In
/// any case, since a disk that does not tell `Deploy` from `deploy` would
/// hold one folder of both, and a source gathers the same on every disk.
fn unclashed(found: Vec<Found>, failures: &mut Vec<Failure>) -> Vec<Found> {
    let folder = |one: &Found| one.filed.to_ascii_lowercase();
    let declared = |one: &Found| one.artifact.name().as_str().to_ascii_lowercase();
    let mut folders: HashMap<String, usize> = HashMap::new();
    let mut names: HashMap<String, usize> = HashMap::new();
    for one in &found {
        *folders.entry(folder(one)).or_default() += 1;
        *names.entry(declared(one)).or_default() += 1;
    }
    let keys: Vec<(String, String, String)> = found
        .iter()
        .map(|one| (folder(one), declared(one), one.route.clone()))
        .collect();

    let mut kept = Vec::with_capacity(found.len());
    for one in found {
        let (by_folder, by_name) = (folder(&one), declared(&one));
        let shared = folders[&by_folder] > 1;
        if !shared && names[&by_name] == 1 {
            kept.push(one);
            continue;
        }
        let others = keys
            .iter()
            .filter(|(folder, name, route)| {
                *route != one.route && (*folder == by_folder || *name == by_name)
            })
            .map(|(_, _, route)| route.clone())
            .collect();
        // A skill by its folder when that is what is shared; a rule always
        // by its name, which its file is with an extension.
        let name = match (shared, one.layout()) {
            (true, Layout::Directory { .. }) => one.filed.clone(),
            _ => one.artifact.name().to_string(),
        };
        failures.push(Failure::Clash {
            path: one.from,
            route: one.route,
            name,
            others,
        });
    }
    kept
}

/// One candidate folder, loaded to learn its name and its summary: the
/// folder, the name it is filed under, and the artifact. `None`, with the
/// reason among `failures`, when it cannot be taken.
///
/// `clone` is the clone it was found in, which is where it was found and
/// not a claim that a clone is a mind project. A manifest that is a link
/// leading out of it is not read: a source can link its `SKILL.md` to a
/// file of the machine it is gathered on, whose front matter the report
/// would then quote.
fn read(
    directory: &Path,
    clone: &Path,
    failures: &mut Vec<Failure>,
) -> Option<(PathBuf, String, Artifact)> {
    let manifest = directory.join(crate::artifact::manifest_of(Kind::Skill));
    if fs::symlink_metadata(&manifest).is_ok_and(|metadata| metadata.is_symlink()) {
        let outside = fs::canonicalize(clone)
            .ok()
            .zip(fs::canonicalize(&manifest).ok())
            .is_some_and(|(clone, file)| !file.starts_with(clone));
        if outside {
            failures.push(Failure::Outside { path: manifest });
            return None;
        }
    }

    let artifact = match Artifact::skill(directory, clone) {
        Ok(artifact) => artifact,
        Err(error) => {
            failures.push(Failure::Artifact(error));
            return None;
        }
    };

    // The folder keeps the name the source gave it rather than the one the
    // skill declares. A disagreement between the two is something `validate`
    // reports; silently renaming the folder would repair the symptom and hide
    // it. A skill at the source's root has no folder of its own in it, the
    // clone's being named for the source, so it is filed under the name it
    // declares.
    if directory == clone {
        let declared = artifact.name().as_str();
        if !install::is_folder_name(declared) {
            failures.push(Failure::Unfiled {
                path: manifest,
                name: declared.to_owned(),
            });
            return None;
        }
        let folder = declared.to_owned();
        return Some((directory.to_path_buf(), folder, artifact));
    }
    let Some(folder) = directory.file_name().and_then(|name| name.to_str()) else {
        failures.push(Failure::UnusableName {
            path: directory.to_path_buf(),
        });
        return None;
    };
    Some((directory.to_path_buf(), folder.to_owned(), artifact))
}

/// One rule's file, loaded to learn its name and its summary: the file,
/// its route from `root`, the folder looked in, which the shelf files it
/// by, and the rule. `None`, with the reason among `failures`, when it
/// cannot be taken.
///
/// Named by its route from `root` without its extension, the rule a
/// project's rules are named by ([`crate::catalog::route`]), and taken only
/// under a name a rule can have ([`install::is_installable_name`]). Not
/// taken and said otherwise, where a skill declaring a name that breaks the
/// rules is taken and said: a skill's folder keeps the name its source gave
/// it whatever it declares, and a rule's name is the place the shelf files
/// it at, the marker of its section in a project's `AGENTS.md` and the file
/// Claude reads it from, so one that is no name is a rule nothing could
/// ever install. Read as `clone`'s, as a skill is, and not read at all when
/// it is a link leading out of the clone, whose text the report would
/// quote.
fn read_rule(
    file: &Path,
    root: &Path,
    clone: &Path,
    failures: &mut Vec<Failure>,
) -> Option<(PathBuf, String, Artifact)> {
    if fs::symlink_metadata(file).is_ok_and(|metadata| metadata.is_symlink()) {
        let outside = fs::canonicalize(clone)
            .ok()
            .zip(fs::canonicalize(file).ok())
            .is_none_or(|(clone, real)| !real.starts_with(clone));
        if outside {
            failures.push(Failure::Outside {
                path: file.to_path_buf(),
            });
            return None;
        }
    }

    let filed = file
        .strip_prefix(root)
        .ok()
        .and_then(paths::to_config_string);
    let (Some(name), Some(filed)) = (crate::catalog::route(root, file), filed) else {
        failures.push(Failure::UnusableName {
            path: file.to_path_buf(),
        });
        return None;
    };
    let issues = crate::artifact::name_issues(&name);
    if !issues.is_empty() {
        failures.push(Failure::Unnamed {
            path: file.to_path_buf(),
            route: paths::relative_to(file, clone)
                .as_deref()
                .and_then(paths::to_config_string)
                .unwrap_or_else(|| filed.clone()),
            problem: crate::artifact::issue_list(&issues),
        });
        return None;
    }

    match Artifact::rule(file, name, clone) {
        Ok(artifact) => Some((file.to_path_buf(), filed, artifact)),
        Err(error) => {
            failures.push(Failure::Artifact(error));
            None
        }
    }
}

/// Every file ending in `extension` under `root`, the folder rules are
/// gathered from, at any depth, in order, but its README and its licence
/// ([`documents_the_folder`]): a rule each.
///
/// The walk a project's rules are found by ([`crate::Catalog`]): a hidden
/// file or folder is passed over, and so are [`PASSED_OVER`], which a
/// project's walk need not pass over and a repository can hold; a link to
/// a folder is not gone into, since a loop of them would never end and no
/// rule needs one; a link to a file is one more, which [`read_rule`] reads
/// only inside the clone. A folder that cannot be read is a gather that
/// cannot be trusted to have found everything, as it is for skills
/// ([`folders_in`]).
fn rule_files(root: &Path, extension: &str) -> Result<Vec<PathBuf>, GatherError> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(folder) = pending.pop() {
        let unread = |source| GatherError::Read {
            path: folder.clone(),
            source,
        };
        for entry in fs::read_dir(&folder).map_err(unread)? {
            let entry = entry.map_err(unread)?;
            let name = entry.file_name();
            if name.as_encoded_bytes().starts_with(b".")
                || PASSED_OVER.iter().any(|passed| name == *passed)
            {
                continue;
            }
            let path = entry.path();
            if entry.file_type().map_err(unread)?.is_dir() {
                pending.push(path);
                continue;
            }
            // A file, or a link leading to one: a link to a folder, or to
            // nothing, is passed over.
            let is_file = fs::metadata(&path).is_ok_and(|metadata| metadata.is_file());
            if !is_file
                || !crate::catalog::has_extension(&path, extension)
                || (folder == root && documents_the_folder(&name))
            {
                continue;
            }
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Whether a file of this name, directly in the folder rules are gathered
/// from, documents that folder rather than being a rule (#111): its README,
/// or its licence ([`Licence::of`] reads the second as every rule's), in
/// any case, whatever its extension, as a collection is documented.
///
/// Only there: further down, a file of either name is a rule like the
/// rest, as it would be in a project's own rules.
fn documents_the_folder(name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    let stem = name.split('.').next().unwrap_or(name);
    stem.eq_ignore_ascii_case("readme") || crate::licence::is_licence_file(name)
}

/// One step on the way from the folder a gather looks in to a folder of
/// skills ([`LAYOUTS`]): a folder by its name, or each folder there.
#[derive(Debug, Clone, Copy)]
enum Step {
    Named(&'static str),
    Any,
}

/// Where a source keeps its skills, when there is no `SKILL.md` at the root
/// of the folder looked in, which makes that folder the one skill (#101):
/// each the steps from there to a folder every folder of which holding a
/// `SKILL.md` is a skill.
///
/// The layouts `gh skill` and `npx skills` both read: a folder of its own
/// at the root (`<name>/SKILL.md`), `skills/`, a namespace of it
/// (`skills/<ns>/`), its two hidden ones (`.curated`, `.system`), the
/// folders agents read in a project (`.agents/skills`, `.claude/skills`),
/// and a plugin's (`plugins/<plugin>/skills`). What only one of them reads
/// is left out, so a source gathers what both tools would install from it.
///
/// Fixed ways rather than a walk of the whole tree, which is what bounds
/// the depth, four folders down at most. A walk taking every `SKILL.md` it
/// met was the other way, and is rejected: an example a skill ships, or a
/// fixture of a test, is a `SKILL.md` too, which neither tool would take.
const LAYOUTS: [&[Step]; 8] = [
    &[],
    &[Step::Named("skills")],
    &[Step::Named("skills"), Step::Any],
    &[Step::Named("skills"), Step::Named(".curated")],
    &[Step::Named("skills"), Step::Named(".system")],
    &[Step::Named(".agents"), Step::Named("skills")],
    &[Step::Named(".claude"), Step::Named("skills")],
    &[Step::Named("plugins"), Step::Any, Step::Named("skills")],
];

/// What no step of a layout goes into unless it names it: git's own folder
/// and what a package manager installs, which are nobody's skills and can be
/// as large as anything on the disk.
const PASSED_OVER: [&str; 2] = [".git", "node_modules"];

/// The folders under `root` that hold a `manifest`, in order: `root` alone
/// when it holds one itself, and otherwise each a folder of [`LAYOUTS`]
/// holds.
///
/// Whole folders, for the reason discovery has: the directory belongs to the
/// artifact, assets and all, so one inside another is the other's. A folder
/// whose name starts with `.` is passed over unless a layout names it, as
/// are [`PASSED_OVER`]. A link is followed only to a folder inside `root`,
/// which is all `--path` gives a gather, and a folder reached by two ways,
/// a layout's folder linked to another's as a repository links
/// `.claude/skills` to `skills`, is one skill, found where it really is,
/// or at the first of the ways in order when it is under no layout itself.
fn candidates(root: &Path, manifest: &str) -> Result<Vec<PathBuf>, GatherError> {
    let bound = fs::canonicalize(root).map_err(|source| GatherError::Read {
        path: root.to_path_buf(),
        source,
    })?;
    if holds(root, manifest) {
        return Ok(vec![root.to_path_buf()]);
    }

    // Each folder holding a skill, where its links lead and as it was reached.
    let mut found: Vec<(PathBuf, PathBuf)> = Vec::new();
    for layout in LAYOUTS {
        let mut folders = vec![root.to_path_buf()];
        for step in layout {
            let mut next = Vec::new();
            for folder in &folders {
                match step {
                    Step::Named(name) => {
                        let named = folder.join(name);
                        if followed_inside(&named, &bound).is_some() {
                            next.push(named);
                        }
                    }
                    Step::Any => next.extend(
                        folders_in(folder, &bound)?
                            .into_iter()
                            .map(|(_, reached)| reached),
                    ),
                }
            }
            folders = next;
        }
        for folder in &folders {
            found.extend(
                folders_in(folder, &bound)?
                    .into_iter()
                    .filter(|(_, skill)| holds(skill, manifest)),
            );
        }
    }

    found.sort_by(|a, b| a.1.cmp(&b.1));
    // Reached as it is, through no link: its route is the folder's own.
    let as_it_is = |(real, reached): &(PathBuf, PathBuf)| {
        real.strip_prefix(&bound).ok() == reached.strip_prefix(root).ok()
    };
    let mut once: Vec<(PathBuf, PathBuf)> = Vec::with_capacity(found.len());
    for pair in found {
        match once.iter_mut().find(|(seen, _)| *seen == pair.0) {
            None => once.push(pair),
            Some(kept) => {
                if !as_it_is(kept) && as_it_is(&pair) {
                    *kept = pair;
                }
            }
        }
    }
    let mut kept: Vec<PathBuf> = once
        .iter()
        .filter(|(real, _)| {
            !once
                .iter()
                .any(|(other, _)| other != real && real.starts_with(other))
        })
        .map(|(_, reached)| reached.clone())
        .collect();
    kept.sort();
    Ok(kept)
}

/// Whether `folder` holds a `manifest` to read: a file, or a link, which
/// [`read`] judges.
fn holds(folder: &Path, manifest: &str) -> bool {
    fs::symlink_metadata(folder.join(manifest))
        .is_ok_and(|metadata| metadata.is_file() || metadata.is_symlink())
}

/// Where `path` leads once its links are followed, when that is a folder
/// inside `bound`.
fn followed_inside(path: &Path, bound: &Path) -> Option<PathBuf> {
    let (real, inside) = copy::followed(path, std::slice::from_ref(&bound.to_path_buf())).ok()?;
    (inside && real.is_dir()).then_some(real)
}

/// The folders directly in `folder` a layout's step into any of them takes,
/// each where its links lead and as it is reached: none of them hidden or
/// [`PASSED_OVER`], and none leading out of `bound`. None in a folder that
/// is not there, which is an absence; one that cannot be read is a gather
/// that cannot be trusted to have found everything.
fn folders_in(folder: &Path, bound: &Path) -> Result<Vec<(PathBuf, PathBuf)>, GatherError> {
    let unread = |source| GatherError::Read {
        path: folder.to_path_buf(),
        source,
    };
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(unread(error)),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(unread)?;
        let name = entry.file_name();
        if name.as_encoded_bytes().starts_with(b".")
            || PASSED_OVER.iter().any(|passed| name == *passed)
        {
            continue;
        }
        let path = entry.path();
        if let Some(real) = followed_inside(&path, bound) {
            found.push((real, path));
        }
    }
    Ok(found)
}

/// A path as the ledger stores it: relative to the workspace, `/` separated,
/// so a workspace gathered on one platform reads on the other.
fn relative(workspace: &WeaverWorkspace, path: &Path) -> String {
    paths::relative_to(path, workspace.root())
        .as_deref()
        .and_then(paths::to_config_string)
        .unwrap_or_else(|| path.display().to_string())
}

/// One artifact that could not be taken, out of a source that mostly could.
#[derive(Debug, Error)]
pub enum Failure {
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
    #[error("{path}: its name is not valid UTF-8, so it cannot be filed")]
    UnusableName { path: PathBuf },
    #[error("{path}: cannot be copied into the workspace: {source}")]
    Copy {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// Another skill of the same source is called `name` too, by the folder
    /// the shelf would file it under or by the name it declares (#101).
    /// Said by its route in the source rather than its path in the clone,
    /// beside the others' routes, which is what tells them apart and what
    /// `--path` takes.
    #[error(
        "{route}: not taken, since {} in the same source {} called `{name}` too; gather the one \
         wanted with `--path`",
        listed(others),
        if others.len() == 1 { "is" } else { "are" }
    )]
    Clash {
        /// Its folder in the clone, which [`Failure::path`] says.
        path: PathBuf,
        route: String,
        name: String,
        others: Vec<String>,
    },
    /// Its manifest, or a rule's file, is a link that leads out of the
    /// source.
    #[error("{path}: is a link leading out of the source, so it is not read")]
    Outside { path: PathBuf },
    /// A skill at the source's root, which is filed under the name it
    /// declares, declares one that is no folder's.
    #[error(
        "{path}: a skill at the root of a source is filed under the name it declares, and \
         `{name}` is not one folder name"
    )]
    Unfiled { path: PathBuf, name: String },
    /// Put on the shelf, and recorded with no content hash, since its copy
    /// there could not be read back to be hashed (#99).
    #[error("{path}: put on the shelf, but cannot be read back to be hashed: {source}")]
    Unhashed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// A rule's route from the folder looked in, which is its name, is no
    /// name a rule can have (#111): said by its route in the source, and
    /// what is wrong with the name, in `validate`'s words.
    #[error("{route}: not taken, since a rule is named by its route, and {problem}")]
    Unnamed {
        /// Its file in the clone, which [`Failure::path`] says.
        path: PathBuf,
        route: String,
        problem: String,
    },
    /// No more in its source, and its folder or file on the shelf could
    /// not be taken away (#100): kept, and still recorded, until a gather
    /// can.
    #[error("{path}: no longer in its source, and cannot be taken off the shelf: {source}")]
    Unremoved {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

codes!(Failure {
    Self::UnusableName { .. } => "gathered_name_not_utf8",
    Self::Copy { .. } => "gathered_copy_failed",
    Self::Clash { .. } => "gathered_name_clash",
    Self::Outside { .. } => "gathered_link_outside",
    Self::Unfiled { .. } => "gathered_root_unnamed",
    Self::Unhashed { .. } => "gathered_unhashed",
    Self::Unnamed { .. } => "gathered_route_unusable",
    Self::Unremoved { .. } => "gathered_unremoved",
} carried {
    Self::Artifact(error) => error,
});

impl Failure {
    /// The path the failure is about.
    pub fn path(&self) -> &Path {
        match self {
            Failure::Artifact(error) => error.path(),
            Failure::UnusableName { path }
            | Failure::Copy { path, .. }
            | Failure::Clash { path, .. }
            | Failure::Outside { path }
            | Failure::Unfiled { path, .. }
            | Failure::Unhashed { path, .. }
            | Failure::Unnamed { path, .. }
            | Failure::Unremoved { path, .. } => path,
        }
    }

    /// The same failure, its path a route from `root` when it is under it:
    /// what a dry run says of a clone it does not keep ([`dry_run`]).
    fn routed(self, root: &Path) -> Failure {
        let routed = |path: PathBuf| match path.strip_prefix(root) {
            Ok(route) => route.to_path_buf(),
            Err(_) => path,
        };
        match self {
            Failure::Artifact(error) => Failure::Artifact(error.routed(root)),
            Failure::UnusableName { path } => Failure::UnusableName { path: routed(path) },
            Failure::Copy { path, source } => Failure::Copy {
                path: routed(path),
                source,
            },
            Failure::Clash {
                path,
                route,
                name,
                others,
            } => Failure::Clash {
                path: routed(path),
                route,
                name,
                others,
            },
            Failure::Outside { path } => Failure::Outside { path: routed(path) },
            Failure::Unfiled { path, name } => Failure::Unfiled {
                path: routed(path),
                name,
            },
            Failure::Unhashed { path, source } => Failure::Unhashed {
                path: routed(path),
                source,
            },
            Failure::Unnamed {
                path,
                route,
                problem,
            } => Failure::Unnamed {
                path: routed(path),
                route,
                problem,
            },
            Failure::Unremoved { path, source } => Failure::Unremoved {
                path: routed(path),
                source,
            },
        }
    }
}

/// `routes` as a sentence lists them: `a`, `a and b`, `a, b and c`.
fn listed(routes: &[String]) -> String {
    match routes {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Something wrong with an artifact that was gathered anyway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// Its manifest, where it now sits on the shelf.
    pub path: PathBuf,
    pub issue: ValidationIssue,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.issue)
    }
}

/// Why a gather could not happen at all, as opposed to happening and finding
/// some artifacts it could not take: those are in the [`Report`].
#[derive(Debug, Error)]
pub enum GatherError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Lock(#[from] LockError),
    /// Its [`Watch`] was cancelled before anything was put on the shelf,
    /// which is as it was, and so is the ledger.
    #[error("cancelled, with nothing put on the shelf")]
    Cancelled,
    /// The URL carries a password or token, or a user name where a token can
    /// stand in for one, which the message names as a user name: calling it a
    /// password would send somebody looking for one that is not there. `url` is shown without it, which is the URL to
    /// gather instead. A URL typed as the branch or the folder is refused
    /// the same way, and is the one `url` names. A token the front end
    /// hands over ([`Request::token`]) is the other way to give it.
    #[error(
        "{url}: a password, a token or, over http(s), a user name in the URL is refused, because \
         gathering stores and shows the URL: gather it without one, and hand the token over with \
         --token-env or --gh, or configure a git credential helper that has it"
    )]
    CredentialsInUrl { url: String },
    /// A token was handed over for a repository reached over ssh, which
    /// logs in with keys of its own. `url` is what would be fetched.
    #[error(
        "{url}: reached over ssh, which logs in with keys of its own, so a token would be for \
         nothing: leave --token-env and --gh out, or gather its https URL"
    )]
    TokenOverSsh { url: String },
    /// A token was handed over for a repository fetched over something
    /// other than https: plain http, which would carry it across the
    /// network as it is, and over which `gix` sends no credential, or a
    /// path, which asks nobody for one.
    #[error(
        "{url}: not fetched over https, the one way a token is sent: leave --token-env and --gh \
         out, or gather its https URL"
    )]
    TokenNotHttps { url: String },
    #[error("{url}: has no `{subdirectory}` folder to gather from")]
    NoSuchFolder { subdirectory: String, url: String },
    /// Not one skill where any of [`LAYOUTS`] looks, in the folder looked
    /// in: the wrong repository, or one keeping its skills where `--path`
    /// has to name. For rules, not one markdown file in the folder named
    /// but its README and its licence (#111).
    #[error("{url}: {}", nothing_found(*kind, path))]
    NothingFound {
        url: String,
        path: String,
        /// What was looked for, added after the rest (#111).
        kind: Kind,
    },
    /// A gather of a kind that is looked for in no folder unless one is
    /// named ([`default_folder`]), which names none: a gather of rules
    /// without `--path` (#111).
    #[error(
        "{} are gathered from the folder `--path` names, every markdown file in it one: name the \
         folder that holds them, or `--path .` for the whole repository",
        .kind.plural()
    )]
    FolderUnnamed { kind: Kind },
    /// `--path`, or the folder a URL names, is not one inside the
    /// repository: it climbs out with `..`, starts at a root of its own, or
    /// is a link in the repository leading out of it.
    #[error(
        "`{path}` is not a folder inside the repository: name one by its route from the \
         repository's root"
    )]
    PathOutside { path: String },
    /// The URL names a branch (`owner/repo@ref`, a folder's address on a
    /// forge), and `--ref` names another.
    #[error("the URL names the branch `{named}`, and `--ref` names `{given}`: give it once")]
    ReferenceTwice { named: String, given: String },
    /// The URL names a folder, and `--path` names another.
    #[error("the URL names the folder `{named}`, and `--path` names `{given}`: give it once")]
    PathTwice { named: String, given: String },
    /// `--name` named what no source can be called ([`SourceName::parse`]).
    #[error(
        "`{name}` cannot name a source: it has to be letters, digits, `.`, `_` and `-`, not \
         starting with `-`, and neither `.`, `..` nor `workspace`"
    )]
    NameInvalid { name: String },
    /// Another source is called that already, at `origin`.
    #[error(
        "`{name}` is the name of the source at {origin} already: gather this one under another \
         `--name`"
    )]
    NameTaken { name: SourceName, origin: Origin },
    /// `--pin` named what is no commit in full ([`Pin::parse`]).
    #[error(
        "`{pin}` cannot pin a source: it has to be a commit in full, 40 hexadecimal digits, \
         not a commit shortened, a branch or a tag"
    )]
    PinInvalid { pin: String },
    #[error("{} cannot be gathered yet", .kind.plural())]
    NotGatherable { kind: Kind },
    #[error("{path}: cannot be read: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

codes!(GatherError {
    Self::Cancelled => "gather_cancelled",
    Self::CredentialsInUrl { .. } => "credentials_in_url",
    Self::TokenOverSsh { .. } => "gather_token_over_ssh",
    Self::TokenNotHttps { .. } => "gather_token_not_https",
    Self::NoSuchFolder { .. } => "gather_no_such_folder",
    Self::NothingFound { .. } => "gather_nothing_found",
    Self::FolderUnnamed { .. } => "gather_folder_unnamed",
    Self::PathOutside { .. } => "gather_path_outside",
    Self::ReferenceTwice { .. } => "gather_ref_twice",
    Self::PathTwice { .. } => "gather_path_twice",
    Self::NameInvalid { .. } => "source_name_invalid",
    Self::NameTaken { .. } => "source_name_taken",
    Self::PinInvalid { .. } => "source_pin_invalid",
    Self::NotGatherable { .. } => "kind_not_gatherable",
    Self::Read { .. } => "gather_read_failed",
} carried {
    Self::Git(error) => error,
    Self::Ledger(error) => error,
    Self::Lock(error) => error,
});

/// What [`GatherError::NothingFound`] says after the URL: nothing of `kind`
/// found in `path`, and where a gather of it looked there, which for
/// skills ends by saying what `--path` is for.
///
/// Matched on the kind, so a third one has to say where it was looked for.
fn nothing_found(kind: Kind, path: &str) -> String {
    let at = looked_in(path);
    match kind {
        Kind::Skill => format!(
            "no skill found {at}: looked for a `SKILL.md` there, in each folder there, and in \
             each folder of `skills`, `skills/<namespace>`, `.agents/skills`, `.claude/skills` \
             and `plugins/<plugin>/skills`; name the folder that holds them with `--path`"
        ),
        Kind::Rule => format!(
            "no rule found {at}: looked for a markdown file (`.md`) at any depth there, but its \
             README and its licence"
        ),
        // Gathered from nowhere yet ([`gatherable`]).
        Kind::Mcp => format!("no {} found {at}", kind.noun()),
    }
}

/// Where [`GatherError::NothingFound`] looked: the repository, or the
/// folder `--path` named.
fn looked_in(path: &str) -> String {
    match path {
        DEFAULT_SUBDIRECTORY => String::from("in the repository"),
        path => format!("in `{path}`"),
    }
}

/// Everything the ledger knows about, for a front end to list.
///
/// Re-exported here because the shelf and its record are one feature: a caller
/// that gathers is the caller that lists.
pub use ledger::Gathered;

#[cfg(test)]
#[path = "gather_test.rs"]
mod tests;
