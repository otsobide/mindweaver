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
         prompts for them: configure a git credential helper, use ssh-agent for an ssh URL, or \
         hand it a token with --token-env or --gh"
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
        &Login {
            offered: None,
            helpers: credentials::configured,
            overrides: &[],
        },
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

/// How a clone of a test of a pin logs in: with no token, through the
/// machine's own helpers, as a gather does, given `overrides`.
fn plainly(
    overrides: &'static [&'static str],
) -> Login<'static, impl FnMut(Action, prompt::Options) -> protocol::Result + 'static> {
    Login {
        offered: None,
        helpers: credentials::configured,
        overrides,
    }
}

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
        &plainly(NO_COMMIT_BY_ITS_ID),
    )
    .unwrap_err();
    assert_eq!(refused.code(), "git_fetch_failed", "{refused}");

    let cloned = pinned(
        &url,
        None,
        &pin,
        &into,
        &Watch::new(),
        &plainly(NO_COMMIT_BY_ITS_ID),
    )
    .unwrap();

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
        &plainly(NO_COMMIT_BY_ITS_ID),
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

    let cloned = pinned(&url, None, &pin, &into, &Watch::new(), &plainly(&[])).unwrap();

    assert_eq!(cloned.revision, Some(pin.revision()));
    assert_eq!(
        fs::read_to_string(cloned.root.join("skills/deploy/SKILL.md")).unwrap(),
        first
    );
    // One commit, not the branch's history.
    assert!(into.join(".git").join("shallow").exists());
}

/// A private repository's server on loopback: every request it reads is
/// asked to log in (`401`, `WWW-Authenticate: Basic`), whatever login it
/// came with, and the `Authorization` each one sent is noted, `None` for a
/// request that sent none. What a clone reaches over `gix`'s own http
/// transport, as it reaches a forge, with no network.
struct Guarded {
    address: std::net::SocketAddr,
    sent: std::sync::Arc<std::sync::Mutex<Vec<Option<String>>>>,
}

impl Guarded {
    fn listening() -> Self {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let address = listener.local_addr().expect("where it listens");
        let sent = std::sync::Arc::<std::sync::Mutex<Vec<Option<String>>>>::default();
        let noted = std::sync::Arc::clone(&sent);
        thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                while !request.windows(4).any(|end| end == b"\r\n\r\n") {
                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(read) => request.extend_from_slice(&buffer[..read]),
                    }
                }
                let request = String::from_utf8_lossy(&request).into_owned();
                let authorization = request.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("authorization")
                        .then(|| value.trim().to_owned())
                });
                noted.lock().unwrap().push(authorization);
                let _ = stream.write_all(
                    b"HTTP/1.1 401 Unauthorized\r\n\
                      WWW-Authenticate: Basic realm=\"private\"\r\n\
                      Content-Length: 0\r\n\
                      Connection: close\r\n\r\n",
                );
            }
        });
        Self { address, sent }
    }

    fn url(&self) -> String {
        format!("http://{}/acme/private.git", self.address)
    }

    fn sent(&self) -> Vec<Option<String>> {
        self.sent.lock().unwrap().clone()
    }
}

/// A cascade of no helper whose prompt panics, as the credentials tests
/// stand one in: whatever a clone asks of the helpers, a person is never
/// asked.
// The cascade's error is `gix`'s, fixed by the callback's signature.
#[allow(clippy::result_large_err)]
fn no_helper_and_a_prompt_that_panics() -> impl FnMut(Action, prompt::Options) -> protocol::Result {
    |action, options| {
        if options.mode != prompt::Mode::Disable || options.askpass.is_some() {
            panic!("the prompt was reached for {action:?}");
        }
        gix::credentials::helper::Cascade::default().invoke(action, options)
    }
}

/// [`no_helper_and_a_prompt_that_panics`], as a clone builds its helpers for
/// each connection.
#[allow(clippy::result_large_err)]
fn a_prompt_that_panics(
    _: gix::Repository,
) -> impl FnMut(Action, prompt::Options) -> protocol::Result {
    no_helper_and_a_prompt_that_panics()
}

/// A private repository's transport, as `gix`'s handshake is handed one,
/// at `url`: every handshake is asked who is fetching (what the http
/// transport makes of a `401`), whatever login it was given, and each login
/// it is given is noted.
///
/// A transport of the test's rather than a server on loopback: a token goes
/// over https alone, which a server here would have to serve with a
/// certificate the machine trusts, and `gix` sends no credential over plain
/// http at all. The handshake that asks the callback, and sets what it
/// answers on the transport, is `gix`'s own.
struct Asking {
    url: String,
    logins: Vec<gix::sec::identity::Account>,
}

impl gix::protocol::transport::client::TransportWithoutIO for Asking {
    fn set_identity(
        &mut self,
        identity: gix::sec::identity::Account,
    ) -> Result<(), gix::protocol::transport::client::Error> {
        self.logins.push(identity);
        Ok(())
    }

    fn to_url(&self) -> Cow<'_, gix::bstr::BStr> {
        Cow::Borrowed(self.url.as_str().into())
    }

    fn connection_persists_across_multiple_requests(&self) -> bool {
        false
    }

    fn configure(
        &mut self,
        _: &dyn std::any::Any,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        Ok(())
    }
}

impl gix::protocol::transport::client::blocking_io::Transport for Asking {
    fn handshake<'a>(
        &mut self,
        _: gix::protocol::transport::Service,
        _: &'a [(&'a str, Option<&'a str>)],
    ) -> Result<
        gix::protocol::transport::client::blocking_io::SetServiceResponse<'_>,
        gix::protocol::transport::client::Error,
    > {
        Err(gix::protocol::transport::client::Error::Io(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "401 Unauthorized",
        )))
    }

    fn request(
        &mut self,
        _: gix::protocol::transport::client::WriteMode,
        _: gix::protocol::transport::client::MessageKind,
        _: bool,
    ) -> Result<
        gix::protocol::transport::client::blocking_io::RequestWriter<'_>,
        gix::protocol::transport::client::Error,
    > {
        unreachable!("nothing is fetched once the handshake fails")
    }
}

/// `gix`'s handshake over an [`Asking`] transport at `url`, asking the
/// callback a clone is given: `offered`, then a cascade of no helper whose
/// prompt panics. What the transport was told, and what the callback noted.
fn handshaken(
    url: &str,
    offered: Option<credentials::Offered>,
) -> (Vec<gix::sec::identity::Account>, Option<Shortfall>) {
    let mut transport = Asking {
        url: url.to_owned(),
        logins: Vec::new(),
    };
    let shortfalls = Shortfalls::default();
    let callback = credentials::never_prompting(
        credentials::offering(offered, no_helper_and_a_prompt_that_panics()),
        shortfalls.clone(),
    );
    let shaken = gix::protocol::handshake(
        &mut transport,
        gix::protocol::transport::Service::UploadPack,
        callback,
        Vec::new(),
        &mut gix::progress::Discard,
    );
    assert!(shaken.is_err(), "the transport lets nobody in");
    (transport.logins, shortfalls.last())
}

/// Every file under `folder` that holds `text`, at any depth.
fn holding(folder: &Path, text: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let _ = files_under(folder, &mut files);
    files
        .into_iter()
        .filter(|file| {
            fs::read(file).is_ok_and(|bytes| String::from_utf8_lossy(&bytes).contains(text))
        })
        .collect()
}

#[test]
fn a_token_handed_over_is_what_the_handshake_logs_in_with_and_nothing_prompts() {
    let token = Token::new(crate::mother::random::hex(40));
    let url = "https://example.com/acme/private.git";

    let (logins, shortfall) = handshaken(url, credentials::Offered::for_url(Some(&token), url));

    assert_eq!(logins.len(), 1, "one login, after the first refusal");
    assert_eq!(logins[0].username, "x-access-token");
    assert_eq!(logins[0].password, token.reveal());
    // Turned down, which is the server's answer to the token and no reason
    // to ask for one.
    assert_eq!(shortfall, Some(Shortfall::Refused));
    let error = fetch_failed(url, shortfall, &"not accepted by the remote");
    assert!(matches!(error, GitError::Fetch { .. }), "{error}");
}

#[test]
fn without_a_token_the_handshake_logs_in_with_nothing_and_says_the_credential_is_missing() {
    let url = "https://example.com/acme/private.git";

    let (logins, shortfall) = handshaken(url, None);

    assert_eq!(logins, []);
    assert_eq!(shortfall, Some(Shortfall::Missing));
    assert_eq!(
        fetch_failed(url, shortfall, &"no credential").code(),
        "git_credentials_needed"
    );
}

#[test]
fn a_token_the_cloned_url_would_carry_in_the_clear_is_never_handed_to_the_transport() {
    let token = Token::new(crate::mother::random::hex(40));
    for url in [
        "http://example.com/acme/private.git",
        "ssh://git@example.com/acme/private.git",
    ] {
        let (logins, _) = handshaken(url, credentials::Offered::for_url(Some(&token), url));

        assert!(
            logins.iter().all(|login| login.password != token.reveal()),
            "{url}"
        );
    }
}

#[test]
fn a_clone_a_token_was_handed_to_keeps_nothing_of_it_whatever_came_of_the_fetch() {
    let server = Guarded::listening();
    let token = Token::new(crate::mother::random::hex(40));
    let cache = tempfile::TempDir::new().unwrap();

    let error = clone(
        &server.url(),
        Wanted::Branch(None),
        &cache.path().join("clone"),
        &Watch::new(),
        &Login {
            offered: credentials::Offered::for_url(
                Some(&token),
                "https://example.com/acme/private.git",
            ),
            helpers: a_prompt_that_panics,
            overrides: &[],
        },
    )
    .unwrap_err();

    for said in [error.to_string(), format!("{error:?}")] {
        assert!(!said.contains(token.reveal()), "{said}");
    }
    assert_eq!(holding(cache.path(), token.reveal()), Vec::<PathBuf>::new());
}

#[test]
fn without_a_token_a_server_asking_for_a_login_is_a_missing_credential_and_nothing_prompts() {
    let server = Guarded::listening();
    let cache = tempfile::TempDir::new().unwrap();
    let url = server.url();

    let error = clone(
        &url,
        Wanted::Branch(None),
        &cache.path().join("clone"),
        &Watch::new(),
        &Login {
            offered: None,
            helpers: a_prompt_that_panics,
            overrides: &[],
        },
    )
    .unwrap_err();

    assert_eq!(server.sent(), [None]);
    assert!(matches!(error, GitError::Credentials { .. }), "{error}");
    assert_eq!(error.code(), "git_credentials_needed");
    assert_eq!(
        error.to_string(),
        format!(
            "{url}: needs credentials, and gathering never prompts for them: configure a git \
             credential helper, use ssh-agent for an ssh URL, or hand it a token with --token-env \
             or --gh"
        )
    );
}

#[test]
fn a_token_given_for_one_host_is_never_sent_to_another_a_fetch_reaches() {
    let server = Guarded::listening();
    let token = Token::new(crate::mother::random::hex(40));
    let cache = tempfile::TempDir::new().unwrap();

    // As `url.<base>.insteadOf` would have it: the token was handed over for
    // the host typed, and the fetch went to another.
    let error = clone(
        &server.url(),
        Wanted::Branch(None),
        &cache.path().join("clone"),
        &Watch::new(),
        &Login {
            offered: credentials::Offered::for_url(
                Some(&token),
                "https://example.com/acme/private.git",
            ),
            helpers: a_prompt_that_panics,
            overrides: &[],
        },
    )
    .unwrap_err();

    assert_eq!(server.sent(), [None]);
    assert!(matches!(error, GitError::Credentials { .. }), "{error}");
}
