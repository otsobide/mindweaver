//! Core's ports driven by adapters of a test's own: a gather fetching with no
//! git at all, and a ledger stamping by a clock that does not move. What each
//! holds is that the rules behind a port do not depend on which adapter is
//! on its other side, which is what makes one adapter replaceable by another.

mod common;

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use common::{skill, workspace};
use mindweaver_core::gather::{self, GatherError, Request, Watch};
use mindweaver_core::ledger::{Action, Ledger, Outcome};
use mindweaver_core::port::{Clock, Cloned, GitError, Remote, Token};
use mindweaver_core::settings::ProjectId;
use mindweaver_core::value::Revision;
use mindweaver_core::Kind;

/// A remote that fetches by writing the files it was made with, and notes
/// what it was asked for.
#[derive(Debug, Default)]
struct Written {
    files: Vec<(&'static str, String)>,
    asked: Mutex<Vec<(String, Option<String>)>>,
}

impl Remote for Written {
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
        self.asked
            .lock()
            .unwrap()
            .push((url.to_owned(), reference.map(str::to_owned)));
        let _ = fs::remove_dir_all(into);
        for (path, contents) in &self.files {
            let file = into.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, contents).unwrap();
        }
        Ok(Cloned {
            root: into.to_path_buf(),
            revision: Some(Revision::from("0123abc")),
        })
    }

    fn scrub(&self, _clone: &Path) -> Result<bool, GitError> {
        Ok(false)
    }
}

/// A clock stopped at one moment.
#[derive(Debug)]
struct Stopped(i64);

impl Clock for Stopped {
    fn now(&self) -> i64 {
        self.0
    }
}

#[test]
fn a_gather_fetches_through_the_remote_it_is_handed_and_shelves_as_it_always_does() {
    let remote = Arc::new(Written {
        files: vec![
            ("skills/deploy/SKILL.md", skill("deploy", "Ship it")),
            ("skills/review/SKILL.md", skill("review", "Read it")),
            ("README.md", String::from("not a skill")),
        ],
        ..Written::default()
    });
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request = Request::git("https://example.com/acme/skills.git")
        .at(Some("main"))
        .through(remote.clone());

    let report = gather::gather(&workspace, &ledger, &request).unwrap();

    assert_eq!(
        *remote.asked.lock().unwrap(),
        [(
            String::from("https://example.com/acme/skills.git"),
            Some(String::from("main"))
        )]
    );
    let added: Vec<&str> = report
        .added
        .iter()
        .map(|harvested| harvested.name.as_str())
        .collect();
    assert_eq!(added, ["deploy", "review"]);
    assert_eq!(report.revision.as_deref(), Some("0123abc"));
    let shelf = workspace.gathered_dir(Kind::Skill).join(&report.directory);
    assert!(shelf.join("deploy/SKILL.md").is_file());
    assert!(!shelf.join("README.md").exists());
    // What the ledger records of it is what it records of a git clone.
    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 2);
    assert_eq!(
        gathered[0].source.origin(),
        "https://example.com/acme/skills.git#main"
    );
}

#[test]
fn a_gather_still_refuses_a_credential_before_any_remote_is_asked() {
    let remote = Arc::new(Written::default());
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let request =
        Request::git("https://me:ghp_t0ken@example.com/acme/skills.git").through(remote.clone());

    let error = gather::gather(&workspace, &ledger, &request).unwrap_err();

    assert!(
        matches!(error, GatherError::CredentialsInUrl { .. }),
        "{error:?}"
    );
    assert!(remote.asked.lock().unwrap().is_empty());
}

#[test]
fn the_ledger_stamps_what_it_writes_by_the_clock_it_is_handed() {
    let ledger = Ledger::in_memory()
        .unwrap()
        .with_clock(Arc::new(Stopped(1_790_000_000)));

    ledger
        .log(Action::Gather, Some("deploy"), Outcome::Ok, None)
        .unwrap();

    let history = ledger.history(1).unwrap();
    assert_eq!(history[0].at, 1_790_000_000);
}

#[test]
fn a_relink_runs_a_write_of_any_type_inside_the_stores_move() {
    let id = ProjectId::parse("0123456789abcdef0123456789abcdef").unwrap();
    let ledger = Ledger::in_memory().unwrap();
    ledger
        .knew(&[(id.clone(), String::from("old"))], &[])
        .unwrap();

    let written = ledger
        .relinked("old", "new", &id, || Ok::<_, ()>("marker"))
        .unwrap();
    let refused = ledger
        .relinked("new", "newer", &id, || Err::<(), _>("refused"))
        .unwrap();

    assert_eq!(written, Ok(("marker", 0)));
    assert_eq!(refused, Err("refused"));
    assert_eq!(ledger.project_at("new").unwrap(), Some(id));
    assert_eq!(ledger.project_at("newer").unwrap(), None);
}
