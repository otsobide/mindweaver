//! Unit tests for `gather.rs`, kept beside it rather than inside it.
//!
//! The integration suite gathers from repositories; these hold the pieces
//! around a gather to their rules: a request as its builders leave it, a
//! source that shows no credential however it was typed, what a scrub
//! says once however often it was run, which folders are candidates, how a
//! path is filed in the ledger, and the codes and words of each failure.

use super::*;

use std::sync::Mutex;

use tempfile::TempDir;

use crate::mother::random;
use crate::mother::{SkillMother, SourceMother, UrlMother, WorkspaceMother};

const DRAWS: usize = 200;

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

/// A remote that is never to be reached, for a request that must be
/// refused before anything is fetched.
#[derive(Debug, Default)]
struct Unreached {
    asked: Mutex<usize>,
}

impl Remote for Unreached {
    fn fetch(
        &self,
        _url: &str,
        _reference: Option<&str>,
        _into: &Path,
        _watch: &Watch,
    ) -> Result<crate::port::Cloned, GitError> {
        *self.asked.lock().unwrap() += 1;
        Err(GitError::Cancelled)
    }

    fn scrub(&self, _clone: &Path) -> Result<bool, GitError> {
        *self.asked.lock().unwrap() += 1;
        Ok(false)
    }
}

fn scrub_failure(url: &str) -> ScrubFailure {
    ScrubFailure {
        url: Redacted::new(url),
        source: GitError::Cancelled,
    }
}

fn leftover(path: &str) -> Leftover {
    Leftover {
        path: PathBuf::from(path),
        source: io::Error::other("busy"),
    }
}

#[test]
fn a_request_for_a_git_source_gathers_skills_from_its_skills_folder_at_its_head() {
    let url = UrlMother::clean();
    let request = Request::git(url.clone());

    assert_eq!(
        request.source,
        Source::Git {
            url: url.clone(),
            reference: None,
        }
    );
    assert_eq!(request.subdirectory, DEFAULT_SUBDIRECTORY);
    assert_eq!(DEFAULT_SUBDIRECTORY, "skills");
    assert_eq!(request.kind, Kind::Skill);
    assert!(!request.watch.is_cancelled());
}

#[test]
fn a_request_takes_the_folder_and_the_branch_it_is_told() {
    for _ in 0..DRAWS {
        let (folder, branch) = (random::word(), random::word());

        let request = Request::git(UrlMother::https())
            .from_subdirectory(folder.clone())
            .at(Some(branch.clone()));

        assert_eq!(request.subdirectory, folder);
        assert_eq!(request.source.reference(), Some(branch.as_str()));
    }
}

/// `at(None)` is the remote's HEAD again, not the branch asked for before.
#[test]
fn a_request_at_no_branch_goes_back_to_the_remote_s_head() {
    let request = Request::git(UrlMother::https())
        .at(Some("main"))
        .at(None::<String>);

    assert_eq!(request.source.reference(), None);
}

/// A front end keeps a clone of the watch to cancel from wherever its keys
/// are read: the request's is that one, not a copy of its own.
#[test]
fn a_request_watched_by_a_watch_is_cancelled_by_cancelling_it() {
    let watch = Watch::new();
    let request = Request::git(UrlMother::https()).watched_by(watch.clone());

    watch.cancel();

    assert!(request.watch.is_cancelled());
}

#[test]
fn a_request_fetches_through_the_remote_it_is_handed() {
    let remote = Arc::new(Unreached::default());
    let request = Request::git(UrlMother::https()).through(remote.clone());

    let _ = request
        .remote
        .fetch("u", None, Path::new("/nowhere"), &request.watch);

    assert_eq!(*remote.asked.lock().unwrap(), 1);
}

#[test]
fn a_source_says_its_kind_url_and_branch_as_it_was_given() {
    for _ in 0..DRAWS {
        let typed = random::pick(&[UrlMother::clean(), UrlMother::credentialed().typed]);
        let branch = random::chance().then(random::word);
        let request = Request::git(typed.clone()).at(branch.clone());

        assert_eq!(request.source.kind(), SourceKind::Git);
        assert_eq!(request.source.url(), typed, "the URL as typed");
        assert_eq!(request.source.reference(), branch.as_deref());
    }
}

/// A request is built before a URL with a credential is refused, so what it
/// serializes to is held to having none whatever was typed.
#[test]
fn a_source_typed_with_any_credential_serializes_without_it() {
    for typed in UrlMother::every_credentialed() {
        let request = Request::git(typed.typed.clone()).at(Some("main"));

        let serialized = serde_json::to_string(&request.source).unwrap();

        assert!(!serialized.contains(&typed.secret), "{serialized}");
        assert_eq!(
            serialized,
            format!(
                r#"{{"kind":"git","url":{},"reference":"main"}}"#,
                serde_json::to_string(&typed.clean).unwrap()
            )
        );
    }
}

#[test]
fn a_source_at_the_remote_s_head_serializes_a_null_branch() {
    let url = UrlMother::https();

    assert_eq!(
        serde_json::to_string(&Request::git(url.clone()).source).unwrap(),
        format!(r#"{{"kind":"git","url":"{url}","reference":null}}"#)
    );
}

/// Refused before anything is registered or fetched, and with the URL said
/// without what it carried, which is the URL to gather instead.
#[test]
fn a_gather_from_any_credentialed_url_is_refused_before_the_remote_is_asked() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let ledger = Ledger::in_memory().unwrap();
    for typed in UrlMother::every_credentialed() {
        let remote = Arc::new(Unreached::default());
        let request = Request::git(typed.typed.clone()).through(remote.clone());

        let refused = gather(&workspace, &ledger, &request).unwrap_err();

        assert!(
            matches!(&refused, GatherError::CredentialsInUrl { url } if url == &typed.clean),
            "{typed:?} gave {refused:?}"
        );
        assert!(!refused.to_string().contains(&typed.secret), "{refused}");
        assert_eq!(*remote.asked.lock().unwrap(), 0, "{typed:?}");
    }
    assert!(ledger.sources().unwrap().is_empty());
    for entry in ledger.history(100).unwrap() {
        assert_eq!(entry.outcome, "failed");
        let said = serde_json::to_string(&entry).unwrap();
        for typed in UrlMother::every_credentialed() {
            assert!(!said.contains(&typed.secret), "{said}");
        }
    }
}

/// A URL typed where the branch or the folder goes would be kept as that
/// branch or folder, word for word, so it is refused as one typed where the
/// URL goes is, before anything is registered.
#[test]
fn a_credentialed_url_typed_as_the_branch_the_folder_or_the_name_is_refused_alike() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let ledger = Ledger::in_memory().unwrap();
    for typed in UrlMother::every_credentialed() {
        let remote = Arc::new(Unreached::default());
        let request = match random::between(0, 2) {
            0 => Request::git(UrlMother::https()).at(Some(typed.typed.as_str())),
            1 => Request::git(UrlMother::https()).from_subdirectory(typed.typed.clone()),
            _ => Request::git(UrlMother::https()).named(Some(typed.typed.as_str())),
        }
        .through(remote.clone());

        let refused = gather(&workspace, &ledger, &request).unwrap_err();

        assert!(
            matches!(&refused, GatherError::CredentialsInUrl { url } if url == &typed.clean),
            "{typed:?} gave {refused:?}"
        );
        assert_eq!(*remote.asked.lock().unwrap(), 0, "{typed:?}");
    }
    assert!(ledger.sources().unwrap().is_empty());
    let history = serde_json::to_string(&ledger.history(100).unwrap()).unwrap();
    for typed in UrlMother::every_credentialed() {
        assert!(!history.contains(&typed.secret), "{history}");
    }
}

#[test]
fn a_gather_of_rules_is_refused_and_fetches_nothing() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let ledger = Ledger::in_memory().unwrap();
    let remote = Arc::new(Unreached::default());
    let mut request = Request::git(UrlMother::https()).through(remote.clone());
    request.kind = Kind::Rule;

    let refused = gather(&workspace, &ledger, &request).unwrap_err();

    assert!(
        matches!(refused, GatherError::NotGatherable { kind: Kind::Rule }),
        "{refused:?}"
    );
    assert_eq!(refused.to_string(), "rules cannot be gathered yet");
    assert_eq!(*remote.asked.lock().unwrap(), 0);
    assert!(ledger.sources().unwrap().is_empty());
}

#[test]
fn a_report_counts_everything_now_on_the_shelf_because_of_it() {
    let harvested = || Harvested {
        name: ArtifactName::from(random::kebab()),
        directory: random::word(),
        summary: None,
        path: PathBuf::from("/w"),
    };
    for _ in 0..50 {
        let (added, updated, unchanged) = (
            random::between(0, 4),
            random::between(0, 4),
            random::between(0, 4),
        );
        let report = Report {
            name: SourceMother::random().name,
            revision: None,
            directory: random::word(),
            added: (0..added).map(|_| harvested()).collect(),
            updated: (0..updated).map(|_| harvested()).collect(),
            unchanged: (0..unchanged).map(|_| harvested()).collect(),
            failures: vec![Failure::UnusableName {
                path: PathBuf::from("/w/x"),
            }],
            warnings: Vec::new(),
        };

        assert_eq!(report.total(), added + updated + unchanged);
        assert_eq!(report.is_empty(), added + updated + unchanged == 0);
    }
}

#[test]
fn a_scrub_that_took_nothing_out_is_empty() {
    assert!(Scrub::default().is_empty());
}

#[test]
fn a_scrub_that_took_anything_out_is_not_empty() {
    let nonempty = [
        Scrub {
            sources: vec![SourceMother::random()],
            ..Scrub::default()
        },
        Scrub {
            actions: 1,
            ..Scrub::default()
        },
        Scrub {
            failures: vec![scrub_failure("https://example.com/a.git")],
            ..Scrub::default()
        },
        Scrub {
            leftovers: vec![leftover("/w/.mindweaver/cache/a")],
            ..Scrub::default()
        },
        Scrub {
            merged: vec![Merged {
                url: crate::value::Redacted::new(&UrlMother::https()),
                reference: None,
                directory: String::from("a"),
                into: SourceMother::random(),
            }],
            ..Scrub::default()
        },
    ];
    for scrub in nonempty {
        assert!(!scrub.is_empty(), "{scrub:?}");
    }
}

/// Two branches of one URL are one credential to tell of, and a clone that
/// failed to be rewritten fails again at every scrub: each is said once.
#[test]
fn a_later_scrub_absorbed_tells_each_url_once_and_counts_every_entry() {
    let url = UrlMother::https();
    let other = UrlMother::https();
    let mut first = Scrub {
        sources: vec![SourceMother::create(&url, Some("main"))],
        actions: 2,
        failures: vec![scrub_failure(&url)],
        leftovers: vec![leftover("/w/a")],
        merged: Vec::new(),
    };
    let later = Scrub {
        sources: vec![
            SourceMother::create(&url, Some("dev")),
            SourceMother::create(&other, None),
        ],
        actions: 3,
        failures: vec![scrub_failure(&url), scrub_failure(&other)],
        leftovers: vec![leftover("/w/a"), leftover("/w/b")],
        merged: Vec::new(),
    };

    first.absorb(later);

    let urls: Vec<&str> = first
        .sources
        .iter()
        .map(|source| source.url.as_str())
        .collect();
    assert_eq!(urls, [url.as_str(), other.as_str()]);
    assert_eq!(
        first.sources[0].reference.as_deref(),
        Some("main"),
        "the first kept"
    );
    assert_eq!(first.actions, 5);
    let failed: Vec<&str> = first
        .failures
        .iter()
        .map(|failure| failure.url.as_str())
        .collect();
    assert_eq!(failed, [url.as_str(), other.as_str()]);
    assert_eq!(first.leftovers.len(), 3, "a folder left is said each time");
}

#[test]
fn absorbing_an_empty_scrub_changes_nothing() {
    let mut scrub = Scrub {
        sources: vec![SourceMother::random()],
        actions: random::between(0, 9),
        ..Scrub::default()
    };
    let (sources, actions) = (scrub.sources.clone(), scrub.actions);

    scrub.absorb(Scrub::default());

    assert_eq!(scrub.sources, sources);
    assert_eq!(scrub.actions, actions);
}

#[test]
fn a_scrub_of_a_ledger_with_no_credential_asks_the_remote_nothing() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let ledger = Ledger::in_memory().unwrap();
    let remote = Unreached::default();

    let scrubbed = scrub(&workspace, &ledger, Duration::ZERO, &remote).unwrap();

    assert!(scrubbed.is_empty(), "{scrubbed:?}");
    assert_eq!(*remote.asked.lock().unwrap(), 0);
}

#[test]
fn candidates_are_the_folders_holding_a_manifest_in_order() {
    let dir = TempDir::new().unwrap();
    let mut names: Vec<String> = (0..random::between(1, 6))
        .map(|_| random::kebab())
        .collect();
    names.sort();
    names.dedup();
    for name in &names {
        write(
            &dir.path().join(name).join("SKILL.md"),
            &SkillMother::create(name, "x"),
        );
    }
    fs::create_dir_all(dir.path().join("assets")).unwrap();
    write(&dir.path().join("README.md"), "not a skill");
    write(&dir.path().join("deep").join("inner").join("SKILL.md"), "x");

    let found = candidates(dir.path(), "SKILL.md").unwrap();

    let expected: Vec<PathBuf> = names.iter().map(|name| dir.path().join(name)).collect();
    assert_eq!(found, expected, "{names:?}");
}

/// The folder was checked to be there just before: one that cannot be
/// listed is a gather that cannot happen, unlike discovery's absence.
#[test]
fn candidates_in_a_folder_that_is_not_there_cannot_be_read() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("skills");

    let refused = candidates(&missing, "SKILL.md").unwrap_err();

    assert!(
        matches!(&refused, GatherError::Read { path, .. } if path == &missing),
        "{refused:?}"
    );
    assert_eq!(refused.code(), "gather_read_failed");
}

#[test]
fn a_path_in_the_workspace_is_filed_slash_separated_from_its_root() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let path = workspace
        .gathered_dir(Kind::Skill)
        .join("github.com-acme-skills")
        .join("deploy");

    assert_eq!(
        relative(&workspace, &path),
        format!(
            ".mindweaver/{}/github.com-acme-skills/deploy",
            Kind::Skill.folder()
        )
    );
}

#[test]
fn any_path_below_the_workspace_is_filed_by_its_route() {
    let (_dir, workspace) = WorkspaceMother::empty();
    for _ in 0..DRAWS {
        let segments: Vec<String> = (0..random::between(1, 4))
            .map(|_| random::kebab())
            .collect();
        let mut path = workspace.root().to_path_buf();
        for segment in &segments {
            path.push(segment);
        }

        assert_eq!(relative(&workspace, &path), segments.join("/"), "{path:?}");
    }
}

#[cfg(unix)]
#[test]
fn a_skill_whose_folder_is_not_utf_8_cannot_be_filed() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let dir = TempDir::new().unwrap();
    let folder = dir.path().join(OsStr::from_bytes(b"caf\xe9"));
    write(&folder.join("SKILL.md"), &SkillMother::create("cafe", "x"));
    let mut failures = Vec::new();

    let taken = read(&folder, dir.path(), &mut failures);

    assert!(taken.is_none());
    assert!(
        matches!(&failures[..], [Failure::UnusableName { path }] if path == &folder),
        "{failures:?}"
    );
    assert_eq!(failures[0].code(), "gathered_name_not_utf8");
}

/// The folder keeps the name the source gave it, whatever the skill
/// declares: the disagreement is `validate`'s to say.
#[test]
fn a_skill_is_filed_under_its_folder_s_name_whatever_it_declares() {
    let dir = TempDir::new().unwrap();
    let folder = dir.path().join("deploy-old");
    write(
        &folder.join("SKILL.md"),
        &SkillMother::create("deploy", "Ship it"),
    );
    let mut failures = Vec::new();

    let (path, filed, artifact) = read(&folder, dir.path(), &mut failures).unwrap();

    assert_eq!(path, folder);
    assert_eq!(filed, "deploy-old");
    assert_eq!(artifact.name().as_str(), "deploy");
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn a_skill_that_cannot_be_read_is_a_failure_and_not_taken() {
    let dir = TempDir::new().unwrap();
    let folder = dir.path().join("broken");
    write(&folder.join("SKILL.md"), "---\nname: [unclosed\n---\n");
    let mut failures = Vec::new();

    assert!(read(&folder, dir.path(), &mut failures).is_none());
    assert!(
        matches!(&failures[..], [Failure::Artifact(_)]),
        "{failures:?}"
    );
}

#[test]
fn each_gather_failure_names_its_path_and_gives_its_code() {
    let copy = Failure::Copy {
        path: PathBuf::from("/w/.mindweaver/skills/s/deploy"),
        source: io::Error::other("full"),
    };
    let unusable = Failure::UnusableName {
        path: PathBuf::from("/w/.mindweaver/cache/s/x"),
    };

    assert_eq!(copy.code(), "gathered_copy_failed");
    assert_eq!(copy.path(), Path::new("/w/.mindweaver/skills/s/deploy"));
    assert_eq!(
        copy.to_string(),
        "/w/.mindweaver/skills/s/deploy: cannot be copied into the workspace: full"
    );
    assert_eq!(unusable.path(), Path::new("/w/.mindweaver/cache/s/x"));
    assert_eq!(
        Failure::CODES,
        ["gathered_name_not_utf8", "gathered_copy_failed"]
    );
}

#[test]
fn a_warning_names_the_manifest_and_what_is_wrong_with_it() {
    let warning = Warning {
        path: PathBuf::from("/w/.mindweaver/skills/s/deploy/SKILL.md"),
        issue: ValidationIssue::MetadataNotAMap,
    };

    assert_eq!(
        warning.to_string(),
        format!(
            "/w/.mindweaver/skills/s/deploy/SKILL.md: {}",
            ValidationIssue::MetadataNotAMap
        )
    );
}

#[test]
fn each_gather_error_of_its_own_gives_its_own_code() {
    let errors = [
        (GatherError::Cancelled, "gather_cancelled"),
        (
            GatherError::CredentialsInUrl {
                url: UrlMother::https(),
            },
            "credentials_in_url",
        ),
        (
            GatherError::NoSuchFolder {
                subdirectory: String::from("skills"),
                url: UrlMother::https(),
            },
            "gather_no_such_folder",
        ),
        (
            GatherError::NameInvalid {
                name: String::from("acme/skills"),
            },
            "source_name_invalid",
        ),
        (
            GatherError::NameTaken {
                name: SourceMother::random().name,
                origin: SourceMother::random().origin(),
            },
            "source_name_taken",
        ),
        (
            GatherError::NotGatherable { kind: Kind::Rule },
            "kind_not_gatherable",
        ),
        (
            GatherError::Read {
                path: PathBuf::from("/w"),
                source: io::Error::other("denied"),
            },
            "gather_read_failed",
        ),
    ];
    let given: Vec<&str> = errors.iter().map(|(_, code)| *code).collect();
    for (error, code) in errors {
        assert_eq!(error.code(), code, "{error:?}");
    }
    assert_eq!(GatherError::CODES, given.as_slice());
}

/// A gather carrying another of core's failures says that failure's code,
/// so the panel tells a busy workspace or a refused fetch apart the same
/// way whatever met it.
#[test]
fn a_gather_error_carrying_another_of_core_s_gives_its_code() {
    let carried = [
        (
            GatherError::from(GitError::Fetch {
                url: UrlMother::https(),
                detail: String::from("refused"),
            }),
            "git_fetch_failed",
        ),
        (
            GatherError::from(LedgerError::Unmade {
                path: PathBuf::from("/w/.mindweaver/mindweaver.db"),
            }),
            "ledger_unmade",
        ),
        (
            GatherError::from(LockError::Busy {
                path: PathBuf::from("/w/.mindweaver/lock"),
                pid: None,
                waited: crate::lock::WAIT,
            }),
            "workspace_busy",
        ),
    ];
    for (error, code) in carried {
        assert_eq!(error.code(), code, "{error:?}");
        assert!(!GatherError::CODES.contains(&code), "{code}");
    }
}

#[test]
fn a_folder_the_source_does_not_have_is_named_with_the_url() {
    let url = UrlMother::https();
    let error = GatherError::NoSuchFolder {
        subdirectory: String::from("agents/skills"),
        url: url.clone(),
    };

    assert_eq!(
        error.to_string(),
        format!("{url}: has no `agents/skills` folder to gather from")
    );
}

#[test]
fn a_clone_that_kept_its_credential_is_said_by_its_url_without_it() {
    for typed in UrlMother::every_credentialed() {
        let failure = scrub_failure(&typed.typed);

        assert!(!failure.to_string().contains(&typed.secret), "{failure}");
        assert!(
            failure
                .to_string()
                .starts_with(&format!("{}: ", typed.clean)),
            "{failure}"
        );
        assert_eq!(failure.code(), "clone_unscrubbed");
    }
}

#[test]
fn a_folder_left_behind_is_said_with_why_and_its_code() {
    let left = leftover("/w/.mindweaver/cache/a");

    assert_eq!(
        left.to_string(),
        "/w/.mindweaver/cache/a: left by a source that is now one with another at the same URL, \
         and cannot be removed: busy"
    );
    assert_eq!(left.code(), "source_leftover");
}

/// A directory that is no one folder name was named by no ledger, and is
/// left rather than followed out of the workspace.
#[test]
fn the_folders_of_a_source_whose_directory_is_no_folder_name_are_left_alone() {
    let (dir, workspace) = WorkspaceMother::empty();
    let outside = dir.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    let mut leftovers = Vec::new();

    remove_folders_of(&workspace, "../../outside", &mut leftovers);

    assert!(outside.is_dir());
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn the_folders_of_a_source_that_went_are_removed_from_the_cache_and_every_shelf() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let gone = SourceMother::random();
    let folders: Vec<PathBuf> = std::iter::once(workspace.cache_dir())
        .chain(Kind::ALL.iter().map(|kind| workspace.gathered_dir(*kind)))
        .map(|folder| folder.join(&gone.directory))
        .collect();
    for folder in &folders {
        write(&folder.join("x"), "x");
    }
    let mut leftovers = Vec::new();

    remove_folders_of(&workspace, &gone.directory, &mut leftovers);

    for folder in &folders {
        assert!(!folder.exists(), "{folder:?}");
    }
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn removing_the_folders_of_a_source_that_has_none_is_no_failure() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let mut leftovers = Vec::new();

    remove_folders_of(
        &workspace,
        &SourceMother::random().directory,
        &mut leftovers,
    );

    assert!(leftovers.is_empty(), "{leftovers:?}");
}
