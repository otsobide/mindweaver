//! `mindweaver gather`: taking skills from a git repository onto the shelf.

mod common;

use std::sync::{Arc, Mutex};

use common::*;

// ---------------------------------------------------------------------------
// gather
//
// The repository is built here rather than fetched, for the reason the core
// suite builds one: a test that needs the network fails for reasons that have
// nothing to do with the code.
// ---------------------------------------------------------------------------

#[test]
fn gather_reports_the_source_and_what_it_took() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it")),
        (
            "skills/commit-style/SKILL.md",
            &skill_file("commit-style", "How we commit"),
        ),
    ]);
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();

    let outcome = weaver(
        dir.path(),
        &["gather", "git", &source.path().to_string_lossy()],
    )
    .unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    assert!(outcome.stdout.contains("added"), "{}", outcome.stdout);
    assert!(
        outcome.stdout.contains("commit-style  How we commit"),
        "{}",
        outcome.stdout
    );
    assert!(
        outcome
            .stdout
            .contains("2 skills: 2 added, 0 updated, 0 unchanged"),
        "{}",
        outcome.stdout
    );
}

#[test]
fn gathering_again_says_nothing_moved() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();

    weaver(dir.path(), &["gather", "git", &url]).unwrap();
    let again = weaver(dir.path(), &["gather", "git", &url]).unwrap();

    assert!(
        again
            .stdout
            .contains("1 skill: 0 added, 0 updated, 1 unchanged"),
        "{}",
        again.stdout
    );
}

#[test]
fn a_skill_that_cannot_be_read_is_a_warning_and_a_non_zero_exit() {
    let source = repository(&[
        ("skills/good/SKILL.md", &skill_file("good", "Fine")),
        ("skills/broken/SKILL.md", "no front matter\n"),
    ]);
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();

    let outcome = weaver(
        dir.path(),
        &["gather", "git", &source.path().to_string_lossy()],
    )
    .unwrap();

    assert!(outcome.stdout.contains("good"));
    assert_eq!(outcome.stderr.len(), 1);
    assert!(outcome.stderr[0].text.contains("front matter"));
    assert!(!outcome.ok);
}

#[test]
fn a_skill_with_nested_metadata_is_still_listed_with_a_warning() {
    let source = repository(&[(
        "skills/nested/SKILL.md",
        "---\nname: nested\ndescription: Nested metadata\nmetadata:\n  owner:\n    team: ops\n---\n",
    )]);
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();

    let outcome = weaver(dir.path(), &["gather", "git", &url]).unwrap();

    assert!(
        outcome.stdout.contains("added  nested  Nested metadata"),
        "{}",
        outcome.stdout
    );
    assert_eq!(outcome.stderr.len(), 1, "{:?}", outcome.stderr);
    assert!(outcome.stderr[0].text.starts_with("warning: "));
    assert!(
        outcome.stderr[0].text.ends_with(
            "SKILL.md: `metadata` entry `owner` is not a string, so it is ignored, and left in the file as written"
        ),
        "{}",
        outcome.stderr[0]
    );
    assert!(!outcome.ok);
    let listed = weaver(dir.path(), &["gather", "list"]).unwrap();
    assert!(listed.stdout.contains("nested"), "{}", listed.stdout);
}

#[test]
fn validate_reports_a_gathered_skill_without_hiding_it() {
    let source = repository(&[
        ("skills/a--b/SKILL.md", &skill_file("a--b", "Doubled")),
        ("skills/fine/SKILL.md", &skill_file("fine", "Nothing wrong")),
    ]);
    let dir = workspace_with_two();
    let url = source.path().to_string_lossy().into_owned();
    let gathered = weaver(dir.path(), &["gather", "git", &url]).unwrap();
    assert!(
        gathered.stdout.contains("added  a--b"),
        "{}",
        gathered.stdout
    );

    let outcome = weaver(dir.path(), &["validate"]).unwrap();

    let shelf = fs::read_dir(dir.path().join(WEAVER_DIR).join("skills"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name()
        .into_string()
        .unwrap();
    assert_eq!(
        outcome.stdout,
        format!(
            "a--b (shelf/{shelf}): 1 problem\n  \
             - `a--b` is not usable as a name: it has to be lowercase letters and digits, joined by single hyphens\n\
             alpha (alpha): ok\n\
             beta (beta): ok\n\
             fine (shelf/{shelf}): ok\n\
             \n\
             4 skills checked, 1 invalid\n"
        )
    );
    assert!(outcome.stderr.is_empty(), "{:?}", outcome.stderr);
    assert!(!outcome.ok);
    // One project's report is that project's, and the shelf is nobody's.
    let one = weaver(dir.path(), &["validate", "-p", "alpha"]).unwrap();
    assert_eq!(one.stdout, "alpha: ok\n\n1 skill checked, 0 invalid\n");
}

#[test]
fn a_gathered_name_cannot_put_a_carriage_return_on_the_terminal() {
    // A source writes the name and the metadata keys, and a carriage return
    // in either would let a warning or a report line overwrite itself with a
    // line that looks like a verdict.
    let source = repository(&[(
        "skills/evil/SKILL.md",
        "---\nname: \"evil\\rfine (shelf/x): ok\"\ndescription: Evil\nmetadata:\n  \"k\\rok\": [1]\n---\n",
    )]);
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();

    let gathered = weaver(dir.path(), &["gather", "git", &url]).unwrap();
    let validated = weaver(dir.path(), &["validate"]).unwrap();

    assert!(!gathered.stderr.is_empty());
    let said = gathered.stderr.iter().map(|line| &line.text);
    for line in said.chain([&validated.stdout]) {
        assert!(!line.contains('\r'), "{line:?}");
    }
    assert!(
        validated
            .stdout
            .starts_with("evil fine (shelf/x): ok: 3 problems"),
        "{}",
        validated.stdout
    );
}

#[test]
fn gather_list_names_where_each_skill_came_from() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();
    weaver(dir.path(), &["gather", "git", &url, "--name", "acme"]).unwrap();

    let outcome = weaver(dir.path(), &["gather", "list"]).unwrap();

    // By the name `--from` takes, which the listing is for.
    assert!(outcome.ok);
    assert_eq!(outcome.stdout, "deploy  acme  Ship it\n");
}

#[test]
fn gather_says_the_name_of_its_source_and_name_calls_it_what_it_is_given() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let other = repository(&[("skills/review/SKILL.md", &skill_file("review", "Look"))]);
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();

    let named = weaver(dir.path(), &["gather", "git", &url, "--name", "team"]).unwrap();
    let renamed = weaver(dir.path(), &["gather", "git", &url, "--name", "team.v2"]).unwrap();
    let taken = weaver(
        dir.path(),
        &[
            "gather",
            "git",
            &other.path().to_string_lossy(),
            "--name",
            "team.v2",
        ],
    )
    .unwrap_err();
    let unusable = weaver(dir.path(), &["gather", "git", &url, "--name", "team/v3"]).unwrap_err();

    assert!(
        named.stdout.starts_with(&format!("team: {url} at ")),
        "{}",
        named.stdout
    );
    assert!(
        renamed.stdout.starts_with(&format!("team.v2: {url} at ")),
        "{}",
        renamed.stdout
    );
    assert_eq!(taken.error.code(), "source_name_taken");
    assert_eq!(
        taken.error.to_string(),
        format!(
            "`team.v2` is the name of the source at {url} already: gather this one under \
             another `--name`"
        )
    );
    assert_eq!(unusable.error.code(), "source_name_invalid");
    assert_eq!(
        unusable.error.to_string(),
        "`team/v3` cannot name a source: it has to be letters, digits, `.`, `_` and `-`, not \
         starting with `-`, and neither `.`, `..` nor `workspace`"
    );
}

#[test]
fn an_empty_shelf_says_how_to_fill_it() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();

    let outcome = weaver(dir.path(), &["gather", "list"]).unwrap();

    assert!(outcome.ok);
    assert!(outcome.stdout.starts_with("nothing gathered yet"));
    assert!(outcome.stdout.contains("mindweaver gather git"));
}

#[test]
fn listing_a_shelf_that_has_no_ledger_makes_none() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let ledger = dir.path().join(WEAVER_DIR).join("mindweaver.db");

    let outcome = weaver(dir.path(), &["gather", "list"]).unwrap();

    assert!(outcome.stdout.starts_with("nothing gathered yet"));
    assert!(!ledger.exists());
}

#[test]
fn gathering_outside_a_workspace_says_what_to_run() {
    let dir = TempDir::new().unwrap();

    let failure = weaver(dir.path(), &["gather", "list"]).unwrap_err();

    assert!(matches!(*failure.error, CliError::NotInWorkspace(_)));
    assert!(failure.error.to_string().contains("mindweaver init"));
}

#[test]
fn gather_is_reachable_the_long_way_round_too() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();

    let short = weaver(dir.path(), &["gather", "git", &url]).unwrap();
    let long = mind(dir.path(), &["weaver", "gather", "list"]).unwrap();

    assert!(short.ok);
    assert!(long.stdout.contains("deploy"), "{}", long.stdout);
}

#[test]
fn gather_list_tells_two_branches_of_one_repository_apart() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();

    // The same URL at its default branch and at a named one: two sources,
    // and two names, the second numbered.
    let head = fs::read_to_string(source.path().join(".git").join("HEAD")).unwrap();
    let branch = head.trim().rsplit('/').next().unwrap().to_owned();
    weaver(dir.path(), &["gather", "git", &url, "--name", "acme"]).unwrap();
    weaver(dir.path(), &["gather", "git", &url, "--ref", &branch]).unwrap();

    let outcome = weaver(dir.path(), &["gather", "list"]).unwrap();

    assert_eq!(outcome.stdout.lines().count(), 2, "{}", outcome.stdout);
    let names: Vec<&str> = outcome
        .stdout
        .lines()
        .map(|line| line.split_whitespace().nth(1).unwrap())
        .collect();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"acme"), "{}", outcome.stdout);
    assert_ne!(names[0], names[1], "{}", outcome.stdout);
}

#[test]
fn a_watched_gather_reports_its_steps_and_prints_what_an_unwatched_one_does() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let url = source.path().to_string_lossy().into_owned();
    let (watched, plain) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    weaver(watched.path(), &["init"]).unwrap();
    weaver(plain.path(), &["init"]).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let watch = Watch::new().reporting_to({
        let seen = Arc::clone(&seen);
        move |progress| seen.lock().unwrap().push(progress)
    });

    let outcome = run_weaver_cli_watched(
        &parsed_weaver(watched.path(), &["gather", "git", &url]),
        &watch,
    )
    .unwrap();

    let seen = seen.lock().unwrap();
    assert_eq!(seen.first(), Some(&Progress::Connecting), "{seen:?}");
    assert_eq!(seen.last(), Some(&Progress::Placing { done: 1, total: 1 }));
    // The progress is the front end's to show; the report is the command's.
    assert_eq!(
        outcome,
        weaver(plain.path(), &["gather", "git", &url]).unwrap()
    );
}

#[test]
fn a_cancelled_gather_says_so_and_leaves_the_shelf_as_it_was() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();
    let watch = Watch::new();
    watch.cancel();

    // The long way round as well, which is `mind`'s run path.
    let failure = run_watched(
        &parsed_mind(dir.path(), &["weaver", "gather", "git", &url]),
        &watch,
    )
    .unwrap_err();

    assert_eq!(
        failure.error.to_string(),
        "cancelled, with nothing put on the shelf"
    );
    let shelf = weaver(dir.path(), &["gather", "list"]).unwrap();
    assert!(
        shelf.stdout.starts_with("nothing gathered yet"),
        "{}",
        shelf.stdout
    );
}

// ---------------------------------------------------------------------------
// credentials an older build stored
// ---------------------------------------------------------------------------

/// Leave in the workspace at `dir` what a Mindweaver from before the refusal
/// left after gathering its one source from `url`: the URL in the source, in
/// the log, and in the clone's config and reflog.
fn left_by_an_older_build(dir: &Path, url: &str) {
    let workspace = WeaverWorkspace::open(dir).unwrap();
    let database = rusqlite::Connection::open(workspace.ledger_path()).unwrap();
    database
        .execute("UPDATE sources SET url = ?1", [url])
        .unwrap();
    database
        .execute(
            "INSERT INTO actions (at, action, target, outcome, detail)
             VALUES (0, 'gather', ?1, 'ok', '1 added, 0 updated, 0 unchanged, 0 failed')",
            [url],
        )
        .unwrap();

    for clone in fs::read_dir(workspace.cache_dir()).unwrap() {
        let git = clone.unwrap().path().join(".git");
        let config: String = fs::read_to_string(git.join("config"))
            .unwrap()
            .lines()
            .map(|line| match line.trim_start().strip_prefix("url = ") {
                Some(_) => format!("\turl = {url}\n"),
                None => format!("{line}\n"),
            })
            .collect();
        fs::write(git.join("config"), config).unwrap();
        let mut reflog = fs::read_to_string(git.join("logs").join("HEAD")).unwrap();
        reflog.push_str(&format!(
            "0000 1111 Fixture <fixture@example.com> 0 +0000\tclone: from {url}\n"
        ));
        fs::write(git.join("logs").join("HEAD"), reflog).unwrap();
    }
}

/// Everything the workspace at `dir` keeps that could hold a URL: its
/// database, byte for byte, and every text file in its cache.
fn kept_by(dir: &Path) -> String {
    fn text_under(root: &Path, into: &mut String) {
        // No cache yet is nothing kept in one.
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                text_under(&path, into);
            } else if let Ok(text) = fs::read_to_string(&path) {
                into.push_str(&text);
            }
        }
    }
    let workspace = WeaverWorkspace::open(dir).unwrap();
    let mut kept =
        String::from_utf8_lossy(&fs::read(workspace.ledger_path()).unwrap()).into_owned();
    text_under(&workspace.cache_dir(), &mut kept);
    kept
}

#[test]
fn a_token_an_older_build_stored_is_never_listed_and_a_command_that_writes_takes_it_out_once() {
    let first = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let second = repository(&[(
        "skills/review/SKILL.md",
        &skill_file("review", "Look again"),
    )]);
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    weaver(
        dir.path(),
        &[
            "gather",
            "git",
            &first.path().to_string_lossy(),
            "--name",
            "acme",
        ],
    )
    .unwrap();
    left_by_an_older_build(dir.path(), "https://me:ghp_t0ken@example.com/acme/skills");

    // Listing reads, so it rewrites nothing, and it shows nothing it should
    // not, the URL its JSON holds included.
    let listed = weaver(dir.path(), &["gather", "list"]).unwrap();
    assert_eq!(listed.stdout, "deploy  acme  Ship it\n");
    assert!(listed.stderr.is_empty(), "{:?}", listed.stderr);
    let json = weaver(dir.path(), &["gather", "list", "--json"]).unwrap();
    assert!(
        json.stdout.contains("\"https://example.com/acme/skills\""),
        "{}",
        json.stdout
    );
    assert!(!json.stdout.contains("ghp_t0ken"), "{}", json.stdout);
    assert!(kept_by(dir.path()).contains("ghp_t0ken"));

    // The next command that writes takes it out of the ledger and the clone,
    // and says so.
    let second = second.path().to_string_lossy().into_owned();
    let gathered = weaver(dir.path(), &["gather", "git", &second]).unwrap();
    assert!(gathered.ok, "{:?}", gathered.stderr);
    assert_eq!(
        gathered.stderr,
        [
            "note: https://example.com/acme/skills: the user name, password or token an older \
             Mindweaver stored in this URL is taken out; gathering it again needs a git \
             credential helper that has it",
            "note: 1 action log entry: the user name, password or token an older Mindweaver \
             stored in a URL in it is taken out",
        ]
    );
    assert!(!kept_by(dir.path()).contains("ghp_t0ken"));
    // Compared word by word: the second source is named after a temp dir,
    // whose length sets how wide the name column is padded.
    let listed = weaver(dir.path(), &["gather", "list"]).unwrap();
    assert!(
        listed.stdout.lines().any(|line| {
            line.split_whitespace().collect::<Vec<_>>() == ["deploy", "acme", "Ship", "it"]
        }),
        "{}",
        listed.stdout
    );

    // Said once: there is nothing left to say the second time.
    let again = weaver(dir.path(), &["gather", "git", &second]).unwrap();
    assert!(again.stderr.is_empty(), "{:?}", again.stderr);
}

/// What schema 6 wrote, as core's migration tests have it: one repository
/// gathered by two spellings of its URL, and another the same way, once
/// with a token an older build stored, `<token>` there.
const VERSION_6: &str = include_str!("../../core/tests/fixtures/ledger-v6.sql");

#[test]
fn the_first_command_that_writes_after_the_step_says_which_sources_are_one_now() {
    let token = format!("ghp_{}", "T0k3n".repeat(7) + "x");
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    rusqlite::Connection::open(dir.path().join(WEAVER_DIR).join("mindweaver.db"))
        .unwrap()
        .execute_batch(&VERSION_6.replace("<token>", &token))
        .unwrap();

    let gathered = weaver(
        dir.path(),
        &["gather", "git", &source.path().to_string_lossy()],
    )
    .unwrap();

    assert!(gathered.ok, "{:?}", gathered.stderr);
    assert_eq!(
        gathered.stderr,
        [
            "note: https://github.com/Acme/Private.git: the same repository as the source \
             `acme-private` (https://github.com/acme/private), so the two are one source now, \
             and the shelf keeps what `acme-private` gathered",
            "note: https://github.com/acme/skills: the same repository as the source \
             `acme-skills` (git@github.com:acme/skills.git), so the two are one source now, and \
             the shelf keeps what `acme-skills` gathered",
            "note: https://github.com/acme/private: the user name, password or token an older \
             Mindweaver stored in this URL is taken out; gathering it again needs a git \
             credential helper that has it",
            "note: 2 action log entries: the user names, passwords or tokens an older \
             Mindweaver stored in URLs in them are taken out",
        ]
    );
    assert!(!kept_by(dir.path()).contains(&token));
    // Said once: there is nothing left to say the second time.
    let again = weaver(
        dir.path(),
        &["gather", "git", &source.path().to_string_lossy()],
    )
    .unwrap();
    assert!(again.stderr.is_empty(), "{:?}", again.stderr);
}

#[test]
fn a_read_that_takes_the_step_leaves_saying_what_it_made_one_to_the_first_command_that_writes() {
    let token = format!("ghp_{}", "T0k3n".repeat(7) + "x");
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    rusqlite::Connection::open(dir.path().join(WEAVER_DIR).join("mindweaver.db"))
        .unwrap()
        .execute_batch(&VERSION_6.replace("<token>", &token))
        .unwrap();

    let listed = weaver(dir.path(), &["gather", "list"]).unwrap();
    let gathered = weaver(
        dir.path(),
        &["gather", "git", &source.path().to_string_lossy()],
    )
    .unwrap();

    assert!(listed.stderr.is_empty(), "{:?}", listed.stderr);
    let notes: Vec<&str> = gathered
        .stderr
        .iter()
        .map(|line| line.text.as_str())
        .filter(|line| line.contains("one source now"))
        .collect();
    assert_eq!(notes.len(), 2, "{:?}", gathered.stderr);
}

#[test]
fn a_url_with_a_token_in_it_is_refused_and_nothing_kept_or_said_repeats_the_token() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();

    let failure = weaver(
        dir.path(),
        &[
            "gather",
            "git",
            "https://me:ghp_t0ken@example.com/acme/skills",
        ],
    )
    .unwrap_err();

    assert_eq!(
        failure.error.to_string(),
        "https://example.com/acme/skills: a password, a token or, over http(s), a user name in \
         the URL is refused, because gathering stores and shows the URL: gather it without one, \
         and configure a git credential helper that has it"
    );
    assert!(failure.warnings.is_empty(), "{:?}", failure.warnings);
    assert!(!kept_by(dir.path()).contains("ghp_t0ken"));
    let listed = weaver(dir.path(), &["gather", "list"]).unwrap();
    assert!(
        listed.stdout.starts_with("nothing gathered yet"),
        "{}",
        listed.stdout
    );
}
