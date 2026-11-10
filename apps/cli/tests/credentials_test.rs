//! No command says a credential typed into a URL, nor keeps one: in what it
//! prints, in what its `--json` prints, in what it refuses with, and in the
//! files it writes. Every way a credential is typed into a URL is tried
//! (`UrlMother::every_credentialed`), so a change that handles one form and
//! not another fails here, whichever command it reaches.

mod common;

use common::mother::OlderBuildMother;
use common::*;
use mindweaver_core::mother::{random, Credentialed, GitRepositoryMother, UrlMother};

/// The refusal a gather of a URL with a credential in it ends with, after
/// the URL without it.
const REFUSED: &str = "a password, a token or, over http(s), a user name in the URL is refused, \
     because gathering stores and shows the URL: gather it without one, and hand the token over \
     with --token-env or --gh, or configure a git credential helper that has it";

/// Everything a command said, whether it ran or not: its report and every
/// line beside it, or why it stopped and what it had noticed first.
fn said(result: &Result<Outcome, Failure>) -> String {
    match result {
        Ok(outcome) => {
            let mut text = outcome.stdout.clone();
            for line in &outcome.stderr {
                text.push('\n');
                text.push_str(&line.text);
            }
            text
        }
        Err(failure) => {
            let mut text = failure.error.to_string();
            for line in &failure.warnings {
                text.push('\n');
                text.push_str(&line.text);
            }
            text
        }
    }
}

/// That `text` holds none of `url`'s credential, naming the form and what
/// was said when it does.
fn holds_no_secret(text: &str, url: &Credentialed, what: &str) {
    assert!(
        !text.contains(&url.secret),
        "{what}, for {:?}, says its secret:\n{text}",
        url.typed
    );
}

#[test]
fn no_way_of_typing_a_credential_into_a_url_to_gather_is_said_or_kept() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();

    for url in UrlMother::every_credentialed() {
        // A branch and a folder now and then, which reach the request the
        // refusal is made of.
        let mut args = vec![String::from("gather"), String::from("git")];
        if random::chance() {
            args.extend([String::from("--ref"), random::word()]);
        }
        if random::chance() {
            args.extend([String::from("--path"), random::word()]);
        }
        args.push(url.typed.clone());
        let args: Vec<&str> = args.iter().map(String::as_str).collect();

        let failure = weaver(dir.path(), &args).unwrap_err();

        assert_eq!(failure.error.code(), "credentials_in_url", "{args:?}");
        assert_eq!(
            failure.error.to_string(),
            format!("{}: {REFUSED}", url.clean),
            "{args:?}"
        );
        assert!(failure.warnings.is_empty(), "{:?}", failure.warnings);
        let mind_args: Vec<&str> = std::iter::once("weaver").chain(args.clone()).collect();
        let long_way = mind(dir.path(), &mind_args);
        assert_eq!(
            said(&long_way),
            said(&Err(failure)),
            "`mind weaver` is `mindweaver`"
        );
        holds_no_secret(&OlderBuildMother::kept(dir.path()), &url, "the workspace");
    }

    let listed = weaver(dir.path(), &["gather", "list"]).unwrap();
    assert!(
        listed.stdout.starts_with("nothing gathered yet"),
        "{}",
        listed.stdout
    );
}

#[test]
fn a_url_typed_where_a_branch_goes_is_said_and_kept_without_its_credential() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let source = GitRepositoryMother::with_skills(&["deploy"]);
    let address = GitRepositoryMother::url(&source);

    for url in UrlMother::every_credentialed() {
        let ran = weaver(
            dir.path(),
            &[
                "gather",
                "git",
                "--ref",
                url.typed.as_str(),
                address.as_str(),
            ],
        );

        // Refused as a URL typed where the URL goes is, before anything is
        // registered, rather than stored as the branch it was typed as.
        let failure = ran.as_ref().unwrap_err();
        assert_eq!(
            failure.error.code(),
            "credentials_in_url",
            "{:?}",
            url.typed
        );
        assert_eq!(
            failure.error.to_string(),
            format!("{}: {REFUSED}", url.clean)
        );
        holds_no_secret(&said(&ran), &url, "the refusal");
        let history = weaver(dir.path(), &["gather", "list", "--json"]);
        holds_no_secret(&said(&history), &url, "the gather list");
        holds_no_secret(&OlderBuildMother::kept(dir.path()), &url, "the workspace");
    }
}

/// What a terminal shows, through the binaries' own reporting, which an
/// in-process run never reaches.
#[test]
fn the_binaries_say_no_credential_typed_into_a_url_to_gather() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();

    for url in UrlMother::every_credentialed() {
        for (binary, args) in [
            ("mindweaver", vec!["gather", "git", url.typed.as_str()]),
            ("mind", vec!["weaver", "gather", "git", url.typed.as_str()]),
        ] {
            let ran = spawn(binary, dir.path(), &args, &[]);

            assert!(!ran.ok, "{binary} {args:?}");
            assert_eq!(ran.stdout, "", "{binary} {args:?}");
            assert_eq!(
                ran.stderr,
                format!("error: {}: {REFUSED}\n", url.clean),
                "{binary} {args:?}"
            );
        }
    }
}

/// Words a URL is quoted in, as a clone's error quotes one, typed where a
/// URL goes: refused as one with a credential, in the same words without
/// it.
#[test]
fn words_quoting_a_url_with_a_credential_are_said_without_it() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();

    for url in UrlMother::every_credentialed() {
        let quoted = UrlMother::quoted(&url);

        let failure = weaver(dir.path(), &["gather", "git", &quoted.typed]).unwrap_err();

        assert_eq!(failure.error.code(), "credentials_in_url", "{quoted:?}");
        assert_eq!(
            failure.error.to_string(),
            format!("{}: {REFUSED}", quoted.clean),
            "{quoted:?}"
        );
        holds_no_secret(
            &OlderBuildMother::kept(dir.path()),
            &quoted,
            "the workspace",
        );
    }
}

/// `--from` names a source by its URL, and one typed with its credential is
/// named back without it, at either level, as the words quoting one are.
#[test]
fn an_origin_typed_with_a_credential_is_said_without_it() {
    let (dir, _workspace, _ledger) = shelved(&["alpha"], &["deploy"]);
    let alpha = dir.path().join("alpha");

    for url in UrlMother::every_credentialed() {
        for from in [url.clone(), UrlMother::quoted(&url)] {
            let at_the_workspace = weaver(
                dir.path(),
                &["install", "-p", "alpha", "deploy", "--from", &from.typed],
            );
            let in_the_project = mind(&alpha, &["install", "deploy", "--from", &from.typed]);

            for (level, result) in [("mindweaver", at_the_workspace), ("mind", in_the_project)] {
                let failure = result.unwrap_err();
                assert_eq!(
                    failure.error.to_string(),
                    format!(
                        "nothing called `deploy` can be installed from {}: gather it, load a \
                         workspace that has it with `mindweaver load`, or add it to the workspace \
                         with `mindweaver add`",
                        from.clean
                    ),
                    "{level} install --from {:?}",
                    from.typed
                );
                holds_no_secret(&said(&Err(failure)), &from, level);
            }
        }
    }
    for copy in copies(&alpha, "deploy") {
        assert!(!copy.exists(), "{}", copy.display());
    }
}

/// Every command that reads what was gathered or installed, with `--json`
/// and without, at either level.
fn readings(dir: &Path) -> Vec<(String, Result<Outcome, Failure>)> {
    let alpha = dir.join("alpha");
    let mut read = Vec::new();
    for args in [
        &["gather", "list"][..],
        &["gather", "list", "--json"],
        &["list"],
        &["list", "--json"],
        &["status"],
        &["status", "--json"],
        &["show", "skill:deploy"],
        &["show", "skill:deploy", "--json"],
        &["validate"],
        &["validate", "--json"],
    ] {
        read.push((format!("mindweaver {args:?}"), weaver(dir, args)));
    }
    for args in [
        &["list", "--json"][..],
        &["status"],
        &["status", "--json"],
        &["show", "deploy", "--json"],
    ] {
        read.push((format!("mind {args:?}"), mind(&alpha, args)));
    }
    read
}

/// What a Mindweaver from before the refusal stored is redacted on its way
/// out of every read, whichever form it was typed in, and a read leaves it
/// where it is, as a read writes nothing.
#[test]
fn a_credential_an_older_build_stored_is_never_read_out() {
    let (dir, _workspace, _ledger) = shelved(&["alpha"], &["deploy"]);
    weaver(dir.path(), &["install", "-p", "alpha", "deploy"]).unwrap();

    for url in UrlMother::every_credentialed() {
        OlderBuildMother::stored(dir.path(), &url.typed);

        for (command, result) in readings(dir.path()) {
            holds_no_secret(&said(&result), &url, &command);
        }
        let shelf = weaver(dir.path(), &["gather", "list", "--json"]).unwrap();
        assert!(
            shelf.stdout.contains(&url.clean),
            "the shelf lists its source as {:?}:\n{}",
            url.clean,
            shelf.stdout
        );
        assert!(
            OlderBuildMother::kept(dir.path()).contains(&url.secret),
            "a read took {:?} out",
            url.typed
        );
    }
}

/// The first command that writes takes every form out of the ledger and the
/// clone, says so naming the URL without it, and leaves nothing of it.
#[test]
fn a_command_that_writes_takes_out_every_form_of_credential_an_older_build_stored() {
    let other = repository(&[(
        "skills/review/SKILL.md",
        &skill_file("review", "Look again"),
    )]);
    let other = other.path().to_string_lossy().into_owned();

    for url in UrlMother::every_credentialed() {
        let (dir, _workspace, _ledger) = shelved(&["alpha"], &["deploy"]);
        weaver(dir.path(), &["install", "-p", "alpha", "deploy"]).unwrap();
        OlderBuildMother::stored(dir.path(), &url.typed);

        let result = weaver(dir.path(), &["gather", "git", &other]);

        holds_no_secret(&said(&result), &url, "the gather");
        let gathered = result.unwrap();
        assert!(gathered.ok, "{:?}: {:?}", url.typed, gathered.stderr);
        assert!(
            gathered
                .stderr
                .iter()
                .any(|line| line.text.starts_with(&format!(
                    "note: {}: the user name, password or token an older Mindweaver stored",
                    url.clean
                ))),
            "{:?}: {:?}",
            url.typed,
            gathered.stderr
        );
        holds_no_secret(
            &OlderBuildMother::kept(dir.path()),
            &url,
            "the workspace, once written",
        );
        for (command, result) in readings(dir.path()) {
            holds_no_secret(&said(&result), &url, &command);
        }
    }
}

#[cfg(feature = "ui")]
mod panel {
    //! The panel's backend, which shows a task's lines and runs them: what it
    //! says of a task carrying a credential, and what its reads say of one an
    //! older build stored.

    use super::*;
    use mindweaver_cli::backend::{PanelBackend, Query};
    use mindweaver_server::{Backend, Underway};
    use serde_json::{json, Value};

    fn backend(root: &Path) -> PanelBackend {
        PanelBackend::launch(root, &Lookup::default()).unwrap()
    }

    fn task(sent: Value) -> Task {
        serde_json::from_value(sent.clone()).unwrap_or_else(|error| panic!("{sent}: {error}"))
    }

    fn query(sent: Value) -> Query {
        serde_json::from_value(sent.clone()).unwrap_or_else(|error| panic!("{sent}: {error}"))
    }

    /// A gather from a source named with a credential is previewed without
    /// it, and refused by the gather as one typed at a terminal is, its run
    /// saying nothing of the credential either; so is an install from such
    /// a source.
    #[test]
    fn the_panel_says_no_credential_a_task_carries() {
        let (dir, _workspace, _ledger) = shelved(&["alpha"], &["deploy"]);
        let backend = backend(dir.path());
        let shelf = || {
            weaver(dir.path(), &["gather", "list", "--json"])
                .unwrap()
                .stdout
        };
        let before = shelf();

        for url in UrlMother::every_credentialed() {
            let gather = task(json!({
                "type": "gather",
                "url": url.typed,
                "folder": "skills",
                "reference": random::chance().then(random::word),
            }));
            let preview = Backend::preview(&backend, &gather).unwrap();
            let shown = serde_json::to_string(&preview).unwrap();
            holds_no_secret(&shown, &url, "the preview of a gather");
            assert!(shown.contains(&url.clean), "{:?}: {shown}", url.typed);
            assert!(preview.cancellable, "{:?}", url.typed);
            let ran = Backend::run(&backend, gather, &Underway::unfollowed()).unwrap();
            assert!(!ran.ok, "{:?}", url.typed);
            assert_eq!(
                ran.runs[0].error.as_ref().map(|error| error.code),
                Some("credentials_in_url"),
                "{:?}",
                url.typed
            );
            holds_no_secret(
                &serde_json::to_string(&ran).unwrap(),
                &url,
                "the run of a gather",
            );

            let install = task(json!({
                "type": "install",
                "projects": ["alpha"],
                "kind": "skill",
                "name": "deploy",
                "from": url.typed,
            }));
            let preview = Backend::preview(&backend, &install).unwrap();
            let shown = serde_json::to_string(&preview).unwrap();
            holds_no_secret(&shown, &url, "the preview of an install");
            assert!(shown.contains(&url.clean), "{shown}");
            let ran = Backend::run(&backend, install, &Underway::unfollowed()).unwrap();
            assert!(!ran.ok, "{:?}", url.typed);
            holds_no_secret(
                &serde_json::to_string(&ran).unwrap(),
                &url,
                "the run of an install",
            );
        }
        // Nothing was gathered, nor any source registered.
        assert_eq!(shelf(), before);
    }

    #[test]
    fn the_panel_reads_no_credential_an_older_build_stored() {
        let (dir, _workspace, _ledger) = shelved(&["alpha"], &["deploy"]);
        weaver(dir.path(), &["install", "-p", "alpha", "deploy"]).unwrap();
        let backend = backend(dir.path());

        for url in UrlMother::every_credentialed() {
            OlderBuildMother::stored(dir.path(), &url.typed);

            for sent in [
                json!({"type": "snapshot"}),
                json!({"type": "projects"}),
                json!({"type": "library"}),
                json!({"type": "project", "entry": "alpha"}),
                json!({"type": "updates"}),
            ] {
                let answer = Backend::query(&backend, query(sent.clone()));
                let text = match answer {
                    Ok(answer) => serde_json::to_string(&answer).unwrap(),
                    Err(refusal) => refusal.message,
                };
                holds_no_secret(&text, &url, &sent.to_string());
                // Held to something: the snapshot names the source, and
                // where the install came from, by the URL.
                if sent["type"] == "snapshot" {
                    assert!(text.contains(&url.clean), "{:?}: {text}", url.clean);
                }
            }
        }
    }
}
