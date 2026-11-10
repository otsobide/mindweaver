//! The `Remote` contract: what every adapter a gather fetches through
//! promises, run once against `Gix` over repositories on the disk and once
//! against a forge kept in memory, so the promise is the port's rather than
//! one library's. Then gathers through each, and through remotes that fail,
//! cancel or hand back less than a clone, to show what a gather takes from
//! whichever remote it is handed.
//!
//! Nothing reaches the network: a repository is a folder `gix` clones by its
//! path, and the fake writes what it holds.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mindweaver_core::adapter::gix::Gix;
use mindweaver_core::gather::{self, GatherError, Progress, Request, Watch};
use mindweaver_core::ledger::{Action, Outcome};
use mindweaver_core::mother::{
    random, GitRepositoryMother, RevisionMother, SkillMother, UrlMother, WorkspaceMother,
};
use mindweaver_core::port::{Cloned, GitError, Remote, Token};
use mindweaver_core::url::redacted;
use mindweaver_core::value::Revision;
use mindweaver_core::{Kind, SourceKind, WeaverWorkspace};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// What is fetched from
// ---------------------------------------------------------------------------

/// A place to fetch from, and the remote that fetches it: what a test of the
/// contract asks of each adapter, so the same test runs against all of them.
trait Upstream {
    /// A repository holding exactly `files`, at one revision.
    fn create(files: &[(&str, &str)]) -> Self;
    /// The URL a fetch of it is handed.
    fn url(&self) -> String;
    /// A URL where there is no repository at all.
    fn nowhere(&self) -> String;
    /// The revision its default branch is at.
    fn head(&self) -> Revision;
    /// A new revision of the default branch, with `files` written over what
    /// it holds.
    fn commit(&self, files: &[(&str, &str)]);
    /// A branch called `name`, at the revision the default branch is at now.
    fn branch(&self, name: &str);
    /// The remote under test.
    fn remote(&self) -> Arc<dyn Remote>;
}

/// A git repository on the disk, fetched by `gix`.
struct Repository(TempDir);

impl Upstream for Repository {
    fn create(files: &[(&str, &str)]) -> Self {
        Repository(GitRepositoryMother::create(files))
    }

    fn url(&self) -> String {
        GitRepositoryMother::url(&self.0)
    }

    fn nowhere(&self) -> String {
        self.0
            .path()
            .join("no-repository")
            .to_string_lossy()
            .into_owned()
    }

    fn head(&self) -> Revision {
        // Read by `gix` with no configuration of the machine's, rather than
        // off the file the mother writes, so a mother that wrote the wrong
        // one would not agree with itself.
        let repository =
            gix::open_opts(self.0.path(), gix::open::Options::isolated()).expect("open it");
        Revision::from(repository.head_id().expect("a commit").to_string())
    }

    fn commit(&self, files: &[(&str, &str)]) {
        GitRepositoryMother::write(self.0.path(), files);
        GitRepositoryMother::commit(self.0.path());
    }

    fn branch(&self, name: &str) {
        let heads = self.0.path().join(".git").join("refs").join("heads");
        fs::create_dir_all(&heads).unwrap();
        fs::write(heads.join(name), format!("{}\n", self.head())).unwrap();
    }

    fn remote(&self) -> Arc<dyn Remote> {
        Arc::new(Gix)
    }
}

/// The branch a forge's repository is at unless another is asked for.
const DEFAULT_BRANCH: &str = "main";

/// The files of one revision, by path.
type Files = BTreeMap<String, String>;

/// A remote of a test's own, kept in memory: one repository at one URL, each
/// branch a revision of some files, fetched by writing them out.
///
/// Written to the contract alone, with no git anywhere in it, so that what
/// the suite holds both adapters to is what the port says.
#[derive(Debug)]
struct Forge {
    url: String,
    branches: Mutex<BTreeMap<String, (Revision, Files)>>,
}

impl Remote for Forge {
    fn fetch(
        &self,
        url: &str,
        reference: Option<&str>,
        into: &Path,
        watch: &Watch,
        _token: Option<&Token>,
    ) -> Result<Cloned, GitError> {
        if watch.is_cancelled() {
            return Err(GitError::Cancelled);
        }
        let _ = fs::remove_dir_all(into);
        let failed = |detail: String| GitError::Fetch {
            url: redacted(url).into_owned(),
            detail,
        };
        if url != self.url {
            return Err(failed(String::from("no repository there")));
        }
        let branch = reference.unwrap_or(DEFAULT_BRANCH);
        let Some((revision, files)) = self.branches.lock().unwrap().get(branch).cloned() else {
            return Err(failed(format!("no ref matched '{branch}'")));
        };
        watch.report(Progress::CheckingOut {
            done: 0,
            total: None,
        });
        for (path, text) in &files {
            let file = into.join(path);
            fs::create_dir_all(file.parent().unwrap()).map_err(|source| GitError::Cache {
                path: into.to_path_buf(),
                source,
            })?;
            fs::write(&file, text).map_err(|source| GitError::Cache {
                path: file.clone(),
                source,
            })?;
        }
        fs::create_dir_all(into).map_err(|source| GitError::Cache {
            path: into.to_path_buf(),
            source,
        })?;
        watch.report(Progress::CheckingOut {
            done: files.len(),
            total: Some(files.len()),
        });
        if watch.is_cancelled() {
            let _ = fs::remove_dir_all(into);
            return Err(GitError::Cancelled);
        }
        Ok(Cloned {
            root: into.to_path_buf(),
            revision: Some(revision),
        })
    }

    fn scrub(&self, _clone: &Path) -> Result<bool, GitError> {
        // It writes no URL anywhere, so there is never one to take out.
        Ok(false)
    }
}

impl Upstream for Arc<Forge> {
    fn create(files: &[(&str, &str)]) -> Self {
        let files: Files = files
            .iter()
            .map(|(path, text)| (path.to_string(), text.to_string()))
            .collect();
        Arc::new(Forge {
            url: UrlMother::https(),
            branches: Mutex::new(BTreeMap::from([(
                String::from(DEFAULT_BRANCH),
                (RevisionMother::random(), files),
            )])),
        })
    }

    fn url(&self) -> String {
        self.url.clone()
    }

    fn nowhere(&self) -> String {
        UrlMother::https()
    }

    fn head(&self) -> Revision {
        self.branches.lock().unwrap()[DEFAULT_BRANCH].0.clone()
    }

    fn commit(&self, files: &[(&str, &str)]) {
        let mut branches = self.branches.lock().unwrap();
        let (revision, held) = branches.get_mut(DEFAULT_BRANCH).unwrap();
        for (path, text) in files {
            held.insert(path.to_string(), text.to_string());
        }
        *revision = RevisionMother::random();
    }

    fn branch(&self, name: &str) {
        let mut branches = self.branches.lock().unwrap();
        let main = branches[DEFAULT_BRANCH].clone();
        branches.insert(name.to_owned(), main);
    }

    fn remote(&self) -> Arc<dyn Remote> {
        Arc::clone(self) as Arc<dyn Remote>
    }
}

/// The fake by the name the suite runs it under.
type Fake = Arc<Forge>;

/// Every test named, as a `#[test]` of its own against `$upstream`.
macro_rules! contract {
    ($upstream:ty as $module:ident: $($test:ident),* $(,)?) => {
        mod $module {
            $(
                #[test]
                fn $test() {
                    super::$test::<$upstream>();
                }
            )*
        }
    };
}

/// Every test named, against every remote. A new adapter joins by an
/// `Upstream` of its own and a line here.
macro_rules! every_remote {
    ($($test:ident),* $(,)?) => {
        contract!(super::Repository as through_gix: $($test),*);
        contract!(super::Fake as through_a_fake: $($test),*);
    };
}

every_remote![
    a_fetch_writes_the_tree_where_it_was_asked_and_names_its_root_and_revision,
    a_fetch_replaces_whatever_was_at_its_destination,
    a_fetch_into_a_folder_whose_parents_are_not_made_yet_makes_them,
    a_fetch_after_a_new_commit_moves_the_revision_and_the_tree,
    a_fetch_at_a_branch_takes_that_branch_rather_than_the_default,
    a_fetch_leaves_the_repository_it_fetched_from_as_it_was,
    a_fetch_of_a_branch_the_repository_lacks_fails_with_a_code_and_leaves_nothing,
    a_fetch_of_a_repository_that_is_not_there_fails_with_a_code_and_leaves_nothing,
    a_fetch_cancelled_before_it_starts_stops_and_leaves_its_destination_alone,
    a_fetch_cancelled_at_its_first_step_stops_and_leaves_nothing_of_the_clone,
    a_fetch_tells_its_watch_only_the_steps_of_a_fetch,
    a_fresh_clone_has_nothing_to_scrub_and_neither_has_a_folder_never_cloned,
    a_gather_through_the_remote_shelves_every_skill_it_fetched,
    a_gather_hands_the_remote_its_url_its_branch_and_the_sources_cache_folder,
    a_gather_again_after_a_commit_says_what_changed,
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Some skills under `skills/`, one with a file beside its manifest, and a
/// file outside the folder a gather reads: the names, and every file.
fn skills() -> (Vec<String>, Vec<(String, String)>) {
    let mut names: Vec<String> = Vec::new();
    while names.len() < random::between(1, 5) {
        let name = random::kebab();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    let mut files: Vec<(String, String)> = names
        .iter()
        .map(|name| {
            (
                format!("skills/{name}/SKILL.md"),
                SkillMother::create(name, &SkillMother::description()),
            )
        })
        .collect();
    files.push((
        format!("skills/{}/scripts/run.sh", names[0]),
        String::from("#!/bin/sh\necho run\n"),
    ));
    files.push((String::from("README.md"), random::text(40)));
    names.sort();
    (names, files)
}

/// `files` as the borrowed pairs a repository is made of.
fn borrowed(files: &[(String, String)]) -> Vec<(&str, &str)> {
    files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect()
}

/// `files` as a tree reads them, by path.
fn tree(files: &[(String, String)]) -> Files {
    files.iter().cloned().collect()
}

/// Every file under `root` but what git keeps in its `.git`, by its path
/// from `root`, `/` separated, with what it holds.
fn tree_of(root: &Path) -> Files {
    fn walk(root: &Path, folder: &Path, found: &mut Files) {
        for entry in fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if folder == root && path.file_name() == Some(".git".as_ref()) {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, found);
            } else {
                let route = path.strip_prefix(root).unwrap();
                let route: Vec<String> = route
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                found.insert(route.join("/"), fs::read_to_string(&path).unwrap());
            }
        }
    }
    let mut found = Files::new();
    walk(root, root, &mut found);
    found
}

/// A folder for clones, and a place in it nothing is at yet.
fn destination() -> (TempDir, PathBuf) {
    let cache = TempDir::new().expect("make a temporary folder");
    let into = cache.path().join(random::word());
    (cache, into)
}

/// What a test leaves at a clone's place before a fetch, to see whether the
/// fetch replaced it.
fn left_there(into: &Path) {
    fs::create_dir_all(into.join("stray")).unwrap();
    fs::write(into.join("stray").join("file"), "left by somebody").unwrap();
}

/// A watch that keeps every step it is told, and where they are kept.
fn recording() -> (Watch, Arc<Mutex<Vec<Progress>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let watch = Watch::new().reporting_to({
        let seen = Arc::clone(&seen);
        move |progress| seen.lock().unwrap().push(progress)
    });
    (watch, seen)
}

/// Whether `error` is a failure a remote says of its own, rather than a
/// cancel, and gives a code `GitError` lists.
fn is_a_coded_failure(error: &GitError) -> bool {
    !matches!(error, GitError::Cancelled) && GitError::CODES.contains(&error.code())
}

// ---------------------------------------------------------------------------
// A fetch
// ---------------------------------------------------------------------------

fn a_fetch_writes_the_tree_where_it_was_asked_and_names_its_root_and_revision<U: Upstream>() {
    let (_, files) = skills();
    let upstream = U::create(&borrowed(&files));
    let (_cache, into) = destination();

    let cloned = upstream
        .remote()
        .fetch(&upstream.url(), None, &into, &Watch::new(), None)
        .unwrap();

    assert_eq!(
        fs::canonicalize(&cloned.root).unwrap(),
        fs::canonicalize(&into).unwrap()
    );
    assert_eq!(tree_of(&cloned.root), tree(&files));
    assert_eq!(cloned.revision, Some(upstream.head()));
}

fn a_fetch_replaces_whatever_was_at_its_destination<U: Upstream>() {
    let (_, files) = skills();
    let upstream = U::create(&borrowed(&files));
    let (_cache, into) = destination();
    left_there(&into);

    let cloned = upstream
        .remote()
        .fetch(&upstream.url(), None, &into, &Watch::new(), None)
        .unwrap();

    assert_eq!(tree_of(&cloned.root), tree(&files));
    assert!(!into.join("stray").exists());
}

fn a_fetch_into_a_folder_whose_parents_are_not_made_yet_makes_them<U: Upstream>() {
    let (_, files) = skills();
    let upstream = U::create(&borrowed(&files));
    let (cache, _) = destination();
    let into = cache
        .path()
        .join(random::word())
        .join(random::word())
        .join("clone");

    let cloned = upstream
        .remote()
        .fetch(&upstream.url(), None, &into, &Watch::new(), None)
        .unwrap();

    assert_eq!(tree_of(&cloned.root), tree(&files));
}

fn a_fetch_after_a_new_commit_moves_the_revision_and_the_tree<U: Upstream>() {
    let (names, mut files) = skills();
    let upstream = U::create(&borrowed(&files));
    let (_cache, into) = destination();
    let remote = upstream.remote();
    let first = remote
        .fetch(&upstream.url(), None, &into, &Watch::new(), None)
        .unwrap();
    let changed = vec![
        (
            format!("skills/{}/SKILL.md", names[0]),
            SkillMother::create(&names[0], "Changed upstream"),
        ),
        (
            String::from("skills/added/SKILL.md"),
            SkillMother::create("added", "Added upstream"),
        ),
    ];

    upstream.commit(&borrowed(&changed));
    let second = remote
        .fetch(&upstream.url(), None, &into, &Watch::new(), None)
        .unwrap();

    assert_ne!(second.revision, first.revision, "{first:?}");
    assert_eq!(second.revision, Some(upstream.head()));
    files.retain(|(path, _)| !changed.iter().any(|(at, _)| at == path));
    files.extend(changed);
    assert_eq!(tree_of(&second.root), tree(&files));
}

fn a_fetch_at_a_branch_takes_that_branch_rather_than_the_default<U: Upstream>() {
    let (names, files) = skills();
    let upstream = U::create(&borrowed(&files));
    let kept = upstream.head();
    upstream.branch("next");
    upstream.commit(&[(
        &format!("skills/{}/SKILL.md", names[0]),
        &SkillMother::create(&names[0], "Only on the default branch"),
    )]);
    let remote = upstream.remote();
    let (_cache, into) = destination();

    let at_branch = remote
        .fetch(
            &upstream.url(),
            Some("next"),
            &into.join("next"),
            &Watch::new(),
            None,
        )
        .unwrap();
    let at_default = remote
        .fetch(
            &upstream.url(),
            None,
            &into.join("default"),
            &Watch::new(),
            None,
        )
        .unwrap();

    assert_eq!(at_branch.revision, Some(kept));
    assert_eq!(tree_of(&at_branch.root), tree(&files));
    assert_eq!(at_default.revision, Some(upstream.head()));
    assert_ne!(tree_of(&at_default.root), tree(&files));
}

fn a_fetch_leaves_the_repository_it_fetched_from_as_it_was<U: Upstream>() {
    let (_, files) = skills();
    let upstream = U::create(&borrowed(&files));
    let head = upstream.head();
    let (_cache, into) = destination();
    let remote = upstream.remote();

    let first = remote
        .fetch(
            &upstream.url(),
            None,
            &into.join("first"),
            &Watch::new(),
            None,
        )
        .unwrap();
    let second = remote
        .fetch(
            &upstream.url(),
            None,
            &into.join("second"),
            &Watch::new(),
            None,
        )
        .unwrap();

    assert_eq!(upstream.head(), head);
    assert_eq!(first.revision, second.revision);
    assert_eq!(tree_of(&first.root), tree_of(&second.root));
}

fn a_fetch_of_a_branch_the_repository_lacks_fails_with_a_code_and_leaves_nothing<U: Upstream>() {
    let (_, files) = skills();
    let upstream = U::create(&borrowed(&files));
    let (_cache, into) = destination();
    left_there(&into);
    let branch = format!("no-{}", random::word());

    let error = upstream
        .remote()
        .fetch(&upstream.url(), Some(&branch), &into, &Watch::new(), None)
        .unwrap_err();

    assert!(is_a_coded_failure(&error), "{branch}: {error:?}");
    assert_eq!(error.code(), "git_fetch_failed", "{error}");
    assert!(!into.exists(), "{error}");
}

fn a_fetch_of_a_repository_that_is_not_there_fails_with_a_code_and_leaves_nothing<U: Upstream>() {
    let (_, files) = skills();
    let upstream = U::create(&borrowed(&files));
    let (_cache, into) = destination();
    let nowhere = upstream.nowhere();

    let error = upstream
        .remote()
        .fetch(&nowhere, None, &into, &Watch::new(), None)
        .unwrap_err();

    assert!(is_a_coded_failure(&error), "{nowhere}: {error:?}");
    assert_eq!(error.code(), "git_fetch_failed", "{error}");
    assert!(!into.exists(), "{error}");
}

fn a_fetch_cancelled_before_it_starts_stops_and_leaves_its_destination_alone<U: Upstream>() {
    let (_, files) = skills();
    let upstream = U::create(&borrowed(&files));
    let (_cache, into) = destination();
    left_there(&into);
    let (watch, seen) = recording();
    watch.cancel();

    let error = upstream
        .remote()
        .fetch(&upstream.url(), None, &into, &watch, None)
        .unwrap_err();

    assert!(matches!(error, GitError::Cancelled), "{error:?}");
    assert_eq!(error.code(), "git_clone_cancelled");
    assert!(
        into.join("stray").join("file").is_file(),
        "a cancel cost it"
    );
    assert_eq!(*seen.lock().unwrap(), [], "nothing was under way");
}

fn a_fetch_cancelled_at_its_first_step_stops_and_leaves_nothing_of_the_clone<U: Upstream>() {
    let (_, files) = skills();
    let upstream = U::create(&borrowed(&files));
    let (_cache, into) = destination();
    let watch = Watch::new();
    let cancel = watch.clone();
    let watch = watch.reporting_to(move |_| cancel.cancel());

    let error = upstream
        .remote()
        .fetch(&upstream.url(), None, &into, &watch, None)
        .unwrap_err();

    assert!(matches!(error, GitError::Cancelled), "{error:?}");
    assert!(!into.exists(), "what there was of it is gone");
}

fn a_fetch_tells_its_watch_only_the_steps_of_a_fetch<U: Upstream>() {
    let (_, files) = skills();
    let upstream = U::create(&borrowed(&files));
    let (_cache, into) = destination();
    let (watch, seen) = recording();

    upstream
        .remote()
        .fetch(&upstream.url(), None, &into, &watch, None)
        .unwrap();

    let seen = seen.lock().unwrap();
    assert!(!seen.is_empty(), "a fetch says how it goes");
    for step in seen.iter() {
        // Reading and placing are the gather's own, said once the remote
        // has handed its clone back.
        assert!(
            matches!(
                step,
                Progress::Connecting
                    | Progress::Receiving { .. }
                    | Progress::Resolving { .. }
                    | Progress::CheckingOut { .. }
            ),
            "{step:?} among {seen:?}"
        );
        assert!(step.cancellable(), "{step:?}");
    }
}

fn a_fresh_clone_has_nothing_to_scrub_and_neither_has_a_folder_never_cloned<U: Upstream>() {
    let (_, files) = skills();
    let upstream = U::create(&borrowed(&files));
    let (cache, into) = destination();
    let remote = upstream.remote();
    remote
        .fetch(&upstream.url(), None, &into, &Watch::new(), None)
        .unwrap();
    let before = tree_of(&into);

    assert!(!remote.scrub(&into).unwrap());
    assert!(!remote.scrub(&into).unwrap(), "nor the second time");
    assert!(!remote.scrub(&cache.path().join("never-cloned")).unwrap());
    assert_eq!(tree_of(&into), before);
}

// ---------------------------------------------------------------------------
// A gather through each remote
// ---------------------------------------------------------------------------

/// A workspace to gather into, with its ledger.
fn gathering() -> (TempDir, WeaverWorkspace, mindweaver_core::Ledger) {
    let (dir, workspace) = WorkspaceMother::empty();
    let ledger = workspace.ledger().expect("open the ledger");
    (dir, workspace, ledger)
}

fn a_gather_through_the_remote_shelves_every_skill_it_fetched<U: Upstream>() {
    let (names, files) = skills();
    let upstream = U::create(&borrowed(&files));
    let (_dir, workspace, ledger) = gathering();

    let report = gather::gather(
        &workspace,
        &ledger,
        &Request::git(upstream.url()).through(upstream.remote()),
    )
    .unwrap();

    let added: Vec<&str> = report.added.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(added, names);
    assert_eq!(report.revision, Some(upstream.head()));
    let shelf = workspace.gathered_dir(Kind::Skill).join(&report.directory);
    let expected: Files = files
        .iter()
        .filter_map(|(path, text)| {
            path.strip_prefix("skills/")
                .map(|inside| (inside.to_owned(), text.clone()))
        })
        .collect();
    assert_eq!(tree_of(&shelf), expected);
    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), names.len());
    assert!(
        gathered
            .iter()
            .all(|artifact| artifact.revision == Some(upstream.head())),
        "{gathered:#?}"
    );
    assert_eq!(ledger.sources().unwrap()[0].revision, Some(upstream.head()));
}

/// A remote that notes what each fetch is asked for, and hands it on.
#[derive(Debug)]
struct Recording {
    inner: Arc<dyn Remote>,
    asked: Mutex<Vec<(String, Option<String>, PathBuf)>>,
    /// The token each fetch was handed, in the order they were asked.
    tokens: Mutex<Vec<Option<Token>>>,
}

impl Remote for Recording {
    fn fetch(
        &self,
        url: &str,
        reference: Option<&str>,
        into: &Path,
        watch: &Watch,
        token: Option<&Token>,
    ) -> Result<Cloned, GitError> {
        self.asked.lock().unwrap().push((
            url.to_owned(),
            reference.map(str::to_owned),
            into.to_path_buf(),
        ));
        self.tokens.lock().unwrap().push(token.cloned());
        self.inner.fetch(url, reference, into, watch, token)
    }

    fn scrub(&self, clone: &Path) -> Result<bool, GitError> {
        self.inner.scrub(clone)
    }
}

fn a_gather_hands_the_remote_its_url_its_branch_and_the_sources_cache_folder<U: Upstream>() {
    let (_, files) = skills();
    let upstream = U::create(&borrowed(&files));
    upstream.branch("next");
    let recording = Arc::new(Recording {
        inner: upstream.remote(),
        asked: Mutex::new(Vec::new()),
        tokens: Mutex::new(Vec::new()),
    });
    let (_dir, workspace, ledger) = gathering();

    let report = gather::gather(
        &workspace,
        &ledger,
        &Request::git(upstream.url())
            .at(Some("next"))
            .through(recording.clone()),
    )
    .unwrap();

    assert_eq!(
        *recording.asked.lock().unwrap(),
        [(
            upstream.url(),
            Some(String::from("next")),
            workspace.cache_dir().join(&report.directory),
        )]
    );
    assert_eq!(*recording.tokens.lock().unwrap(), [None]);
}

/// Every file under `root`, `.git` and the ledger included, whose bytes
/// hold `text`.
fn holding(root: &Path, text: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else if String::from_utf8_lossy(&fs::read(&path).unwrap()).contains(text) {
                found.push(path);
            }
        }
    }
    found
}

/// A token a front end handed over reaches the remote with each fetch, a
/// dry run's too, and nothing a gather writes or says holds it: not the
/// ledger, the clone, the shelf nor the report.
#[test]
fn a_gather_hands_its_token_to_the_remote_and_keeps_none_of_it() {
    let (names, files) = skills();
    let forge = Fake::create(&borrowed(&files));
    let recording = Arc::new(Recording {
        inner: forge.remote(),
        asked: Mutex::new(Vec::new()),
        tokens: Mutex::new(Vec::new()),
    });
    let (dir, workspace, ledger) = gathering();
    let secret = random::hex(40);
    let token = Token::new(secret.clone());
    let request = Request::git(forge.url())
        .with_token(Some(token.clone()))
        .through(recording.clone());

    let report = gather::gather(&workspace, &ledger, &request).unwrap();
    let foreseen = gather::dry_run(&workspace, &ledger, &request).unwrap();

    assert_eq!(report.added.len(), names.len());
    assert_eq!(foreseen.unchanged.len(), names.len());
    assert_eq!(
        *recording.tokens.lock().unwrap(),
        [Some(token.clone()), Some(token)]
    );
    assert_eq!(holding(dir.path(), &secret), Vec::<PathBuf>::new());
    let said = serde_json::to_string(&(report, ledger.history(10).unwrap())).unwrap();
    assert!(!said.contains(&secret), "{said}");
}

fn a_gather_again_after_a_commit_says_what_changed<U: Upstream>() {
    let (names, files) = skills();
    let upstream = U::create(&borrowed(&files));
    let (_dir, workspace, ledger) = gathering();
    let request = Request::git(upstream.url()).through(upstream.remote());
    gather::gather(&workspace, &ledger, &request).unwrap();

    let changed = &names[0];
    upstream.commit(&[
        (
            &format!("skills/{changed}/SKILL.md"),
            &SkillMother::create(changed, "Changed upstream"),
        ),
        (
            "skills/zz-added/SKILL.md",
            &SkillMother::create("zz-added", "Added upstream"),
        ),
    ]);
    let report = gather::gather(&workspace, &ledger, &request).unwrap();

    let named = |harvested: &[gather::Harvested]| -> Vec<String> {
        harvested.iter().map(|h| h.name.to_string()).collect()
    };
    assert_eq!(named(&report.updated), std::slice::from_ref(changed));
    assert_eq!(named(&report.added), ["zz-added"]);
    assert_eq!(named(&report.unchanged), names[1..]);
    assert_eq!(report.revision, Some(upstream.head()));
}

#[test]
fn gathers_through_gix_and_through_a_fake_put_the_same_skills_on_the_shelf() {
    let (_, files) = skills();
    let repository = Repository::create(&borrowed(&files));
    let forge = Fake::create(&borrowed(&files));
    let mut shelved = Vec::new();
    for (url, remote) in [
        (repository.url(), repository.remote()),
        (forge.url(), forge.remote()),
    ] {
        let (_dir, workspace, ledger) = gathering();
        let report =
            gather::gather(&workspace, &ledger, &Request::git(&url).through(remote)).unwrap();
        shelved.push(tree_of(
            &workspace.gathered_dir(Kind::Skill).join(&report.directory),
        ));
    }

    assert_eq!(shelved[0], shelved[1]);
    assert!(!shelved[0].is_empty());
}

/// A remote that fails every fetch the way a server that refuses one does.
#[derive(Debug)]
struct Refusing;

impl Remote for Refusing {
    fn fetch(
        &self,
        url: &str,
        _reference: Option<&str>,
        _into: &Path,
        _watch: &Watch,
        _token: Option<&Token>,
    ) -> Result<Cloned, GitError> {
        Err(GitError::Fetch {
            url: redacted(url).into_owned(),
            detail: String::from("the server hung up"),
        })
    }

    fn scrub(&self, _clone: &Path) -> Result<bool, GitError> {
        Ok(false)
    }
}

#[test]
fn a_remote_that_cannot_fetch_fails_the_gather_with_its_code_and_the_failure_is_logged() {
    let (_dir, workspace, ledger) = gathering();
    let url = UrlMother::https();

    let error = gather::gather(
        &workspace,
        &ledger,
        &Request::git(&url).through(Arc::new(Refusing)),
    )
    .unwrap_err();

    assert!(
        matches!(error, GatherError::Git(GitError::Fetch { .. })),
        "{error:?}"
    );
    assert_eq!(error.code(), "git_fetch_failed");
    assert_eq!(ledger.gathered().unwrap(), []);
    // A place the shelf is filled from, though nothing came of it yet.
    let sources = ledger.sources().unwrap();
    assert_eq!(sources.len(), 1, "{sources:?}");
    assert_eq!(sources[0].url, url.as_str());
    assert_eq!(sources[0].revision, None);
    let history = ledger.history(10).unwrap();
    assert_eq!(history.len(), 1, "{history:?}");
    assert_eq!(history[0].action, Action::Gather.slug());
    assert_eq!(history[0].outcome, Outcome::Failed.slug());
    assert_eq!(history[0].target.as_deref(), Some(url.as_str()));
    assert!(
        history[0]
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("the server hung up")),
        "{history:?}"
    );
}

/// A remote that is cancelled whatever it is asked.
#[derive(Debug)]
struct Cancelling;

impl Remote for Cancelling {
    fn fetch(
        &self,
        _url: &str,
        _reference: Option<&str>,
        _into: &Path,
        _watch: &Watch,
        _token: Option<&Token>,
    ) -> Result<Cloned, GitError> {
        Err(GitError::Cancelled)
    }

    fn scrub(&self, _clone: &Path) -> Result<bool, GitError> {
        Ok(false)
    }
}

#[test]
fn a_remote_that_says_it_was_cancelled_leaves_no_source_behind() {
    let (_dir, workspace, ledger) = gathering();
    let url = UrlMother::https();

    let error = gather::gather(
        &workspace,
        &ledger,
        &Request::git(&url).through(Arc::new(Cancelling)),
    )
    .unwrap_err();

    assert!(matches!(error, GatherError::Cancelled), "{error:?}");
    assert_eq!(
        ledger
            .source(
                SourceKind::Git,
                &url,
                None,
                mindweaver_core::DEFAULT_SUBDIRECTORY
            )
            .unwrap(),
        None,
        "{url}"
    );
    assert_eq!(ledger.sources().unwrap(), []);
    assert_eq!(ledger.history(10).unwrap(), [], "a cancel is not logged");
    assert!(!workspace.gathered_dir(Kind::Skill).exists());
}

/// A remote whose clone resolved no commit, as one of a repository with
/// none is, or whose root is a folder inside where it was asked to fetch.
#[derive(Debug)]
struct Reshaped {
    inner: Arc<dyn Remote>,
    revision: bool,
    inside: Option<&'static str>,
}

impl Remote for Reshaped {
    fn fetch(
        &self,
        url: &str,
        reference: Option<&str>,
        into: &Path,
        watch: &Watch,
        token: Option<&Token>,
    ) -> Result<Cloned, GitError> {
        let at = self
            .inside
            .map_or_else(|| into.to_path_buf(), |inside| into.join(inside));
        let cloned = self.inner.fetch(url, reference, &at, watch, token)?;
        Ok(Cloned {
            root: cloned.root,
            revision: cloned.revision.filter(|_| self.revision),
        })
    }

    fn scrub(&self, clone: &Path) -> Result<bool, GitError> {
        self.inner.scrub(clone)
    }
}

#[test]
fn a_remote_that_resolves_no_revision_gathers_without_one() {
    let (names, files) = skills();
    let forge = Fake::create(&borrowed(&files));
    let (_dir, workspace, ledger) = gathering();
    let remote = Arc::new(Reshaped {
        inner: forge.remote(),
        revision: false,
        inside: None,
    });

    let report = gather::gather(
        &workspace,
        &ledger,
        &Request::git(forge.url()).through(remote),
    )
    .unwrap();

    assert_eq!(report.revision, None);
    assert_eq!(report.added.len(), names.len());
    let gathered = ledger.gathered().unwrap();
    assert!(
        gathered.iter().all(|artifact| artifact.revision.is_none()),
        "{gathered:#?}"
    );
    assert_eq!(ledger.sources().unwrap()[0].revision, None);
}

#[test]
fn a_gather_reads_from_the_root_the_remote_hands_back() {
    let (names, files) = skills();
    let repository = Repository::create(&borrowed(&files));
    let (_dir, workspace, ledger) = gathering();
    let remote = Arc::new(Reshaped {
        inner: repository.remote(),
        revision: true,
        inside: Some("worktree"),
    });

    let report = gather::gather(
        &workspace,
        &ledger,
        &Request::git(repository.url()).through(remote),
    )
    .unwrap();

    let added: Vec<&str> = report.added.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(added, names);
    assert!(workspace
        .cache_dir()
        .join(&report.directory)
        .join("worktree")
        .join("skills")
        .is_dir());
}

// ---------------------------------------------------------------------------
// What only Gix says
// ---------------------------------------------------------------------------

#[test]
fn gix_names_a_revision_by_the_full_commit_id_the_repository_holds() {
    let (_, files) = skills();
    let repository = Repository::create(&borrowed(&files));
    let (_cache, into) = destination();

    let cloned = Gix
        .fetch(&repository.url(), None, &into, &Watch::new(), None)
        .unwrap();

    let revision = cloned.revision.expect("a commit to resolve");
    assert_eq!(revision.len(), 40, "{revision}");
    assert!(
        revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{revision}"
    );
    assert_eq!(revision, repository.head());
}

#[test]
fn gix_fetches_a_tag_as_it_fetches_a_branch() {
    let (_, files) = skills();
    let repository = Repository::create(&borrowed(&files));
    let tagged = repository.head();
    let tags = repository.0.path().join(".git").join("refs").join("tags");
    fs::create_dir_all(&tags).unwrap();
    fs::write(tags.join("v1"), format!("{tagged}\n")).unwrap();
    repository.commit(&[("README.md", "after the tag\n")]);
    let (_cache, into) = destination();

    let cloned = Gix
        .fetch(&repository.url(), Some("v1"), &into, &Watch::new(), None)
        .unwrap();

    assert_eq!(cloned.revision, Some(tagged));
    assert_eq!(tree_of(&cloned.root), tree(&files));
}

#[test]
fn gix_keeps_the_clone_a_gather_can_be_looked_at_afterwards_by() {
    let (_, files) = skills();
    let repository = Repository::create(&borrowed(&files));
    let (_cache, into) = destination();

    Gix.fetch(&repository.url(), None, &into, &Watch::new(), None)
        .unwrap();

    let clone = gix::open_opts(&into, gix::open::Options::isolated()).expect("a repository");
    assert_eq!(
        Revision::from(clone.head_id().unwrap().to_string()),
        repository.head()
    );
}

#[test]
fn a_request_fetches_through_gix_unless_another_remote_is_handed_over() {
    let request = Request::git(UrlMother::https());

    assert_eq!(format!("{:?}", request.remote), format!("{Gix:?}"));
    let handed = request.through(Arc::new(Refusing));
    assert_eq!(format!("{:?}", handed.remote), format!("{Refusing:?}"));
}
