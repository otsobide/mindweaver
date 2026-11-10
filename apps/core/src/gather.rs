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

mod watch;

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::ser::{SerializeStruct, Serializer};
use serde::Serialize;
use thiserror::Error;

use crate::adapter::gix::Gix;
use crate::artifact::{Artifact, ArtifactError, ValidationIssue};
use crate::code::codes;
use crate::copy::{self, Change};
use crate::kind::{Kind, Layout};
use crate::ledger::{self, Action, Ledger, LedgerError, Merged, Outcome, SourceKind};
use crate::lock::LockError;
use crate::paths;
use crate::port::{GitError, Remote};
use crate::url::{carries_credentials, fetched, redacted};
use crate::value::{ArtifactName, Origin, Redacted, Revision, SourceName};
use crate::workspace::WeaverWorkspace;

pub use watch::{Progress, Watch};

/// The folder inside a source that is harvested unless another is named.
pub const DEFAULT_SUBDIRECTORY: &str = "skills";

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
    /// The folder inside the source to harvest.
    pub subdirectory: String,
    /// What to look for there.
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
}

impl Request {
    /// Gather skills from a git repository's `skills` folder.
    pub fn git(url: impl Into<String>) -> Self {
        Self {
            source: Source::Git {
                url: url.into(),
                reference: None,
            },
            subdirectory: DEFAULT_SUBDIRECTORY.to_owned(),
            kind: Kind::Skill,
            watch: Watch::new(),
            remote: Arc::new(Gix),
            name: None,
        }
    }

    /// Take from `subdirectory` instead of `skills`.
    pub fn from_subdirectory(mut self, subdirectory: impl Into<String>) -> Self {
        self.subdirectory = subdirectory.into();
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
}

/// One artifact that reached the workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Harvested {
    /// The name it declares, which is what it will be asked for by.
    pub name: ArtifactName,
    /// The folder it came from inside the source, which is the folder it kept.
    pub directory: String,
    /// The one line a listing shows.
    pub summary: Option<String>,
    /// Where it now sits in the workspace.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
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
    // What was fetched, `owner/repo` read as the GitHub repository it names.
    let url = fetched(request.source.url());

    // The log records what was attempted, not only what worked: a gather that
    // failed is the entry somebody opens the log to find. A log write that
    // fails on the error path is swallowed, because the error it would replace
    // is the more useful of the two.
    match &outcome {
        Ok(report) => {
            let detail = format!(
                "{} added, {} updated, {} unchanged, {} failed",
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

/// The work itself, with the logging left to [`gather`].
fn harvest(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    request: &Request,
) -> Result<Report, GatherError> {
    // Refused before anything is registered or fetched, rather than used for
    // this one fetch and dropped from what is stored: a token on the command
    // line is already where `ps` and the shell history show it, and every
    // report names the URL. A git credential helper, or the front end once
    // #110 lands, keeps it off the command line altogether.
    //
    // So is a URL typed where the branch, the folder or the name goes,
    // which the ledger keeps as that branch or folder, word for word, and an
    // error repeats: it is somebody's credential all the same.
    let typed = [
        Some(request.source.url()),
        request.source.reference(),
        Some(request.subdirectory.as_str()),
        request.name.as_deref(),
    ];
    if let Some(carrying) = typed
        .into_iter()
        .flatten()
        .find(|text| carries_credentials(text))
    {
        return Err(GatherError::CredentialsInUrl {
            url: redacted(carrying).into_owned(),
        });
    }
    let name = match &request.name {
        Some(typed) => Some(
            SourceName::parse(typed).ok_or_else(|| GatherError::NameInvalid {
                name: typed.clone(),
            })?,
        ),
        None => None,
    };

    // Only directory-shaped kinds are gatherable today. Matching on the kind
    // rather than on its layout keeps the compiler asking: a third kind has to
    // answer this question before it can be added.
    match request.kind {
        Kind::Skill => {}
        Kind::Rule | Kind::Mcp => return Err(GatherError::NotGatherable { kind: request.kind }),
    }
    let Layout::Directory { manifest } = request.kind.layout() else {
        return Err(GatherError::NotGatherable { kind: request.kind });
    };

    // Before the source is registered, so a gather cancelled before it began
    // has nothing to undo.
    if request.watch.is_cancelled() {
        return Err(GatherError::Cancelled);
    }

    // Found by the URL every spelling of it is ([`crate::url::canonical`]),
    // so a second spelling gathers into the source the first registered,
    // and registered as fetched.
    let url = fetched(request.source.url());
    let found = ledger.source(
        request.source.kind(),
        &url,
        request.source.reference(),
        &request.subdirectory,
    )?;
    // Refused before anything is registered: one name is one source, and
    // which of two a `--from` named is not something to guess.
    if let Some(name) = &name {
        if let Some(holder) = ledger.source_named(name)? {
            if found.as_ref().is_none_or(|found| found.id != holder.id) {
                return Err(GatherError::NameTaken {
                    name: name.clone(),
                    origin: holder.origin(),
                });
            }
        }
    }
    let new = found.is_none();
    let mut source = ledger.source_for(
        request.source.kind(),
        &url,
        request.source.reference(),
        &request.subdirectory,
    )?;
    let was = source.name.clone();
    if let Some(name) = name.filter(|name| *name != source.name) {
        ledger.rename_source(source.id, &name)?;
        source.name = name;
    }

    let outcome = take(workspace, ledger, request, &url, &source, manifest);
    if !new && source.name != was && matches!(outcome, Err(GatherError::Cancelled)) {
        // Called what it was again, as a cancel leaves the ledger.
        ledger.rename_source(source.id, &was)?;
    }
    if new && matches!(outcome, Err(GatherError::Cancelled)) {
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

/// Fetch `source` from `url`, as it was typed this time, read what it
/// offers, and put that on the shelf: the gather, once its source is
/// registered.
///
/// From the URL typed rather than the one the source was first registered
/// with, though both are one repository's: the second spelling is the one
/// somebody asked for now, an ssh one where https needs a credential the
/// helpers lack, or the reverse.
fn take(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    request: &Request,
    url: &str,
    source: &ledger::Source,
    manifest: &str,
) -> Result<Report, GatherError> {
    let watch = &request.watch;
    let cache = workspace.cache_dir().join(&source.directory);
    watch.report(Progress::Connecting);
    let clone = match &request.source {
        Source::Git { reference, .. } => request
            .remote
            .fetch(url, reference.as_deref(), &cache, watch)
            .map_err(|error| match error {
                GitError::Cancelled => GatherError::Cancelled,
                error => error.into(),
            })?,
    };

    let harvested_from = clone.root.join(&request.subdirectory);
    if !harvested_from.is_dir() {
        return Err(GatherError::NoSuchFolder {
            subdirectory: request.subdirectory.clone(),
            url: url.to_owned(),
        });
    }

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
    };

    // Everything is read before anything is placed, rather than each
    // artifact read and placed in turn, so that the shelf only starts to
    // change once all of it is known.
    let found = candidates(&harvested_from, manifest)?;
    let mut taken = Vec::with_capacity(found.len());
    watch.report(Progress::Harvesting {
        done: 0,
        total: found.len(),
    });
    for (done, directory) in found.iter().enumerate() {
        if let Some(read) = read(directory, &clone.root, &mut report.failures) {
            taken.push(read);
        }
        watch.report(Progress::Harvesting {
            done: done + 1,
            total: found.len(),
        });
    }

    // The last moment a cancel is honoured, and the reason everything is read
    // first: past here the shelf and the ledger change, and a gather stopped
    // halfway through that would leave them neither as they were nor as the
    // source has them. `Progress::cancellable` says so of every step after.
    if watch.is_cancelled() {
        return Err(GatherError::Cancelled);
    }
    // Noted only now, and not by a gather that fails before it gets here,
    // since the revision a source names is the one its shelf holds.
    ledger.saw_source(source.id, clone.revision.as_deref())?;

    watch.report(Progress::Placing {
        done: 0,
        total: taken.len(),
    });
    for (done, (directory, folder, artifact)) in taken.iter().enumerate() {
        let destination = shelf.join(folder);
        let placed = copy::replace(directory, &destination, &clone.root);
        watch.report(Progress::Placing {
            done: done + 1,
            total: taken.len(),
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

        // Lenient, as the Agent Skills client guide advises for skills
        // written for other agents: what is wrong with one is said, and it is
        // taken anyway. Installing it is where it is held to the rules.
        report
            .warnings
            .extend(artifact.validate().into_iter().map(|issue| Warning {
                path: destination.join(manifest),
                issue,
            }));

        let recorded = relative(workspace, &destination);
        ledger.record(
            source,
            request.kind,
            artifact.name(),
            &recorded,
            artifact.summary(),
            clone.revision.as_deref(),
        )?;

        let harvested = Harvested {
            name: artifact.name().to_owned(),
            directory: folder.clone(),
            summary: artifact.summary().map(str::to_owned),
            path: destination,
        };
        match change {
            Change::Added => report.added.push(harvested),
            Change::Updated => report.updated.push(harvested),
            Change::Unchanged => report.unchanged.push(harvested),
        }
    }

    report.failures.sort_by(|a, b| a.path().cmp(b.path()));
    // Stable, so one file's warnings stay in the order `validate` gives them.
    report.warnings.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(report)
}

/// One candidate folder, loaded to learn its name and its summary: the
/// folder, the name it is filed under, and the artifact. `None`, with the
/// reason among `failures`, when it cannot be taken.
///
/// `project` is the clone it was found in, which is where it was found and
/// not a claim that a clone is a mind project.
fn read(
    directory: &Path,
    clone: &Path,
    failures: &mut Vec<Failure>,
) -> Option<(PathBuf, String, Artifact)> {
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
    // it.
    let Some(folder) = directory.file_name().and_then(|name| name.to_str()) else {
        failures.push(Failure::UnusableName {
            path: directory.to_path_buf(),
        });
        return None;
    };
    Some((directory.to_path_buf(), folder.to_owned(), artifact))
}

/// The directories under `root` that hold a `manifest`.
///
/// Only the immediate ones, for the reason discovery has: the directory
/// belongs to the artifact, assets and all.
fn candidates(root: &Path, manifest: &str) -> Result<Vec<PathBuf>, GatherError> {
    let entries = fs::read_dir(root).map_err(|source| GatherError::Read {
        path: root.to_path_buf(),
        source,
    })?;

    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| GatherError::Read {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.join(manifest).is_file() {
            found.push(path);
        }
    }
    found.sort();
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
}

codes!(Failure {
    Self::UnusableName { .. } => "gathered_name_not_utf8",
    Self::Copy { .. } => "gathered_copy_failed",
} carried {
    Self::Artifact(error) => error,
});

impl Failure {
    /// The path the failure is about.
    pub fn path(&self) -> &Path {
        match self {
            Failure::Artifact(error) => error.path(),
            Failure::UnusableName { path } | Failure::Copy { path, .. } => path,
        }
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
    /// the same way, and is the one `url` names. A provider the front end supplies (#110) will be a
    /// second way to give the credential.
    #[error(
        "{url}: a password, a token or, over http(s), a user name in the URL is refused, because \
         gathering stores and shows the URL: gather it without one, and configure a git \
         credential helper that has it"
    )]
    CredentialsInUrl { url: String },
    #[error("{url}: has no `{subdirectory}` folder to gather from")]
    NoSuchFolder { subdirectory: String, url: String },
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
    Self::NoSuchFolder { .. } => "gather_no_such_folder",
    Self::NameInvalid { .. } => "source_name_invalid",
    Self::NameTaken { .. } => "source_name_taken",
    Self::NotGatherable { .. } => "kind_not_gatherable",
    Self::Read { .. } => "gather_read_failed",
} carried {
    Self::Git(error) => error,
    Self::Ledger(error) => error,
    Self::Lock(error) => error,
});

/// Everything the ledger knows about, for a front end to list.
///
/// Re-exported here because the shelf and its record are one feature: a caller
/// that gathers is the caller that lists.
pub use ledger::Gathered;

#[cfg(test)]
#[path = "gather_test.rs"]
mod tests;
