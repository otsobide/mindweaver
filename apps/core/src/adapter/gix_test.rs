//! Unit tests for `gix.rs`, kept beside it rather than inside it.

use super::*;

use gix::credentials::helper::Action;
use gix::credentials::protocol;

/// What the credentials callback notes when the server turns down the login a
/// helper gave for `connected`, the URL `gix` connected to.
// The stub helper's error is `gix`'s, fixed by the callback's signature.
#[allow(clippy::result_large_err)]
fn refused_at(connected: &str) -> Option<Shortfall> {
    let shortfalls = Shortfalls::default();
    let mut callback = credentials::never_prompting(
        |action, _| match action {
            Action::Get(context) => Ok(Some(protocol::Outcome {
                identity: gix::sec::identity::Account {
                    username: "git".into(),
                    password: "hunter2".into(),
                    oauth_refresh_token: None,
                },
                next: context.into(),
            })),
            Action::Store(_) | Action::Erase(_) => Ok(None),
        },
        shortfalls.clone(),
    );
    let Ok(Some(answer)) = callback(Action::get_for_url(connected)) else {
        panic!("{connected}: the helper's credential was not passed on");
    };
    callback(answer.next.erase()).unwrap();
    shortfalls.last()
}

#[test]
fn a_missing_credential_names_the_url_without_its_userinfo_and_the_ways_to_provide_one() {
    let error = fetch_failed(
        "https://me:hunter2@example.com/acme/private.git",
        Some(Shortfall::Missing),
        &"Failed to obtain credentials",
    );

    assert_eq!(
        error.to_string(),
        "https://example.com/acme/private.git: needs credentials, and gathering never \
         prompts for them: configure a git credential helper, or use ssh-agent for an ssh URL"
    );
}

#[test]
fn a_refused_login_is_judged_by_the_url_gix_connected_to_not_the_one_typed() {
    // `url."git@example.com:".insteadOf = https://example.com/`: typed as
    // https, fetched over ssh, and ssh had no key the server takes.
    let error = fetch_failed(
        "https://example.com/acme/private.git",
        refused_at("git@example.com:acme/private.git"),
        &"not accepted by the remote",
    );
    assert!(matches!(error, GitError::Credentials { .. }), "{error}");

    // The reverse rewrite: typed as ssh, fetched over https, and the server
    // turned down the credential a helper gave.
    let error = fetch_failed(
        "git@example.com:acme/private.git",
        refused_at("https://example.com/acme/private.git"),
        &"not accepted by the remote",
    );
    assert_eq!(
        error.to_string(),
        "git@example.com:acme/private.git: cannot be fetched: not accepted by the remote"
    );
}

#[test]
fn a_refused_https_credential_keeps_what_gix_said() {
    let error = fetch_failed(
        "https://example.com/acme/private.git",
        Some(Shortfall::Refused),
        &"not accepted by the remote",
    );

    assert_eq!(
        error.to_string(),
        "https://example.com/acme/private.git: cannot be fetched: not accepted by the remote"
    );
}

#[test]
fn a_fetch_that_needed_no_credential_keeps_what_gix_said() {
    let error = fetch_failed(
        "https://example.com/acme/skills",
        None,
        &"connection refused",
    );

    assert!(matches!(error, GitError::Fetch { .. }), "{error}");
}

#[test]
fn a_failed_fetch_names_no_credential_even_where_gix_repeats_one() {
    // `url."https://me:hunter2@example.com/".insteadOf = https://example.com/`
    // gave the URL gix connected to a token the typed one never had.
    let error = fetch_failed(
        "https://example.com/acme/private.git",
        None,
        &"error sending request for url \
          (https://me:hunter2@example.com/acme/private.git/info/refs?service=git-upload-pack)",
    );

    assert_eq!(
        error.to_string(),
        "https://example.com/acme/private.git: cannot be fetched: error sending request for url \
         (https://example.com/acme/private.git/info/refs?service=git-upload-pack)"
    );
}

#[test]
fn a_url_gix_cannot_parse_is_named_without_its_credentials_and_so_is_what_gix_said() {
    let cache = tempfile::TempDir::new().unwrap();
    let error = clone(
        "https://me:hunter2@exa mple.com/acme/private.git",
        Wanted::Branch(None),
        &cache.path().join("clone"),
        &Watch::new(),
        &[],
    )
    .unwrap_err();

    assert!(matches!(error, GitError::Url { .. }), "{error}");
    assert!(!error.to_string().contains("hunter2"), "{error}");
}

/// A clone's `.git` as `gix` leaves it after cloning `url`, without the
/// objects, which no scrub reads.
fn clone_made_from(url: &str) -> tempfile::TempDir {
    let clone = tempfile::TempDir::new().unwrap();
    let git = clone.path().join(".git");
    let logs = git.join("logs").join("refs").join("remotes").join("origin");
    fs::create_dir_all(&logs).unwrap();
    fs::write(
        git.join("config"),
        format!(
            "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = {url}\n\
             \tfetch = +refs/heads/main:refs/remotes/origin/main\n"
        ),
    )
    .unwrap();
    let entry = format!("0000 1111 Fixture <fixture@example.com> 0 +0000\tclone: from {url}\n");
    fs::write(git.join("logs").join("HEAD"), &entry).unwrap();
    fs::write(logs.join("HEAD"), &entry).unwrap();
    clone
}

/// Every file under `root`, read as text, for asserting on what is left.
fn everything_in(root: &Path) -> String {
    let mut files = Vec::new();
    files_under(root, &mut files).unwrap();
    files
        .iter()
        .map(|file| fs::read_to_string(file).unwrap())
        .collect()
}

#[test]
fn scrubbing_a_clone_takes_the_credentials_out_of_its_config_and_its_reflogs() {
    let clone = clone_made_from("https://me:hunter2@example.com/acme/private.git");

    assert!(scrub(clone.path()).unwrap());

    let left = everything_in(clone.path());
    assert!(!left.contains("hunter2"), "{left}");
    assert!(
        left.contains("\turl = https://example.com/acme/private.git\n"),
        "{left}"
    );
    assert!(
        left.contains("\tclone: from https://example.com/acme/private.git\n"),
        "{left}"
    );
    // The rest of the config is left as it was.
    assert!(left.contains("fetch = +refs/heads/main:refs/remotes/origin/main"));
}

#[test]
fn scrubbing_a_clone_again_or_one_without_credentials_changes_nothing() {
    let clone = clone_made_from("https://me:hunter2@example.com/acme/private.git");
    scrub(clone.path()).unwrap();
    assert!(!scrub(clone.path()).unwrap());

    let clean = clone_made_from("git@example.com:acme/private.git");
    let before = everything_in(clean.path());
    assert!(!scrub(clean.path()).unwrap());
    assert_eq!(everything_in(clean.path()), before);

    // Nothing there at all is nothing to scrub.
    let gone = tempfile::TempDir::new().unwrap();
    assert!(!scrub(&gone.path().join("never-cloned")).unwrap());
}

#[test]
fn the_step_read_off_gix_progress_is_the_latest_under_way() {
    use gix::odb::pack::index::write::ProgressId as Pack;
    use gix::progress::Count as _;

    let root = tree::Root::new();
    let mut fetch = root.add_child("fetch");
    let mut snapshot = Vec::new();
    root.sorted_snapshot(&mut snapshot);
    assert_eq!(reading(&snapshot), None, "nothing counted yet");

    let received = fetch.add_child_with_id("indexing", Pack::IndexObjects.into());
    received.init(Some(340), None);
    received.set(120);
    root.sorted_snapshot(&mut snapshot);
    assert_eq!(
        reading(&snapshot),
        Some(Progress::Receiving {
            done: 120,
            total: Some(340)
        })
    );

    // Resolving starts while the receiving count is still in the tree.
    let resolved = fetch.add_child_with_id("Resolving", Pack::ResolveObjects.into());
    resolved.init(Some(340), None);
    resolved.set(7);
    root.sorted_snapshot(&mut snapshot);
    assert_eq!(
        reading(&snapshot),
        Some(Progress::Resolving {
            done: 7,
            total: Some(340)
        })
    );
    drop((received, resolved));

    let files = root.add_child_with_id(
        "checkout",
        gix::clone::checkout::main_worktree::ProgressId::CheckoutFiles.into(),
    );
    files.init(Some(12), None);
    // Counted the way gix counts files: on the shared counter, without
    // telling the progress.
    files.counter().fetch_add(5, Ordering::Relaxed);
    root.sorted_snapshot(&mut snapshot);
    assert_eq!(
        reading(&snapshot),
        Some(Progress::CheckingOut {
            done: 5,
            total: Some(12)
        })
    );
}

/// What `gix` is told to turn protocol v2 off with: git's own server over
/// protocol v1 hands out what its branches and tags are at, and no other
/// commit by its id, as a server that keeps `uploadpack.allowAnySHA1InWant`
/// off does.
const NO_COMMIT_BY_ITS_ID: &[&str] = &["protocol.version=1"];

/// A repository on the disk at its third commit, with a branch `side` left
/// at the first, and the second, which no branch is at: its skill's file
/// as it was then, and the commit, as a pin.
fn moved_on() -> (tempfile::TempDir, String, Pin) {
    let deploy = |description: &str| crate::mother::SkillMother::create("deploy", description);
    let repository = crate::mother::GitRepositoryMother::create(&[(
        "skills/deploy/SKILL.md",
        &deploy("Ship it, as it was first"),
    )]);
    let head = || {
        let opened = gix::open_opts(repository.path(), gix::open::Options::isolated()).unwrap();
        Pin::parse(&opened.head_id().unwrap().to_string()).unwrap()
    };
    let heads = repository.path().join(".git").join("refs").join("heads");
    fs::write(heads.join("side"), format!("{}\n", head())).unwrap();
    let second = deploy("Ship it, as it was");
    for text in [second.as_str(), &deploy("Ship it, as it is now")] {
        crate::mother::GitRepositoryMother::write(
            repository.path(),
            &[("skills/deploy/SKILL.md", text)],
        );
        crate::mother::GitRepositoryMother::commit(repository.path());
    }
    let opened = gix::open_opts(repository.path(), gix::open::Options::isolated()).unwrap();
    let parent = opened
        .head_commit()
        .unwrap()
        .parent_ids()
        .next()
        .unwrap()
        .to_string();
    (repository, second, Pin::parse(&parent).unwrap())
}

#[test]
fn a_server_that_hands_out_no_commit_by_its_id_has_the_pin_found_on_its_branch_instead() {
    let (repository, first, pin) = moved_on();
    let url = repository.path().to_string_lossy().into_owned();
    let cache = tempfile::TempDir::new().unwrap();
    let into = cache.path().join("clone");

    // Asked for alone, the commit is refused.
    let refused = clone(
        &url,
        Wanted::Commit(&pin),
        &into,
        &Watch::new(),
        NO_COMMIT_BY_ITS_ID,
    )
    .unwrap_err();
    assert_eq!(refused.code(), "git_fetch_failed", "{refused}");

    let cloned = pinned(&url, None, &pin, &into, &Watch::new(), NO_COMMIT_BY_ITS_ID).unwrap();

    assert_eq!(cloned.revision, Some(pin.revision()));
    assert_eq!(
        fs::read_to_string(cloned.root.join("skills/deploy/SKILL.md")).unwrap(),
        first
    );
    // HEAD detached at the pin, as a clone at a commit leaves it, and the
    // branch's whole history there, which is how the pin was found.
    let opened = gix::open_opts(&into, gix::open::Options::isolated()).unwrap();
    assert!(opened.head().unwrap().is_detached());
    assert_eq!(opened.head_id().unwrap().to_string(), pin.as_str());
    assert!(!into.join(".git").join("shallow").exists());
}

#[test]
fn a_pin_the_branch_does_not_reach_is_refused_when_no_commit_is_handed_out_by_its_id() {
    // The second commit, which `side`, left at the first, does not reach.
    let (repository, _, ahead) = moved_on();
    let url = repository.path().to_string_lossy().into_owned();
    let cache = tempfile::TempDir::new().unwrap();
    let into = cache.path().join("clone");

    let error = pinned(
        &url,
        Some("side"),
        &ahead,
        &into,
        &Watch::new(),
        NO_COMMIT_BY_ITS_ID,
    )
    .unwrap_err();

    assert_eq!(error.code(), "git_pin_unreachable", "{error}");
    assert_eq!(
        error.to_string(),
        format!("{url}: has no commit {ahead} that `side` reaches, so nothing is gathered at it")
    );
    assert!(!into.exists());
}

#[test]
fn a_pin_is_fetched_alone_where_the_server_hands_one_out_by_its_id() {
    let (repository, first, pin) = moved_on();
    let url = repository.path().to_string_lossy().into_owned();
    let cache = tempfile::TempDir::new().unwrap();
    let into = cache.path().join("clone");

    let cloned = pinned(&url, None, &pin, &into, &Watch::new(), &[]).unwrap();

    assert_eq!(cloned.revision, Some(pin.revision()));
    assert_eq!(
        fs::read_to_string(cloned.root.join("skills/deploy/SKILL.md")).unwrap(),
        first
    );
    // One commit, not the branch's history.
    assert!(into.join(".git").join("shallow").exists());
}
