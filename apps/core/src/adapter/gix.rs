//! The [`Remote`] a build fetches with: `gix`, cloning a git repository into
//! the workspace cache.
//!
//! `gix` rather than shelling out to `git`: nothing here depends on a `git`
//! being installed or on which one is first on the PATH, and core keeps doing
//! its own I/O rather than supervising a process. A clone never prompts for a
//! credential; [`credentials`] says why. Everything `gix` says is turned into
//! core's own words here, at the edge ([`GitError`], [`Cloned`]), so no `gix`
//! type reaches the port or anything behind it.

use std::borrow::Cow;
use std::fmt::Display;
use std::fs;
use std::io;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use credentials::{Shortfall, Shortfalls};
use gix::credentials::helper::Action;
use gix::credentials::protocol;
use gix::progress::{self as gix_progress, tree};
use gix::prompt;

use crate::gather::{Progress, Watch};
use crate::port::{Cloned, GitError, Remote, Token};
use crate::url::redacted;
use crate::value::{Pin, Revision};
use crate::workspace::{replace_file, WorkspaceError};

/// How often `gix`'s counts are read while it fetches: ten times a second,
/// about as often as a person can read a number that changes.
const POLL: Duration = Duration::from_millis(100);

mod credentials;

/// `gix`, as the [`Remote`] a gather fetches through.
#[derive(Debug, Clone, Copy, Default)]
pub struct Gix;

impl Remote for Gix {
    fn fetch(
        &self,
        url: &str,
        reference: Option<&str>,
        pin: Option<&Pin>,
        into: &Path,
        watch: &Watch,
        token: Option<&Token>,
    ) -> Result<Cloned, GitError> {
        let login = Login {
            offered: credentials::Offered::for_url(token, url),
            helpers: credentials::configured,
            overrides: &[],
        };
        match pin {
            None => clone(url, Wanted::Branch(reference), into, watch, &login),
            Some(pin) => pinned(url, reference, pin, into, watch, &login),
        }
    }

    fn scrub(&self, clone: &Path) -> Result<bool, GitError> {
        scrub(clone)
    }
}

/// How a clone logs in, and what it is configured with beside the
/// machine's: the token a front end handed over, offered where it is asked
/// for (`offered`); the credential helpers built for each connection from
/// the repository it configures, which give the rest (`helpers`), handed in
/// so that a test can stand in a cascade whose prompt panics and still
/// clone over a real transport; and `overrides`, none but a test's, which
/// turns protocol v2 off for a server to refuse a commit asked for by its
/// id. One value, since every clone a fetch makes, the fallback of a pin's
/// included, logs in the same way.
struct Login<'a, H> {
    offered: Option<credentials::Offered>,
    helpers: fn(gix::Repository) -> H,
    overrides: &'a [&'a str],
}

/// What a clone checks out: whatever a branch or tag is at, the remote's
/// HEAD when none is named, or one commit, by its id.
#[derive(Debug, Clone, Copy)]
enum Wanted<'a> {
    Branch(Option<&'a str>),
    Commit(&'a Pin),
}

/// Clone `url` at `pin` into `into`: the commit alone, by its id, where the
/// server hands one out so, as GitHub, GitLab and git's own do over
/// protocol v2; otherwise everything `reference` reaches, the remote's HEAD
/// when none is named, the pin checked out of it once it is found there
/// ([`reaching`]).
///
/// The commit alone first, since it is one commit's files where the other
/// way is a branch's whole history. Only a fetch the server turned down
/// is tried the other way: a credential nobody has, a cancel, or a URL or
/// a folder that will not do is the same failure whichever is fetched.
fn pinned<H>(
    url: &str,
    reference: Option<&str>,
    pin: &Pin,
    into: &Path,
    watch: &Watch,
    login: &Login<'_, H>,
) -> Result<Cloned, GitError>
where
    H: FnMut(Action, prompt::Options) -> protocol::Result + 'static,
{
    match clone(url, Wanted::Commit(pin), into, watch, login) {
        Err(GitError::Fetch { .. }) => reaching(url, reference, pin, into, watch, login),
        cloned => cloned,
    }
}

/// Clone `url` into `into`, at what is `wanted`, replacing whatever was
/// there, telling `watch` what `gix` counts as it goes, and logging in as
/// `login` says.
///
/// A fetch into the existing clone is the obvious optimisation and is
/// deliberately not done: the clone is shallow, so re-cloning costs about what
/// a fetch would, and it is one code path rather than three (clone, fetch,
/// reconcile a checkout that someone may have edited). Gathering is not
/// something anybody runs in a loop.
fn clone<H>(
    url: &str,
    wanted: Wanted<'_>,
    into: &Path,
    watch: &Watch,
    login: &Login<'_, H>,
) -> Result<Cloned, GitError>
where
    H: FnMut(Action, prompt::Options) -> protocol::Result + 'static,
{
    // Before the clone already there is removed, which a cancel should not
    // cost.
    if watch.is_cancelled() {
        return Err(GitError::Cancelled);
    }
    emptied(into)?;

    let shortfalls = Shortfalls::default();
    let mut prepare = prepared(url, into, &shortfalls, login)?
        // Depth 1: the history is not what is being gathered, the files are.
        .with_shallow(gix::remote::fetch::Shallow::DepthAtRemote(
            NonZeroU32::new(1).expect("1 is not zero"),
        ));

    match wanted {
        Wanted::Branch(None) => {}
        Wanted::Branch(Some(reference)) => {
            prepare =
                prepare
                    .with_ref_name(Some(reference))
                    .map_err(|error| GitError::Reference {
                        // Redacted as every URL this error says is: a URL typed
                        // where a branch goes is still somebody's credential.
                        reference: redacted(reference).into_owned(),
                        detail: said(error),
                    })?;
        }
        // Fetched alone, and checked out with HEAD detached at it.
        Wanted::Commit(pin) => {
            prepare =
                prepare
                    .with_revision(Some(pin.as_str()))
                    .map_err(|error| GitError::Reference {
                        reference: pin.to_string(),
                        detail: said(error),
                    })?;
        }
    }

    // The watch's own flag, so a cancel stops `gix` where it checks, between
    // the objects it receives and the files it writes, rather than once it
    // is done. A clone it stops partway is removed by `gix` itself.
    let interrupt = watch.flag();
    let (mut checkout, _) = counted(watch, |progress| {
        prepare
            .fetch_then_checkout(progress, interrupt)
            .map_err(|error| {
                if interrupt.load(Ordering::Relaxed) {
                    GitError::Cancelled
                } else {
                    fetch_failed(url, shortfalls.last(), &error)
                }
            })
    })?;
    // Said before the checkout starts rather than once its count is first
    // read, so the step is named even when it is over before that.
    watch.report(Progress::CheckingOut {
        done: 0,
        total: None,
    });
    let checked_out = counted(watch, |progress| {
        checkout
            .main_worktree(progress, interrupt)
            .map_err(|error| GitError::Checkout {
                url: redacted(url).into_owned(),
                detail: said(error),
            })
    });
    // A checkout `gix` stops partway still succeeds, with some of the files
    // written, and is then no longer `gix`'s to remove. What will not go is
    // replaced by the next clone, as every clone replaces what is there.
    if interrupt.load(Ordering::Relaxed) {
        drop(checkout);
        let _ = std::fs::remove_dir_all(into);
        return Err(GitError::Cancelled);
    }
    let (repository, _) = checked_out?;
    // Nothing to do for a URL `gather` let through, which carries no
    // credential. Done here anyway, so that a clone keeps none on disk
    // whoever asks for one, rather than because every caller remembered.
    scrub(into)?;

    Ok(Cloned {
        root: repository
            .workdir()
            .map_or_else(|| into.to_path_buf(), Path::to_path_buf),
        // A repository with no commits has no HEAD to resolve, and that is not
        // a failure: it is an empty source, which the harvest then reports as
        // having nothing in it.
        revision: repository
            .head_id()
            .ok()
            .map(|id| Revision::from(id.to_string())),
    })
}

/// Clone everything `reference` reaches from `url` into `into`, the
/// remote's HEAD when none is named, and check `pin` out of it, once it is
/// found to be among it: a pin on a server that hands out no commit by its
/// id ([`pinned`]).
///
/// Its whole history rather than a shallow clone, since that is what tells
/// the pin is a commit the branch reaches: a server that will not hand a
/// commit out by its id hands out only what its branches and tags are at,
/// and the pin is most often older. A pin none of it holds is refused
/// ([`GitError::PinUnreachable`]), and the clone made to look is removed,
/// as a fetch that fails leaves nothing.
fn reaching<H>(
    url: &str,
    reference: Option<&str>,
    pin: &Pin,
    into: &Path,
    watch: &Watch,
    login: &Login<'_, H>,
) -> Result<Cloned, GitError>
where
    H: FnMut(Action, prompt::Options) -> protocol::Result + 'static,
{
    if watch.is_cancelled() {
        return Err(GitError::Cancelled);
    }
    emptied(into)?;
    let shortfalls = Shortfalls::default();
    let mut prepare = prepared(url, into, &shortfalls, login)?;
    if let Some(reference) = reference {
        prepare = prepare
            .with_ref_name(Some(reference))
            .map_err(|error| GitError::Reference {
                reference: redacted(reference).into_owned(),
                detail: said(error),
            })?;
    }
    let interrupt = watch.flag();
    let (repository, _) = counted(watch, |progress| {
        prepare.fetch_only(progress, interrupt).map_err(|error| {
            if interrupt.load(Ordering::Relaxed) {
                GitError::Cancelled
            } else {
                fetch_failed(url, shortfalls.last(), &error)
            }
        })
    })?;

    let checked_out = checked_out_at(&repository, url, reference, pin, watch);
    if checked_out.is_err() || interrupt.load(Ordering::Relaxed) {
        drop(repository);
        let _ = std::fs::remove_dir_all(into);
        checked_out?;
        return Err(GitError::Cancelled);
    }
    scrub(into)?;
    Ok(Cloned {
        root: repository
            .workdir()
            .map_or_else(|| into.to_path_buf(), Path::to_path_buf),
        revision: Some(pin.revision()),
    })
}

/// Check `pin` out of `repository`, a clone of `url` at `reference` with
/// its history, once the commit `reference` is at is found to reach it:
/// HEAD detached at the pin, as a clone `gix` makes at a commit leaves it,
/// and its files written into the clone's empty working tree.
fn checked_out_at(
    repository: &gix::Repository,
    url: &str,
    reference: Option<&str>,
    pin: &Pin,
    watch: &Watch,
) -> Result<(), GitError> {
    let failed = |detail: &dyn Display| GitError::Checkout {
        url: redacted(url).into_owned(),
        detail: said(detail),
    };
    let unreachable = || GitError::PinUnreachable {
        url: redacted(url).into_owned(),
        pin: pin.clone(),
        reference: reference.map(|reference| redacted(reference).into_owned()),
    };
    let wanted = gix::ObjectId::from_hex(pin.as_bytes()).map_err(|error| failed(&error))?;
    // A repository with no commit reaches none.
    let Ok(tip) = repository.head_commit() else {
        return Err(unreachable());
    };
    let mut reached = false;
    for commit in repository
        .rev_walk([tip.id])
        .all()
        .map_err(|error| failed(&error))?
    {
        if commit.map_err(|error| failed(&error))?.id == wanted {
            reached = true;
            break;
        }
    }
    if !reached {
        return Err(unreachable());
    }

    repository
        .edit_reference(gix::refs::transaction::RefEdit {
            change: gix::refs::transaction::Change::Update {
                log: gix::refs::transaction::LogChange {
                    mode: gix::refs::transaction::RefLog::AndReference,
                    force_create_reflog: false,
                    message: "checkout: the commit a source is pinned to".into(),
                },
                expected: gix::refs::transaction::PreviousValue::Any,
                new: gix::refs::Target::Object(wanted),
            },
            name: "HEAD".try_into().map_err(|error| failed(&error))?,
            deref: false,
        })
        .map_err(|error| failed(&error))?;
    let tree = repository
        .find_commit(wanted)
        .map_err(|error| failed(&error))?
        .tree_id()
        .map_err(|error| failed(&error))?;
    let mut index = repository
        .index_from_tree(&tree)
        .map_err(|error| failed(&error))?;
    let mut options = repository
        .checkout_options(gix::worktree::stack::state::attributes::Source::IdMapping)
        .map_err(|error| failed(&error))?;
    options.destination_is_initially_empty = true;
    let objects = repository
        .objects
        .clone()
        .into_arc()
        .map_err(|error| failed(&error))?;
    let workdir = repository
        .workdir()
        .ok_or_else(|| failed(&"the clone has no working tree"))?
        .to_path_buf();

    watch.report(Progress::CheckingOut {
        done: 0,
        total: None,
    });
    let interrupt = watch.flag();
    counted(watch, |mut progress| {
        let files = progress.add_child_with_id(
            "checkout".to_owned(),
            gix::clone::checkout::main_worktree::ProgressId::CheckoutFiles.into(),
        );
        let bytes = progress.add_child_with_id(
            "writing".to_owned(),
            gix::clone::checkout::main_worktree::ProgressId::BytesWritten.into(),
        );
        files.init(Some(index.entries().len()), gix::progress::count("files"));
        bytes.init(None, gix::progress::bytes());
        gix::worktree::state::checkout(
            &mut index, &workdir, objects, &files, &bytes, interrupt, options,
        )
        .map_err(|error| failed(&error))
    })?;
    index
        .write(Default::default())
        .map_err(|error| failed(&error))
}

/// Remove whatever is at `into`, and make the folders it goes in, for a
/// clone to be made there.
fn emptied(into: &Path) -> Result<(), GitError> {
    if into.exists() {
        std::fs::remove_dir_all(into).map_err(|source| GitError::Cache {
            path: into.to_path_buf(),
            source,
        })?;
    }
    if let Some(parent) = into.parent() {
        std::fs::create_dir_all(parent).map_err(|source| GitError::Cache {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    Ok(())
}

/// A clone of `url` into `into` made ready to fetch, never prompting for a
/// credential, logging in as `login` says, what is missing noted in
/// `shortfalls`, and given `login`'s overrides beside the machine's
/// configuration.
fn prepared<H>(
    url: &str,
    into: &Path,
    shortfalls: &Shortfalls,
    login: &Login<'_, H>,
) -> Result<gix::clone::PrepareFetch, GitError>
where
    H: FnMut(Action, prompt::Options) -> protocol::Result + 'static,
{
    Ok(gix::prepare_clone(url, into)
        .map_err(|error| GitError::Url {
            url: redacted(url).into_owned(),
            detail: said(error),
        })?
        .with_in_memory_config_overrides(login.overrides.iter().copied())
        // Every connection `gix` lets the clone configure, not only the
        // first: a shallow clone connects once to find the branch and again
        // to fetch it, and either one may be asked for a credential. The one
        // it does not is a retry made when the remote has a branch named
        // `HEAD`, which asks the way `gix` does by default, prompt included;
        // docs/architecture.md keeps it among the open questions.
        .configure_connection({
            let shortfalls = shortfalls.clone();
            let offered = login.offered.clone();
            let helpers = login.helpers;
            move |connection| {
                let repository = connection.remote().repo().clone();
                connection.set_credentials(credentials::never_prompting(
                    credentials::offering(offered.clone(), helpers(repository)),
                    shortfalls.clone(),
                ));
                Ok(())
            }
        }))
}

/// Run `work` with a progress tree of its own, telling `watch` what `gix`
/// counts in it until `work` returns.
///
/// The tree is read from a thread of its own, as it is made to be, rather
/// than through a progress type that reports each time it is told something:
/// `gix` counts most of its work on shared atomics, from threads of its own,
/// without telling the progress anything. That thread is joined before this
/// returns, so nothing it reports can arrive after what the gather says next.
fn counted<T>(watch: &Watch, work: impl FnOnce(tree::Item) -> T) -> T {
    let root = tree::Root::new();
    let progress = root.add_child("gather");
    let (finished, done) = mpsc::channel::<()>();
    thread::scope(|scope| {
        let root = &root;
        scope.spawn(move || {
            let mut tasks = Vec::new();
            let mut said = None;
            // Until `finished` goes, which `recv_timeout` sees at once rather
            // than at the end of the wait.
            while let Err(RecvTimeoutError::Timeout) = done.recv_timeout(POLL) {
                root.sorted_snapshot(&mut tasks);
                // Only what changed, so a count that stands still is said
                // once rather than ten times a second.
                if let Some(now) = reading(&tasks).filter(|now| said != Some(*now)) {
                    watch.report(now);
                    said = Some(now);
                }
            }
        });
        let result = work(progress);
        drop(finished);
        result
    })
}

/// The step a snapshot of `gix`'s progress tree shows, in core's words: the
/// latest of receiving the pack, resolving it, and checking out its files
/// that is under way, with its count.
///
/// Picked out by the identifiers `gix` gives those counts rather than by the
/// names it shows, which it is free to reword. What else the tree holds
/// (bytes read, what the server says it is doing) is left out: the object
/// and file counts are the ones with a total, and so the ones that say how
/// far there is to go.
fn reading(
    tasks: &[(gix_progress::prodash::progress::Key, gix_progress::Task)],
) -> Option<Progress> {
    let count = |id: gix_progress::Id| {
        tasks
            .iter()
            .filter(|(_, task)| task.id == id)
            .find_map(|(_, task)| task.progress.as_ref())
            .map(|value| (value.step.load(Ordering::Relaxed), value.done_at))
    };
    let checkout: gix_progress::Id =
        gix::clone::checkout::main_worktree::ProgressId::CheckoutFiles.into();
    let resolve: gix_progress::Id = gix::odb::pack::index::write::ProgressId::ResolveObjects.into();
    let receive: gix_progress::Id = gix::odb::pack::index::write::ProgressId::IndexObjects.into();
    if let Some((done, total)) = count(checkout) {
        Some(Progress::CheckingOut { done, total })
    } else if let Some((done, total)) = count(resolve) {
        Some(Progress::Resolving { done, total })
    } else {
        count(receive).map(|(done, total)| Progress::Receiving { done, total })
    }
}

/// Rewrite what a clone keeps of the URL it was made from without the
/// credentials in it, returning whether there were any.
///
/// `gix` writes that URL as typed into the remote's entry in `.git/config`,
/// and again into every reflog, as `clone: from <url>`; `FETCH_HEAD` would
/// hold it too. Those files are text and the URL is in them whole, so they
/// are rewritten through [`redacted`] rather than through `gix`'s config
/// editor, which would reach the config and not the logs. Deleting the clone
/// was the other way to be rid of them, and is rejected: the clone is kept so
/// a gather can be looked at afterwards, and a scrub has no reason to take
/// that away. What is missing is an absence, not a failure, and a symlink is
/// not followed out of the clone.
fn scrub(clone: &Path) -> Result<bool, GitError> {
    let git = clone.join(".git");
    let mut files = vec![git.join("config"), git.join("FETCH_HEAD")];
    files_under(&git.join("logs"), &mut files)?;

    let mut scrubbed = false;
    for file in files {
        if !fs::symlink_metadata(&file).is_ok_and(|metadata| metadata.is_file()) {
            continue;
        }
        let bytes = fs::read(&file).map_err(|source| WorkspaceError::Read {
            path: file.clone(),
            source,
        })?;
        // What `gix` writes there is UTF-8, URLs included. A file that is
        // not was written by something else, and is left as it was.
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        if let Cow::Owned(clean) = redacted(&text) {
            replace_file(&file, &clean)?;
            scrubbed = true;
        }
    }
    Ok(scrubbed)
}

/// Every plain file under `directory`, at any depth, without following a
/// symlink.
fn files_under(directory: &Path, found: &mut Vec<PathBuf>) -> Result<(), GitError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(WorkspaceError::Read {
                path: directory.to_path_buf(),
                source,
            }
            .into())
        }
    };
    for entry in entries {
        let entry = entry.map_err(|source| WorkspaceError::Read {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => files_under(&path, found)?,
            Ok(kind) if kind.is_file() => found.push(path),
            _ => {}
        }
    }
    Ok(())
}

/// What a failed fetch is reported as: the credential it lacked, when that is
/// what stopped it, and otherwise what `gix` said.
fn fetch_failed(url: &str, shortfall: Option<Shortfall>, detail: &dyn Display) -> GitError {
    let url = redacted(url).into_owned();
    match shortfall {
        Some(Shortfall::Missing) => GitError::Credentials { url },
        // Whether a refusal over ssh is a missing key is the callback's to
        // say, not `url`'s: a rewrite in git config may have sent the fetch
        // over another transport than the one `url` names.
        Some(Shortfall::Refused) | None => GitError::Fetch {
            url,
            detail: said(detail),
        },
    }
}

/// What `gix` said, without the credentials of any URL it repeats.
///
/// Redacting the URL that was typed is not enough: `gix` names the URL it
/// connected to, which `url.<base>.insteadOf` in git config may have given a
/// token the typed one never had.
fn said(error: impl Display) -> String {
    redacted(&error.to_string()).into_owned()
}

#[cfg(test)]
#[path = "gix_test.rs"]
mod tests;
