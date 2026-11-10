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

use std::collections::HashMap;
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
use crate::install::{self, LeftOut};
use crate::kind::{Kind, Layout};
use crate::ledger::{self, Action, Ledger, LedgerError, Merged, Outcome, SourceKind};
use crate::lock::LockError;
use crate::paths;
use crate::port::{Cloned, GitError, Remote, Token};
use crate::url::{carries_credentials, located, over_ssh, redacted};
use crate::value::{ArtifactName, Origin, Redacted, Revision, SourceName};
use crate::workspace::WeaverWorkspace;

pub use watch::{Progress, Watch};

/// The folder inside a source that is harvested unless another is named:
/// its root, where every one of [`LAYOUTS`] is looked for.
///
/// `.`, the route the root is, rather than an empty one, so a source
/// shows a folder wherever its folder is shown, and `--path .` is the
/// same source as no `--path`.
pub const DEFAULT_SUBDIRECTORY: &str = ".";

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
    /// The folder inside the source to harvest, its root unless told
    /// another ([`DEFAULT_SUBDIRECTORY`]); a URL that names one says it
    /// instead ([`crate::url::located`]).
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
            subdirectory: DEFAULT_SUBDIRECTORY.to_owned(),
            kind: Kind::Skill,
            watch: Watch::new(),
            remote: Arc::new(Gix),
            name: None,
            token: None,
        }
    }

    /// Take only what is in `subdirectory`, a route from the source's root,
    /// rather than from the whole of it.
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

    /// Log in with `token`, when one is given, rather than as the
    /// credential helpers and ssh do.
    pub fn with_token(mut self, token: Option<Token>) -> Self {
        self.token = token;
        self
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
    /// folder of its own there, the name it declares.
    pub directory: String,
    /// The one line a listing shows.
    pub summary: Option<String>,
    /// Where it now sits in the workspace.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// Where it is in the source: its folder's route from the source's
    /// root, `/` separated, `.` for the root itself. Added after the rest
    /// (#101), since a source keeps its skills in more than one place.
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
    let (typed, manifest) = typed(request)?;
    if request.watch.is_cancelled() {
        return Err(GatherError::Cancelled);
    }
    let known = known(ledger, request.source.kind(), &typed)?;
    if let Some(name) = &typed.name {
        taken(ledger, name, known.as_ref().map(|(source, _)| source))?;
    }
    let (name, directory) = match known {
        Some((source, _)) => (source.name, source.directory),
        None => ledger.would_register(&typed.url)?,
    };
    let name = typed.name.clone().unwrap_or(name);
    let scratch = workspace
        .scratch_dir()
        .join(format!("gather-{}", copy::unique_name()));
    let outcome = foresee(
        workspace, request, &typed, manifest, &scratch, name, directory,
    );
    // Whatever came of it: nothing of a dry run is kept. One that will not
    // go is left in a folder no agent reads, under a name no other run is
    // given.
    let _ = fs::remove_dir_all(&scratch);
    outcome
}

/// A dry run once its source is known: fetched into `scratch`, read, and
/// each skill copied there and compared with the shelf's copy of it.
fn foresee(
    workspace: &WeaverWorkspace,
    request: &Request,
    typed: &Typed,
    manifest: &str,
    scratch: &Path,
    name: SourceName,
    directory: String,
) -> Result<Report, GatherError> {
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
    };
    let mut failures = Vec::new();
    let examined = examine(&clone, typed, manifest, &request.watch, &mut failures);
    report.failures = failures
        .into_iter()
        .map(|failure| failure.routed(&clone.root))
        .collect();
    let leave_out = left_out_of_shelf();
    for found in examined? {
        let destination = shelf.join(&found.folder);
        let copy = scratch.join("shelf").join(&found.folder);
        let change = copy::replace_leaving(&found.directory, &copy, &clone.root, &leave_out)
            .and_then(|_| copy::compared(&copy, &destination));
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
        noted(&mut report, &found, &destination, manifest);
        let harvested = found.harvested(destination);
        match change {
            Change::Added => report.added.push(harvested),
            Change::Updated => report.updated.push(harvested),
            Change::Unchanged => report.unchanged.push(harvested),
        }
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
/// to its rule.
#[derive(Debug, Clone)]
struct Typed {
    /// What is fetched ([`located`]).
    url: String,
    reference: Option<String>,
    path: String,
    name: Option<SourceName>,
}

/// Read what `request` was typed with, refusing what no gather takes before
/// anything is registered or fetched, beside the manifest a folder of its
/// kind holds.
fn typed(request: &Request) -> Result<(Typed, &'static str), GatherError> {
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
        Some(request.subdirectory.as_str()),
        request.name.as_deref(),
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
    let given = route_of(&request.subdirectory).ok_or_else(|| GatherError::PathOutside {
        path: request.subdirectory.clone(),
    })?;
    let path = match at.path {
        Some(named) => {
            let route = route_of(&named).ok_or_else(|| GatherError::PathOutside {
                path: named.clone(),
            })?;
            if given != DEFAULT_SUBDIRECTORY && given != route {
                return Err(GatherError::PathTwice {
                    named,
                    given: request.subdirectory.clone(),
                });
            }
            route
        }
        None => given,
    };

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
    Ok((
        Typed {
            url: at.url,
            reference,
            path,
            name,
        },
        manifest,
    ))
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
fn route_of(typed: &str) -> Option<String> {
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
/// origin that holds no folder, which two sources would share.
fn known(
    ledger: &Ledger,
    kind: SourceKind,
    typed: &Typed,
) -> Result<Option<(ledger::Source, bool)>, LedgerError> {
    let reference = typed.reference.as_deref();
    if let Some(found) = ledger.source(kind, &typed.url, reference, &typed.path)? {
        return Ok(Some((found, false)));
    }
    if typed.path != DEFAULT_SUBDIRECTORY {
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
    let (typed, manifest) = typed(request)?;

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

    let outcome = take(workspace, ledger, request, &typed, &source, manifest);
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
    manifest: &str,
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
    };

    // Everything is read before anything is placed, rather than each
    // artifact read and placed in turn, so that the shelf only starts to
    // change once all of it is known.
    let found = examine(&clone, typed, manifest, watch, &mut report.failures)?;

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

    let total = found.len();
    watch.report(Progress::Placing { done: 0, total });
    let leave_out = left_out_of_shelf();
    for (done, one) in found.into_iter().enumerate() {
        let destination = shelf.join(&one.folder);
        let placed = copy::replace_leaving(&one.directory, &destination, &clone.root, &leave_out);
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
        noted(&mut report, &one, &destination, manifest);

        let recorded = relative(workspace, &destination);
        ledger.record(
            source,
            request.kind,
            one.artifact.name(),
            &recorded,
            one.artifact.summary(),
            clone.revision.as_deref(),
        )?;

        let harvested = one.harvested(destination);
        match change {
            Change::Added => report.added.push(harvested),
            Change::Updated => report.updated.push(harvested),
            Change::Unchanged => report.unchanged.push(harvested),
        }
    }

    Ok(ordered(report))
}

/// Say in `report` what is wrong with `found`, put at `destination`, and
/// what it was put there without.
///
/// Lenient, as the Agent Skills client guide advises for skills written
/// for other agents: what is wrong with one is said, and it is taken
/// anyway. Installing it is where it is held to the rules. Judged as it is
/// filed, under the folder the shelf keeps it in, which for a skill at the
/// source's root is not the folder it was read from.
fn noted(report: &mut Report, found: &Found, destination: &Path, manifest: &str) {
    report.warnings.extend(
        found
            .artifact
            .validate_filed_as(&found.folder)
            .into_iter()
            .map(|issue| Warning {
                path: destination.join(manifest),
                issue,
            }),
    );
    report.left_out.extend(
        install::left_out(&found.directory)
            .into_iter()
            .map(|left| Unshelved {
                name: found.artifact.name().to_owned(),
                left,
            }),
    );
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

/// A skill found in a source: its folder in the clone, the folder the shelf
/// files it under, its route from the source's root, and what it declares.
struct Found {
    directory: PathBuf,
    folder: String,
    route: String,
    artifact: Artifact,
}

impl Found {
    /// What the report says of it, once it is at `path`.
    fn harvested(self, path: PathBuf) -> Harvested {
        Harvested {
            name: self.artifact.name().to_owned(),
            directory: self.folder,
            summary: self.artifact.summary().map(str::to_owned),
            path,
            route: self.route,
        }
    }
}

/// What `clone` offers in the folder `typed` names, each skill read, with
/// what could not be among `failures`: everything a gather knows before the
/// shelf changes.
///
/// The folder is held to being inside the clone once links are followed,
/// since `--path` names one of the repository's, and found to hold a skill
/// at all: a source that offers nothing where every layout looks is a
/// gather that names the wrong thing, said rather than gathered as nothing,
/// which would leave the shelf holding a revision's skills under a revision
/// that has none.
fn examine(
    clone: &Cloned,
    typed: &Typed,
    manifest: &str,
    watch: &Watch,
    failures: &mut Vec<Failure>,
) -> Result<Vec<Found>, GatherError> {
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

    let candidates = candidates(&root, manifest)?;
    if candidates.is_empty() {
        return Err(GatherError::NothingFound {
            url: typed.url.clone(),
            path: typed.path.clone(),
        });
    }
    let total = candidates.len();
    watch.report(Progress::Harvesting { done: 0, total });
    let mut found = Vec::with_capacity(total);
    for (done, directory) in candidates.iter().enumerate() {
        if let Some((directory, folder, artifact)) = read(directory, &clone.root, failures) {
            let route = paths::relative_to(&directory, &clone.root)
                .as_deref()
                .and_then(paths::to_config_string)
                .filter(|route| !route.is_empty())
                .unwrap_or_else(|| DEFAULT_SUBDIRECTORY.to_owned());
            found.push(Found {
                directory,
                folder,
                route,
                artifact,
            });
        }
        watch.report(Progress::Harvesting {
            done: done + 1,
            total,
        });
    }
    Ok(unclashed(found, failures))
}

/// `found` without every skill another in the same source is called as
/// too, by the folder the shelf would file it under or by the name it
/// declares, each of those a failure naming the others (#101).
///
/// Neither is taken: which of them a name means is not a gather's to pick,
/// and taking the first would make which skill a project gets depend on
/// the order a folder is listed in. By the folder because the shelf keeps
/// one folder of a name a source, and by the name because the ledger
/// records one skill of a name a source, which a second would replace. In
/// any case, since a disk that does not tell `Deploy` from `deploy` would
/// hold one folder of both, and a source gathers the same on every disk.
fn unclashed(found: Vec<Found>, failures: &mut Vec<Failure>) -> Vec<Found> {
    let folder = |one: &Found| one.folder.to_ascii_lowercase();
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
        let name = if shared {
            one.folder.clone()
        } else {
            one.artifact.name().to_string()
        };
        failures.push(Failure::Clash {
            path: one.directory,
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
    /// Its manifest is a link that leads out of the source.
    #[error("{path}: is a link leading out of the source, so it is not read")]
    Outside { path: PathBuf },
    /// A skill at the source's root, which is filed under the name it
    /// declares, declares one that is no folder's.
    #[error(
        "{path}: a skill at the root of a source is filed under the name it declares, and \
         `{name}` is not one folder name"
    )]
    Unfiled { path: PathBuf, name: String },
}

codes!(Failure {
    Self::UnusableName { .. } => "gathered_name_not_utf8",
    Self::Copy { .. } => "gathered_copy_failed",
    Self::Clash { .. } => "gathered_name_clash",
    Self::Outside { .. } => "gathered_link_outside",
    Self::Unfiled { .. } => "gathered_root_unnamed",
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
            | Failure::Unfiled { path, .. } => path,
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
    /// has to name.
    #[error(
        "{url}: no skill found {}: looked for a `SKILL.md` there, in each folder there, and in \
         each folder of `skills`, `skills/<namespace>`, `.agents/skills`, `.claude/skills` and \
         `plugins/<plugin>/skills`; name the folder that holds them with `--path`",
        looked_in(path)
    )]
    NothingFound { url: String, path: String },
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
    Self::PathOutside { .. } => "gather_path_outside",
    Self::ReferenceTwice { .. } => "gather_ref_twice",
    Self::PathTwice { .. } => "gather_path_twice",
    Self::NameInvalid { .. } => "source_name_invalid",
    Self::NameTaken { .. } => "source_name_taken",
    Self::NotGatherable { .. } => "kind_not_gatherable",
    Self::Read { .. } => "gather_read_failed",
} carried {
    Self::Git(error) => error,
    Self::Ledger(error) => error,
    Self::Lock(error) => error,
});

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
