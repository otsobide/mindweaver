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
use crate::value::Revision;
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
        into: &Path,
        watch: &Watch,
        token: Option<&Token>,
    ) -> Result<Cloned, GitError> {
        clone(
            url,
            reference,
            into,
            watch,
            credentials::Offered::for_url(token, url),
            credentials::configured,
        )
    }

    fn scrub(&self, clone: &Path) -> Result<bool, GitError> {
        scrub(clone)
    }
}

/// Clone `url` into `into`, replacing whatever was there, telling `watch`
/// what `gix` counts as it goes, logging in with `offered` where it is
/// asked for and with the credential `helpers` (built for each connection
/// from the repository it configures) give elsewhere.
///
/// `helpers` is handed in rather than named here so that a test can stand
/// in a cascade whose prompt panics, and still clone over a real transport.
///
/// A fetch into the existing clone is the obvious optimisation and is
/// deliberately not done: the clone is shallow, so re-cloning costs about what
/// a fetch would, and it is one code path rather than three (clone, fetch,
/// reconcile a checkout that someone may have edited). Gathering is not
/// something anybody runs in a loop.
fn clone<H>(
    url: &str,
    reference: Option<&str>,
    into: &Path,
    watch: &Watch,
    offered: Option<credentials::Offered>,
    helpers: fn(gix::Repository) -> H,
) -> Result<Cloned, GitError>
where
    H: FnMut(Action, prompt::Options) -> protocol::Result + 'static,
{
    // Before the clone already there is removed, which a cancel should not
    // cost.
    if watch.is_cancelled() {
        return Err(GitError::Cancelled);
    }
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

    let shortfalls = Shortfalls::default();
    let mut prepare = gix::prepare_clone(url, into)
        .map_err(|error| GitError::Url {
            url: redacted(url).into_owned(),
            detail: said(error),
        })?
        // Depth 1: the history is not what is being gathered, the files are.
        .with_shallow(gix::remote::fetch::Shallow::DepthAtRemote(
            NonZeroU32::new(1).expect("1 is not zero"),
        ))
        // Every connection `gix` lets the clone configure, not only the
        // first: a shallow clone connects once to find the branch and again
        // to fetch it, and either one may be asked for a credential. The one
        // it does not is a retry made when the remote has a branch named
        // `HEAD`, which asks the way `gix` does by default, prompt included;
        // docs/architecture.md keeps it among the open questions.
        .configure_connection({
            let shortfalls = shortfalls.clone();
            move |connection| {
                let repository = connection.remote().repo().clone();
                connection.set_credentials(credentials::never_prompting(
                    credentials::offering(offered.clone(), helpers(repository)),
                    shortfalls.clone(),
                ));
                Ok(())
            }
        });

    if let Some(reference) = reference {
        prepare = prepare
            .with_ref_name(Some(reference))
            .map_err(|error| GitError::Reference {
                // Redacted as every URL this error says is: a URL typed
                // where a branch goes is still somebody's credential.
                reference: redacted(reference).into_owned(),
                detail: said(error),
            })?;
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
