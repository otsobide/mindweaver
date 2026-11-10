//! The TUI, driven key by key through the state machine and drawn into an
//! in-memory terminal.

mod common;

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use common::*;
use mindweaver_core::{Agent, Share};
use tui::state::Step as TuiStep;

// ---------------------------------------------------------------------------
// The TUI
//
// Driven the way the install screen is: keys pressed at the state machine,
// the work it asks for done by `tui::step` exactly as the loop does it, and
// the screen drawn into an in-memory terminal. Only the loop reading a real
// keyboard is left untested, and it holds nothing but the loop.
// ---------------------------------------------------------------------------

#[test]
fn weaver_on_its_own_is_the_tui_at_both_spellings() {
    let bare = WeaverCli::try_parse_from(["mindweaver"]).unwrap();
    let named = WeaverCli::try_parse_from(["mindweaver", "tui"]).unwrap();
    let long = Cli::try_parse_from(["mind", "weaver"]).unwrap();

    assert!(bare.command.is_none());
    assert!(matches!(
        named.command,
        Some(mindweaver_cli::WeaverCommand::Tui)
    ));
    assert!(matches!(
        long.command,
        Some(mindweaver_cli::Command::Weaver { command: None })
    ));
}

#[test]
fn the_tui_opens_on_every_linked_project_and_what_it_holds() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());

    let drawn = tui_drawn(&app);
    assert!(drawn.contains("Projects (2)"), "{drawn}");
    assert!(drawn.contains("alpha"), "{drawn}");
    assert!(drawn.contains("beta"), "{drawn}");
    assert!(
        drawn.contains("A skill"),
        "the first project's skill:\n{drawn}"
    );

    press_key(&mut app, KeyCode::Down);
    assert_eq!(app.member().unwrap().name, "beta");
}

#[test]
fn without_a_workspace_the_tui_offers_to_create_one_here() {
    let dir = TempDir::new().unwrap();
    let mut app = tui_in(dir.path());
    assert!(matches!(app.view, View::Nowhere));
    assert!(tui_drawn(&app).contains("No weaver workspace here"));

    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(task, Some(Task::InitWorkspace));
    assert!(dir.path().join(WEAVER_DIR).join(WEAVER_CONFIG).is_file());
    assert!(matches!(app.view, View::Workspace(_)));
    assert!(app.transcript().starts_with("$ mindweaver init\n"));
}

#[test]
fn a_new_workspace_goes_straight_to_linking_what_is_under_it() {
    let dir = TempDir::new().unwrap();
    let alpha = dir.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    mind(&alpha, &["init"]).unwrap();
    let mut app = tui_in(dir.path());

    press_and_run(&mut app, KeyCode::Enter);

    assert!(matches!(app.mode, Mode::Linking(_)), "{:?}", app.mode);
    assert!(tui_drawn(&app).contains("alpha"));
    let task = press_and_run(&mut app, KeyCode::Enter);
    assert_eq!(
        task,
        Some(Task::Link {
            path: String::from("alpha")
        })
    );
    assert!(registry(dir.path()).contains("\"alpha\""));
    // Nothing is left to link, so it is back on the projects.
    assert!(matches!(app.mode, Mode::Browse));
    assert_eq!(app.member().unwrap().name, "alpha");
}

#[test]
fn a_repository_is_made_a_mind_project_only_when_that_is_confirmed() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    fs::create_dir_all(dir.path().join("gamma/.git")).unwrap();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('l'));
    assert_eq!(press_key(&mut app, KeyCode::Enter), tui::state::Step::Stay);
    assert!(matches!(app.mode, Mode::Confirming(_)));
    assert!(tui_drawn(&app).contains("mind -C gamma init && mindweaver link gamma"));

    // No: back to the form, and nothing written.
    press_key(&mut app, KeyCode::Char('n'));
    assert!(matches!(app.mode, Mode::Linking(_)));
    assert!(!dir.path().join("gamma").join(MIND_DIR).exists());

    press_key(&mut app, KeyCode::Enter);
    let task = press_and_run(&mut app, KeyCode::Char('y'));

    assert_eq!(
        task,
        Some(Task::InitAndLink {
            path: String::from("gamma")
        })
    );
    // What `mind init` makes anywhere: settings kept out of git.
    assert_eq!(
        names_in(&dir.path().join("gamma").join(MIND_DIR)),
        [".gitignore", SETTINGS_FILE]
    );
    assert!(registry(dir.path()).contains("\"gamma\""));
    let transcript = app.transcript();
    assert!(
        transcript.contains("$ mind -C gamma init\n"),
        "{transcript}"
    );
    assert!(
        transcript.contains("$ mindweaver link gamma\n"),
        "{transcript}"
    );
}

#[test]
fn a_typed_path_that_is_not_a_project_is_asked_about_too() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    // Not a repository either, so the workspace could not have offered it.
    fs::create_dir(dir.path().join("plain")).unwrap();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('l'));
    type_text(&mut app, "plain");
    press_and_run(&mut app, KeyCode::Enter);

    assert!(matches!(app.mode, Mode::Confirming(_)), "{:?}", app.mode);
    assert!(
        registry(dir.path()).contains("projects = []"),
        "nothing yet"
    );
    press_and_run(&mut app, KeyCode::Char('y'));
    assert!(registry(dir.path()).contains("\"plain\""));
}

#[test]
fn m_opens_the_minds_screen_with_the_linked_ones_ticked() {
    let dir = workspace_with_candidates();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('m'));

    let Mode::Minds(screen) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    let rows: Vec<(&str, bool)> = screen
        .rows
        .iter()
        .map(|row| (row.path.as_str(), row.ticked))
        .collect();
    // What is not linked first, as `mindweaver scan --suggest` lists it: a
    // project before a repository.
    assert_eq!(
        rows,
        [
            ("gamma", false),
            ("repo", false),
            ("alpha", true),
            ("beta", true)
        ]
    );
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("[x] alpha"), "{drawn}");
    assert!(drawn.contains("[ ] repo"), "{drawn}");
    assert!(drawn.contains("mind project with an id"), "{drawn}");
    assert!(
        drawn.contains("git repository (mind init first)"),
        "{drawn}"
    );
}

#[test]
fn applying_the_minds_screen_links_and_unlinks_in_one_pass() {
    let dir = workspace_with_candidates();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Char('m'));

    // Tick gamma and repo, untick alpha.
    press_key(&mut app, KeyCode::Char(' '));
    press_key(&mut app, KeyCode::Down);
    press_key(&mut app, KeyCode::Char(' '));
    press_key(&mut app, KeyCode::Down);
    press_key(&mut app, KeyCode::Char(' '));
    assert!(tui_drawn(&app).contains("- unlink"));
    press_key(&mut app, KeyCode::Char('a'));

    // Asked first, naming the repository `mind init` will write into.
    assert!(matches!(app.mode, Mode::Confirming(_)), "{:?}", app.mode);
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("Link 2 and unlink 1?"), "{drawn}");
    assert!(drawn.contains("repo is not a mind project yet"), "{drawn}");
    let Mode::Confirming(confirm) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    assert!(
        confirm
            .question
            .iter()
            .any(|line| line.contains("(a .mind kept out of git, and empty skills/ and rules/)")),
        "{:?}",
        confirm.question
    );
    assert!(!registry(dir.path()).contains("\"gamma\""), "nothing yet");

    let task = press_and_run(&mut app, KeyCode::Char('y'));

    assert_eq!(
        task,
        Some(Task::Minds {
            init: vec![String::from("repo")],
            link: vec![String::from("gamma"), String::from("repo")],
            unlink: vec![String::from("alpha")],
        })
    );
    let registry = registry(dir.path());
    assert!(registry.contains("\"gamma\""), "{registry}");
    assert!(registry.contains("\"repo\""), "{registry}");
    assert!(registry.contains("\"beta\""), "{registry}");
    assert!(!registry.contains("\"alpha\""), "{registry}");
    // What `mind init` makes anywhere: settings kept out of git.
    assert_eq!(
        names_in(&dir.path().join("repo").join(MIND_DIR)),
        [".gitignore", SETTINGS_FILE]
    );
    let transcript = app.transcript();
    assert!(transcript.contains("$ mind -C repo init\n"), "{transcript}");
    assert!(
        transcript.contains("$ mindweaver link gamma repo\n"),
        "{transcript}"
    );
    assert!(
        transcript.contains("$ mindweaver unlink alpha\n"),
        "{transcript}"
    );
}

/// `workspace_with_candidates`, with one more of each reason not linked:
/// `older`, a project a `mind.toml` marks, and `notes`, a repository
/// holding an `AGENTS.md`.
fn workspace_with_one_of_each() -> TempDir {
    let dir = workspace_with_candidates();
    let older = dir.path().join("older");
    fs::create_dir(&older).unwrap();
    older_project(&older);
    fs::create_dir_all(dir.path().join("notes/.git")).unwrap();
    fs::write(dir.path().join("notes/AGENTS.md"), "# Agents\n").unwrap();
    dir
}

#[test]
fn the_minds_screen_lists_what_weaver_scan_suggest_does_in_its_order_and_words() {
    let dir = workspace_with_one_of_each();
    let app = tui::minds(dir.path(), &[], &Lookup::default()).unwrap();

    let suggested = weaver(dir.path(), &["scan", "--suggest"]).unwrap();

    // Each line of the command as the route and what follows its standing.
    let lines: Vec<(String, String)> = suggested
        .stdout
        .lines()
        .take_while(|line| !line.is_empty())
        .map(|line| {
            let (route, rest) = line.split_once(' ').unwrap();
            let rest = rest.trim_start();
            let why = rest
                .strip_prefix("not linked")
                .map_or("", str::trim_start)
                .to_owned();
            (route.to_owned(), why)
        })
        .collect();
    let Mode::Minds(screen) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    let rows: Vec<(String, String)> = screen
        .rows
        .iter()
        .map(|row| (row.path.clone(), row.why.clone()))
        .collect();
    assert_eq!(rows, lines);
    assert_eq!(
        rows.iter()
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>(),
        ["gamma", "older", "notes", "repo", "alpha", "beta"]
    );
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("mind project without an id"), "{drawn}");
    assert!(
        drawn.contains("git repository with AGENTS.md (mind init first)"),
        "{drawn}"
    );
}

#[test]
fn the_link_form_offers_what_is_not_linked_in_the_order_and_words_of_a_suggestion() {
    let dir = workspace_with_one_of_each();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('l'));

    let routes: Vec<&str> = app
        .workspace()
        .unwrap()
        .unlinked
        .iter()
        .map(|unlinked| unlinked.route.as_str())
        .collect();
    assert_eq!(routes, ["gamma", "older", "notes", "repo"]);
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("gamma  mind project with an id"), "{drawn}");
    assert!(
        drawn.contains("notes  git repository with AGENTS.md (mind init first)"),
        "{drawn}"
    );
    // The first offered is the one picked: enter links the project.
    let task = press_and_run(&mut app, KeyCode::Enter);
    assert_eq!(
        task,
        Some(Task::Link {
            path: String::from("gamma")
        })
    );
}

#[test]
fn the_minds_screen_cuts_a_long_reason_to_keep_what_applying_does_in_sight() {
    let dir = workspace_with_two();
    let repo = dir.path().join("github.com/org/repo");
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::create_dir(repo.join(".claude")).unwrap();
    fs::create_dir(repo.join(".codex")).unwrap();
    for file in ["AGENTS.md", "CLAUDE.md", ".mcp.json", "opencode.json"] {
        fs::write(repo.join(file), "").unwrap();
    }
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Char('m'));
    // Tick the repository and untick alpha, the rows under it.
    press_key(&mut app, KeyCode::Char(' '));
    press_key(&mut app, KeyCode::Down);
    press_key(&mut app, KeyCode::Char(' '));

    let reason = "git repository with .claude, .codex, .mcp.json, AGENTS.md, CLAUDE.md, \
                  opencode.json (mind init first)";
    for width in [80, 100, 160] {
        let mut terminal = Terminal::new(TestBackend::new(width, 30)).unwrap();
        terminal.draw(|frame| tui::ui::draw(&app, frame)).unwrap();
        let lines: Vec<String> = terminal
            .backend()
            .buffer()
            .content()
            .chunks(usize::from(width))
            .map(|line| line.iter().map(|cell| cell.symbol()).collect())
            .collect();
        let row = |path: &str| {
            lines
                .iter()
                .find(|line| line.contains(path))
                .unwrap_or_else(|| panic!("{width}: no {path} in {lines:#?}"))
        };
        let repo = row("[x] github.com/org/repo");
        assert!(repo.contains("+ mind init, then link"), "{width}: {repo}");
        let alpha = row("[ ] alpha");
        assert!(alpha.contains("- unlink"), "{width}: {alpha}");
        // Cut where the pane has no room for all of it, and said whole
        // where it does.
        assert_eq!(
            repo.contains(reason),
            !repo.contains('…'),
            "{width}: {repo}"
        );
    }
    let mut terminal = Terminal::new(TestBackend::new(160, 30)).unwrap();
    terminal.draw(|frame| tui::ui::draw(&app, frame)).unwrap();
    let drawn: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(drawn.contains(reason), "{drawn}");
}

#[test]
fn the_minds_screen_and_the_link_form_say_a_moved_project_moved_as_weaver_scan_suggest_does() {
    let dir = workspace_with_own();
    weaver(dir.path(), &["install", "-p", "alpha", "commit-style"]).unwrap();
    fs::create_dir(dir.path().join("apps")).unwrap();
    fs::rename(dir.path().join("alpha"), dir.path().join("apps/alpha")).unwrap();

    let suggested = weaver(dir.path(), &["scan", "--suggest"]).unwrap();

    // Each line of the command not linked, as its route and what follows
    // its standing.
    let not_linked: Vec<(String, String)> = suggested
        .stdout
        .lines()
        .take_while(|line| !line.is_empty())
        .filter_map(|line| {
            let (route, rest) = line.split_once(' ')?;
            let why = rest.trim_start().strip_prefix("not linked")?;
            Some((route.to_owned(), why.trim_start().to_owned()))
        })
        .collect();
    assert_eq!(
        not_linked,
        [(
            String::from("apps/alpha"),
            String::from("moved: alpha -> apps/alpha")
        )]
    );
    // Its reason is its id, which ranks it first, but what it wants is a
    // relink, so the screen says it moved as the command does, rather than
    // offering it as the project most worth linking.
    let app = tui::minds(dir.path(), &[], &Lookup::default()).unwrap();
    let Mode::Minds(screen) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    let found: Vec<(String, String)> = screen
        .rows
        .iter()
        .filter(|row| matches!(row.standing, tui::minds::Standing::Found { .. }))
        .map(|row| (row.path.clone(), row.why.clone()))
        .collect();
    assert_eq!(found, not_linked);
    let drawn = tui_drawn(&app);
    assert!(
        drawn.contains("[ ] apps/alpha  moved: alpha -> apps/alpha"),
        "{drawn}"
    );

    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Char('l'));
    let drawn = tui_drawn(&app);
    assert!(
        drawn.contains("apps/alpha  moved: alpha -> apps/alpha"),
        "{drawn}"
    );
}

#[test]
fn leaving_the_minds_screen_applies_nothing() {
    let dir = workspace_with_candidates();
    let before = registry(dir.path());
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Char('m'));
    press_key(&mut app, KeyCode::Char(' '));

    press_key(&mut app, KeyCode::Esc);

    assert!(matches!(app.mode, Mode::Browse), "{:?}", app.mode);
    assert_eq!(registry(dir.path()), before);
}

#[test]
fn applying_an_untouched_minds_screen_asks_nothing() {
    let dir = workspace_with_candidates();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Char('m'));

    press_key(&mut app, KeyCode::Char('a'));

    assert!(matches!(app.mode, Mode::Minds(_)), "{:?}", app.mode);
}

#[test]
fn weaver_minds_is_a_command_at_both_spellings() {
    let direct = WeaverCli::try_parse_from(["mindweaver", "minds"]).unwrap();
    let nested = Cli::try_parse_from(["mind", "weaver", "minds", "work"]).unwrap();

    assert!(matches!(
        direct.command,
        Some(mindweaver_cli::WeaverCommand::Minds { ref roots }) if roots.is_empty()
    ));
    assert!(matches!(
        nested.command,
        Some(mindweaver_cli::Command::Weaver {
            command: Some(mindweaver_cli::WeaverCommand::Minds { ref roots })
        }) if roots == &[PathBuf::from("work")]
    ));
}

/// `workspace_with_candidates`, and beside it `work`, a folder holding
/// `acme/delta`, a mind project, and `plain`, a git repository: what
/// `mindweaver minds <work>` offers.
fn candidates_and_a_folder_beside() -> (TempDir, TempDir) {
    let dir = workspace_with_candidates();
    let work = TempDir::new().unwrap();
    let delta = work.path().join("acme/delta");
    fs::create_dir_all(&delta).unwrap();
    new_project(&delta);
    fs::create_dir_all(work.path().join("plain/.git")).unwrap();
    (dir, work)
}

#[test]
fn weaver_minds_given_a_folder_offers_what_is_under_it_beside_every_linked_one() {
    let (dir, work) = candidates_and_a_folder_beside();

    let app = tui::minds(dir.path(), &[work.path().to_path_buf()], &Lookup::default()).unwrap();

    // By path, since the lines run from the workspace root, outside the
    // folder; and not `gamma` or `repo`, under the workspace it was not
    // told to look under.
    let delta = work.path().join("acme/delta").display().to_string();
    let plain = work.path().join("plain").display().to_string();
    assert_eq!(
        minds_rows(&app),
        vec![delta.as_str(), plain.as_str(), "alpha", "beta"]
    );
    let drawn = tui_drawn(&app);
    assert!(
        drawn.contains(&format!("found under {}", work.path().display())),
        "{drawn}"
    );
}

#[test]
fn weaver_minds_given_a_folder_that_is_not_there_says_so_beside_what_the_others_hold() {
    let (dir, work) = candidates_and_a_folder_beside();
    let missing = work.path().join("wrok");

    let app = tui::minds(
        dir.path(),
        &[missing.clone(), work.path().join("plain")],
        &Lookup::default(),
    )
    .unwrap();

    let plain = work.path().join("plain").display().to_string();
    assert_eq!(minds_rows(&app), vec![plain.as_str(), "alpha", "beta"]);
    // In the words `mindweaver scan` warns with.
    let said = &app.workspace().unwrap().scan_warnings;
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].starts_with(&format!("{}: cannot be listed: ", missing.display())),
        "{said:?}"
    );
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("What the scan missed"), "{drawn}");
    assert!(drawn.contains("cannot be listed"), "{drawn}");
}

#[test]
fn weaver_minds_given_only_a_folder_that_is_not_there_is_not_an_empty_answer() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();

    let app = tui::minds(dir.path(), &[dir.path().join("wrok")], &Lookup::default()).unwrap();

    assert!(minds_rows(&app).is_empty());
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("nothing linked"), "{drawn}");
    assert!(drawn.contains("cannot be listed"), "{drawn}");
}

#[test]
fn applying_the_minds_screen_given_a_folder_links_by_path_and_looks_there_again() {
    let (dir, work) = candidates_and_a_folder_beside();
    let mut app = tui::minds(dir.path(), &[work.path().to_path_buf()], &Lookup::default()).unwrap();
    let delta = work.path().join("acme/delta");
    let spelled = delta.display().to_string();

    press_key(&mut app, KeyCode::Char(' '));
    press_key(&mut app, KeyCode::Char('a'));
    let task = press_and_run(&mut app, KeyCode::Char('y'));

    assert_eq!(
        task,
        Some(Task::Minds {
            init: Vec::new(),
            link: vec![spelled.clone()],
            unlink: Vec::new(),
        })
    );
    assert!(WeaverWorkspace::open(dir.path()).unwrap().is_linked(&delta));
    let transcript = app.transcript();
    assert!(
        transcript.contains(&format!("$ mindweaver link {spelled}\n")),
        "{transcript}"
    );
    // Read again under the same folder: `plain` is still offered, and
    // `gamma`, under the workspace, still is not.
    press_key(&mut app, KeyCode::Esc);
    press_key(&mut app, KeyCode::Char('m'));
    let rows = minds_rows(&app);
    let plain = work.path().join("plain").display().to_string();
    assert!(rows.contains(&plain.as_str()), "{rows:?}");
    assert!(!rows.contains(&"gamma"), "{rows:?}");
    assert_eq!(rows.len(), 4, "{rows:?}");
}

#[test]
fn adding_in_the_tui_is_mind_add() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('a'));
    type_text(&mut app, "deploy");
    press_key(&mut app, KeyCode::Tab);
    type_text(&mut app, "Ship the service");
    // The form says what it is about to run, as it would be typed.
    assert!(tui_drawn(&app).contains("mind -C alpha add skill deploy 'Ship the service'"));
    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(
        task,
        Some(Task::Add {
            holder: Holder::Project(String::from("alpha")),
            kind: Kind::Skill,
            name: String::from("deploy"),
            description: String::from("Ship the service"),
            template: None,
            server: None,
        })
    );
    assert!(dir.path().join("alpha/skills/deploy/SKILL.md").is_file());
    // And the cursor is on what was just made.
    assert_eq!(app.current_item().unwrap().name, "deploy");
    assert_eq!(app.focus, tui::state::Focus::Items);
    assert!(app
        .transcript()
        .contains("$ mind -C alpha add skill deploy 'Ship the service'\ncreated skill deploy at "));
}

#[test]
fn switching_the_kind_adds_a_rule_instead() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('a'));
    // Up past the template line to the kind, and back down to the name.
    press_key(&mut app, KeyCode::Up);
    press_key(&mut app, KeyCode::Up);
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Down);
    press_key(&mut app, KeyCode::Down);
    type_text(&mut app, "git/no-force-push");
    press_key(&mut app, KeyCode::Down);
    type_text(&mut app, "Never force-push");
    press_and_run(&mut app, KeyCode::Enter);

    assert!(dir
        .path()
        .join("alpha/rules/git/no-force-push.md")
        .is_file());
}

#[test]
fn a_rejected_name_keeps_the_form_open_with_the_reason_in_it() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('a'));
    type_text(&mut app, "Bad Name");
    press_key(&mut app, KeyCode::Tab);
    type_text(&mut app, "Whatever");
    press_and_run(&mut app, KeyCode::Enter);

    let Mode::Adding(form) = &app.mode else {
        panic!("the form closed: {:?}", app.mode);
    };
    assert!(form.error.as_deref().unwrap().contains("`Bad Name`"));
    assert_eq!(form.name.text(), "Bad Name", "what was typed is kept");
    assert!(!dir.path().join("alpha/skills/Bad Name").exists());
}

#[test]
fn unlinking_asks_first_and_forgets_without_deleting() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('U'));
    assert!(tui_drawn(&app).contains("Unlink alpha?"));
    let task = press_and_run(&mut app, KeyCode::Char('y'));

    assert_eq!(
        task,
        Some(Task::Unlink {
            entry: String::from("alpha")
        })
    );
    assert!(!registry(dir.path()).contains("\"alpha\""));
    assert!(dir.path().join("alpha").join(MIND_DIR).is_dir());
    assert_eq!(app.workspace().unwrap().projects(), 1);
}

#[test]
fn u_updates_the_project_or_the_artifact_under_the_cursor_and_shows_what_it_said() {
    let dir = workspace_with_own();
    weaver(dir.path(), &["install", "-p", "alpha", "commit-style"]).unwrap();
    weaver(dir.path(), &["install", "-p", "beta", "commit-style"]).unwrap();
    let manifest = own_directory_for(dir.path(), Kind::Skill).join("commit-style/SKILL.md");
    let since = fs::read_to_string(&manifest).unwrap() + "Since.\n";
    fs::write(&manifest, &since).unwrap();
    let mut app = tui_in(dir.path());

    let project = press_and_run(&mut app, KeyCode::Char('u'));
    let read = tui_drawn(&app);
    press_key(&mut app, KeyCode::Esc);
    press_key(&mut app, KeyCode::Down);
    press_key(&mut app, KeyCode::Right);
    // Past beta's own skill, `beta`, to what it was given.
    press_key(&mut app, KeyCode::Down);
    let artifact = press_and_run(&mut app, KeyCode::Char('u'));

    assert_eq!(
        project,
        Some(Task::Update {
            holder: Holder::Project(String::from("alpha")),
            reference: None,
            force: false,
        })
    );
    assert_eq!(project.unwrap().describe(), "mind -C alpha update --yes");
    assert!(
        read.contains("updated commit-style in alpha, from workspace"),
        "{read}"
    );
    let artifact = artifact.unwrap();
    assert_eq!(
        artifact,
        Task::Update {
            holder: Holder::Project(String::from("beta")),
            reference: Some(String::from("skill:commit-style")),
            force: false,
        }
    );
    assert_eq!(
        artifact.describe(),
        "mind -C beta update --yes skill:commit-style"
    );
    for project in ["alpha", "beta"] {
        for copy in copies(&dir.path().join(project), "commit-style") {
            assert_eq!(fs::read_to_string(copy.join("SKILL.md")).unwrap(), since);
        }
    }
    assert!(app
        .transcript()
        .contains("$ mind -C alpha update --yes\nupdated commit-style in alpha, from workspace\n"));
}

#[test]
fn u_on_the_workspaces_own_row_updates_every_project_it_links() {
    let dir = workspace_with_own();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Up);

    let Some(task) = press_and_run(&mut app, KeyCode::Char('u')) else {
        panic!("u runs a command");
    };

    assert_eq!(task.describe(), "mindweaver update --all --yes");
    assert!(
        tui_drawn(&app).contains("nothing to update in alpha and beta"),
        "{}",
        tui_drawn(&app)
    );
}

#[test]
fn u_on_a_project_that_cannot_be_opened_says_so_and_runs_nothing() {
    let dir = workspace_with_two();
    fs::remove_dir_all(dir.path().join("alpha")).unwrap();
    let mut app = tui_in(dir.path());

    assert_eq!(
        press_key(&mut app, KeyCode::Char('u')),
        tui::state::Step::Stay
    );
    assert!(
        tui_drawn(&app).contains("alpha cannot be opened, so nothing in it can be updated"),
        "{}",
        tui_drawn(&app)
    );
}

#[test]
fn a_stale_entry_is_shown_where_it_can_be_unlinked() {
    let dir = workspace_with_two();
    fs::remove_dir_all(dir.path().join("alpha")).unwrap();
    let mut app = tui_in(dir.path());

    let drawn = tui_drawn(&app);
    assert!(drawn.contains("! alpha"), "{drawn}");
    assert!(drawn.contains("cannot be opened"), "{drawn}");

    press_key(&mut app, KeyCode::Char('U'));
    press_and_run(&mut app, KeyCode::Char('y'));

    assert!(!registry(dir.path()).contains("\"alpha\""));
}

#[test]
fn a_stale_entry_the_scan_found_moved_is_relinked_where_it_went() {
    let dir = workspace_with_own();
    weaver(dir.path(), &["install", "-p", "alpha", "commit-style"]).unwrap();
    fs::create_dir(dir.path().join("apps")).unwrap();
    fs::rename(dir.path().join("alpha"), dir.path().join("apps/alpha")).unwrap();
    let mut app = tui_in(dir.path());

    let drawn = tui_drawn(&app);
    assert!(drawn.contains("! alpha"), "{drawn}");
    assert!(
        drawn.contains("it moved to apps/alpha: M relinks it there"),
        "{drawn}"
    );
    press_key(&mut app, KeyCode::Char('M'));
    assert!(tui_drawn(&app).contains("Relink alpha to apps/alpha?"));
    let task = press_and_run(&mut app, KeyCode::Char('y'));

    let relink = Task::Relink {
        old: String::from("alpha"),
        new: String::from("apps/alpha"),
    };
    assert_eq!(
        relink.commands(),
        [["mindweaver", "relink", "alpha", "apps/alpha"]]
    );
    assert_eq!(task, Some(relink));
    assert!(registry(dir.path()).contains("\"apps/alpha\""));
    let status = weaver(dir.path(), &["status", "-p", "apps/alpha"]).unwrap();
    assert!(
        status.stdout.contains("commit-style  current"),
        "{}",
        status.stdout
    );
}

#[test]
fn m_on_a_stale_entry_whose_project_was_not_found_says_how_to_relink_it_by_hand() {
    let dir = workspace_with_two();
    fs::remove_dir_all(dir.path().join("alpha")).unwrap();
    let mut app = tui_in(dir.path());

    let step = press_key(&mut app, KeyCode::Char('M'));

    assert!(matches!(step, tui::state::Step::Stay));
    let said = &app.message.as_ref().unwrap().text;
    assert!(
        said.contains("`mindweaver relink alpha <new>` follows it to where it went"),
        "{said}"
    );
}

#[test]
fn a_on_a_foreign_skill_adopts_it_into_the_library_once_asked() {
    let dir = workspace_with_two();
    let notes = dir.path().join("alpha/.claude/skills/notes");
    fs::create_dir_all(&notes).unwrap();
    fs::write(notes.join("SKILL.md"), skill_file("notes", "Keep notes")).unwrap();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Down);
    assert_eq!(app.current_item().unwrap().name, "notes");

    press_key(&mut app, KeyCode::Char('A'));
    assert!(tui_drawn(&app).contains("Adopt notes from alpha into the library?"));
    let Mode::Confirming(confirm) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert_eq!(
        confirm.question[2],
        "It is copied into the workspace's library, and the project's copies are recorded as \
         installed from there, as they are: updates reach them from now on. Nothing in the \
         project is written."
    );
    let task = press_and_run(&mut app, KeyCode::Char('y'));

    let adopt = Task::Adopt {
        project: String::from("alpha"),
        kind: Kind::Skill,
        name: String::from("notes"),
    };
    assert_eq!(
        adopt.commands(),
        [["mindweaver", "adopt", "-p", "alpha", "--yes", "skill:notes"]]
    );
    assert_eq!(task, Some(adopt));
    assert!(own_directory_for(dir.path(), Kind::Skill)
        .join("notes/SKILL.md")
        .is_file());
    let status = weaver(dir.path(), &["status", "-p", "alpha"]).unwrap();
    assert!(
        status
            .stdout
            .contains("notes  missing   claude,opencode  workspace"),
        "{}",
        status.stdout
    );
}

#[test]
fn a_on_a_skill_that_is_not_foreign_says_what_it_adopts() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Right);
    assert_eq!(app.current_item().unwrap().name, "alpha");

    let step = press_key(&mut app, KeyCode::Char('A'));

    assert!(matches!(step, tui::state::Step::Stay));
    let said = &app.message.as_ref().unwrap().text;
    assert!(said.contains("this skill is authored"), "{said}");
    assert!(!own_directory_for(dir.path(), Kind::Skill)
        .join("alpha")
        .exists());
}

#[test]
fn enter_on_an_artifact_shows_it_as_mind_show_does() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Right);
    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(
        task,
        Some(Task::Show {
            holder: Holder::Project(String::from("alpha")),
            reference: String::from("skill:alpha"),
        })
    );
    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert_eq!(reader.title, "mind -C alpha show skill:alpha");
    let shown = mind(&dir.path().join("alpha"), &["show", "skill:alpha"]).unwrap();
    let lines: Vec<&str> = reader.lines.iter().map(|(_, line)| line.as_str()).collect();
    assert_eq!(lines, shown.stdout.lines().collect::<Vec<_>>());
    // Reading output changes nothing, so it is not in the transcript.
    assert_eq!(app.transcript(), "");
}

#[test]
fn capital_r_reads_the_disk_again_and_runs_no_line() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Down);
    // Added from a shell beside the screen, which reads the disk when it
    // opens and after a line of its own changed something, and not between.
    mind(
        &dir.path().join("beta"),
        &["add", "skill", "later", "Added from a shell"],
    )
    .unwrap();
    assert!(!tui_drawn(&app).contains("Added from a shell"));

    let task = press_and_run(&mut app, KeyCode::Char('R'));

    assert_eq!(task, Some(Task::Refresh));
    assert!(Task::Refresh.commands().is_empty());
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("Added from a shell"), "{drawn}");
    assert_eq!(app.member().unwrap().name, "beta");
    assert!(matches!(app.mode, Mode::Browse), "{:?}", app.mode);
    // Nothing ran, so there is nothing to repeat.
    assert_eq!(app.transcript(), "");
}

#[test]
fn the_keys_sheet_is_drawn_from_its_last_line_and_back_from_its_first() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    let first = "Every key here runs a command you could type instead.";

    press_key(&mut app, KeyCode::Char('?'));
    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    let last = reader.lines.last().unwrap().1.trim().to_owned();
    // Wide enough that no line of it wraps, so each is one row to find; and
    // longer than the screen is high, or scrolling it would show nothing new.
    let drawn = tui_drawn_at(&app, 200, 30);
    assert!(drawn.contains(first), "{drawn}");
    assert!(!drawn.contains(&last), "{drawn}");

    press_key(&mut app, KeyCode::Char('G'));
    let drawn = tui_drawn_at(&app, 200, 30);
    assert!(drawn.contains(&last), "{drawn}");
    assert!(!drawn.contains(first), "{drawn}");

    press_key(&mut app, KeyCode::Char('g'));
    assert!(tui_drawn_at(&app, 200, 30).contains(first));
}

#[test]
fn a_ledger_that_will_not_open_is_said_on_each_projects_rows() {
    let dir = workspace_with_two();
    let ledger = dir.path().join(WEAVER_DIR).join("mindweaver.db");
    fs::write(&ledger, "not a database\n").unwrap();
    let mut app = tui_in(dir.path());

    let said = ledger.display().to_string();
    let mut names = Vec::new();
    for _ in 0..2 {
        let member = app.member().unwrap();
        let warnings = &member.holdings.as_ref().unwrap().warnings;
        assert_eq!(warnings.len(), 1, "{}: {warnings:?}", member.name);
        assert!(warnings[0].contains(&said), "{warnings:?}");
        names.push(member.name.clone());
        press_key(&mut app, KeyCode::Down);
    }
    assert_eq!(names, ["alpha", "beta"]);
}

#[test]
fn an_invalid_artifact_is_marked_where_it_is_listed() {
    let dir = workspace_with_two();
    write_skill(
        &dir.path().join("alpha"),
        "deploy",
        &skill_file("Deployment", "Ship it"),
    );
    let app = tui_in(dir.path());

    let drawn = tui_drawn(&app);

    assert!(drawn.contains("✗"), "{drawn}");
    assert!(drawn.contains("the directory is `deploy`"), "{drawn}");
}

#[test]
fn a_projects_rows_say_how_each_artifact_stands_and_which_agents_read_it() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let collapse = dir.path().join("collapse");
    fs::create_dir(&collapse).unwrap();
    new_project(&collapse);
    weaver(dir.path(), &["link", "collapse"]).unwrap();
    weaver(
        dir.path(),
        &["add", "skill", "commit-style", "How commits are written"],
    )
    .unwrap();
    weaver(dir.path(), &["install", "-p", "collapse", "commit-style"]).unwrap();
    weaver(dir.path(), &["add", "skill", "deploy", "Ship the service"]).unwrap();
    weaver(dir.path(), &["install", "-p", "collapse", "deploy"]).unwrap();
    fs::remove_dir_all(collapse.join(".claude/skills/deploy")).unwrap();
    mind(&collapse, &["add", "skill", "review", "Review a change"]).unwrap();
    let notes = collapse.join(".claude/skills/notes");
    fs::create_dir_all(&notes).unwrap();
    fs::write(notes.join("SKILL.md"), skill_file("notes", "My own notes")).unwrap();
    let mut app = tui_in(dir.path());

    let drawn = tui_drawn(&app);
    press_key(&mut app, KeyCode::Up);
    let own = tui_drawn(&app);

    // What `mindweaver status -p collapse` says, a row each: the state, then a
    // badge in each agent's place where that agent reads it.
    for row in [
        "skill commit-style  current  CC CX OC  How commits are written",
        "skill deploy        missing     CX OC  Ship the service",
        "skill notes         foreign  CC    OC  My own notes",
        "skill review        authored           Review a change",
    ] {
        assert!(drawn.contains(row), "{row}\n{drawn}");
    }
    let status = weaver(dir.path(), &["status", "-p", "collapse"]).unwrap();
    assert_eq!(
        status.stdout,
        "\
commit-style  current   claude,codex,opencode  workspace
deploy        missing   codex,opencode         workspace
notes         foreign   claude,opencode
review        authored  -
"
    );
    // The workspace's own are no project's, and say nothing of the kind.
    assert!(
        own.contains("skill commit-style  How commits are written"),
        "{own}"
    );
}

#[test]
fn validate_in_the_tui_reads_as_weaver_validate_does() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());

    press_and_run(&mut app, KeyCode::Char('v'));

    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    let expected = weaver(dir.path(), &["validate"]).unwrap().stdout;
    let lines: Vec<&str> = reader.lines.iter().map(|(_, line)| line.as_str()).collect();
    assert_eq!(lines, expected.lines().collect::<Vec<_>>());
    press_key(&mut app, KeyCode::Esc);
    assert!(matches!(app.mode, Mode::Browse));
}

#[test]
fn a_stand_in_runner_is_handed_the_lines_a_task_is() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    let heard = Arc::new(Mutex::new(Vec::new()));
    app.runner = Some(Runner::new({
        let heard = Arc::clone(&heard);
        move |words, _, _, _| {
            heard.lock().unwrap().push(words.to_vec());
            Done {
                command: words.join(" "),
                stdout: String::from("stood in\n"),
                stderr: Vec::new(),
                error: None,
                ok: true,
            }
        }
    }));

    press_and_run(&mut app, KeyCode::Char('v'));

    assert_eq!(*heard.lock().unwrap(), [["mindweaver", "validate"]]);
    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert_eq!(reader.lines[0].1, "stood in");
}

#[test]
fn gathering_in_the_tui_is_weaver_gather_git() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let url = source.path().to_string_lossy().into_owned();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('g'));
    type_text(&mut app, &url);
    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(
        task,
        Some(Task::Gather {
            url: url.clone(),
            folder: String::from("skills"),
            reference: None,
        })
    );
    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert!(reader.lines.iter().any(|(_, line)| line.contains("added")));
    assert_eq!(app.workspace().unwrap().shelf, 1);
    assert!(app.transcript().contains("$ mindweaver gather git "));
}

#[test]
fn a_gather_runs_apart_with_its_progress_drawn_until_it_is_over() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let mut app = tui_in(dir.path());
    let (go, gate) = mpsc::channel();
    app.runner = Some(slow_gather(gate));

    press_key(&mut app, KeyCode::Char('g'));
    type_text(&mut app, SLOW);
    let TuiStep::Run(task) = press_key(&mut app, KeyCode::Enter) else {
        panic!("{:?}", app.mode);
    };
    tui::step(&mut app, task);

    // Back at once, with the gather still going and the screen drawing it.
    assert!(app.running.is_some());
    settle_until(&mut app, |app| {
        reported(app) == Some(Progress::Gather(Gather::Harvesting { done: 0, total: 3 }))
    });
    let drawn = tui_drawn(&app);
    assert!(
        drawn.contains(&format!("mindweaver gather git {SLOW}")),
        "{drawn}"
    );
    assert!(drawn.contains("reading folders 0/3"), "{drawn}");
    // A key for anything else was aimed at a screen about to change.
    assert_eq!(press_key(&mut app, KeyCode::Char('q')), TuiStep::Stay);

    go.send(()).unwrap();
    settle_until(&mut app, |app| {
        reported(app) == Some(Progress::Gather(Gather::Harvesting { done: 1, total: 3 }))
    });
    assert!(tui_drawn(&app).contains("reading folders 1/3"));

    for _ in 0..3 {
        go.send(()).unwrap();
    }
    settle_until(&mut app, |app| app.running.is_none());

    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert_eq!(reader.lines[0].1, "3 skills: 3 added");
    assert!(app.transcript().contains(&format!(
        "$ mindweaver gather git {SLOW}\n3 skills: 3 added\n"
    )));
}

#[test]
fn esc_cancels_a_running_gather_and_the_form_says_so() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let mut app = tui_in(dir.path());
    let (_go, gate) = mpsc::channel();
    app.runner = Some(slow_gather(gate));
    start_slow_gather(&mut app);
    assert!(tui_drawn(&app).contains("esc cancels"));

    assert_eq!(press_key(&mut app, KeyCode::Esc), TuiStep::Stay);

    // Asked, and said to be asked, until the gather has heard it.
    assert!(app.running.as_ref().unwrap().cancelling);
    assert!(tui_drawn(&app).contains("cancelling, before anything is placed"));
    settle_until(&mut app, |app| app.running.is_none());
    // Back in the form it was started from, with why it stopped, as for any
    // gather that did not get as far as a report.
    let Mode::Gathering(form) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert_eq!(
        form.error.as_deref(),
        Some("cancelled, with nothing put on the shelf")
    );
    assert!(app.transcript().contains(&format!(
        "$ mindweaver gather git {SLOW}\nerror: cancelled, with nothing put on the shelf\n"
    )));
}

#[test]
fn a_gather_already_placing_says_a_cancel_is_too_late_and_finishes() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let mut app = tui_in(dir.path());
    let (go, gate) = mpsc::channel();
    app.runner = Some(placing_gather(gate));
    start_gather(&mut app, Progress::Gather(Gather::Placing { done: 0, total: 3 }));
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("placing on the shelf 0/3"), "{drawn}");
    // No key offered that would change nothing.
    assert!(!drawn.contains("esc cancels"), "{drawn}");
    assert!(drawn.contains("too late to cancel: placing"), "{drawn}");

    assert_eq!(press_key(&mut app, KeyCode::Esc), TuiStep::Stay);

    // Nor a stop claimed that the gather will not make.
    let drawn = tui_drawn(&app);
    assert!(!drawn.contains("cancelling"), "{drawn}");
    assert!(drawn.contains("too late to cancel: placing"), "{drawn}");
    go.send(()).unwrap();
    settle_until(&mut app, |app| app.running.is_none());
    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert_eq!(reader.lines[0].1, "3 skills: 3 added");
}

#[test]
fn leaving_while_a_gather_runs_cancels_it_and_waits_for_it() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let mut app = tui_in(dir.path());
    let (_go, gate) = mpsc::channel();
    app.runner = Some(slow_gather(gate));
    start_slow_gather(&mut app);

    let step = app.press(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

    assert_eq!(step, TuiStep::Quit);
    // What the loop does before it returns.
    tui::wait(&mut app);
    assert!(app.running.is_none());
    assert!(app
        .transcript()
        .contains("error: cancelled, with nothing put on the shelf"));
}

#[test]
fn the_working_boxes_are_drawn_on_a_terminal_narrower_than_they_are() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let mut app = tui_in(dir.path());
    let (_go, gate) = mpsc::channel();
    app.runner = Some(slow_gather(gate));
    start_slow_gather(&mut app);
    let mut terminal = Terminal::new(TestBackend::new(20, 30)).unwrap();

    // Drawn on every frame while the gather runs, so a pane split narrow
    // halfway through must not end the process, and the gather with it.
    terminal.draw(|frame| tui::ui::draw(&app, frame)).unwrap();
    app.running.as_mut().unwrap().cancel();
    tui::wait(&mut app);
    // And the box a task that runs in place is drawn with.
    app.busy = Some(format!("mindweaver gather git {SLOW}"));
    terminal.draw(|frame| tui::ui::draw(&app, frame)).unwrap();
}

#[test]
fn a_gather_refused_for_a_token_in_its_url_shows_the_token_neither_while_it_runs_nor_after() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('g'));
    type_text(&mut app, "https://me:ghp_t0ken@example.com/acme/skills");
    let task = press_and_run(&mut app, KeyCode::Enter).unwrap();

    // What the box drawn over the screen says while it runs.
    assert_eq!(
        task.describe(),
        "mindweaver gather git https://example.com/acme/skills"
    );
    let transcript = app.transcript();
    assert!(
        transcript.starts_with("$ mindweaver gather git https://example.com/acme/skills\n"),
        "{transcript}"
    );
    assert!(transcript.contains("is refused"), "{transcript}");
    assert!(!transcript.contains("ghp_t0ken"), "{transcript}");
}

/// Where the shelf `shelved` gathered offers its skills from, as the
/// install screen says it.
fn shelf_origin(workspace: &WeaverWorkspace, ledger: &Ledger) -> String {
    screen_for(workspace, ledger).targets[0].rows[0]
        .candidate
        .origin()
        .into_string()
}

#[test]
fn the_install_screen_opens_inside_the_tui_and_applies_from_there() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let origin = shelf_origin(&workspace, &ledger);
    let mut app = tui_in(dir.path());

    press_and_run(&mut app, KeyCode::Char('i'));
    assert!(matches!(app.mode, Mode::Installing(_)), "{:?}", app.mode);
    assert!(tui_drawn(&app).contains("Skills in collapse"));

    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Char(' '));
    press_key(&mut app, KeyCode::Char('a'));
    let task = press_and_run(&mut app, KeyCode::Char('y'));

    assert_eq!(
        task,
        Some(Task::ApplyInstall {
            changes: vec![Task::Install {
                projects: vec![String::from("collapse")],
                kind: Kind::Skill,
                name: String::from("deploy"),
                from: Some(origin.clone()),
                yes: false,
            }]
        })
    );
    for copy in copies(&dir.path().join("collapse"), "deploy") {
        assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
    }
    let said = format!("installed deploy into collapse, from {origin}");
    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert!(reader.lines.iter().any(|(_, line)| *line == said));
    assert_eq!(
        app.transcript(),
        format!("$ mindweaver install -p collapse --from {origin} skill:deploy\n{said}\n")
    );
}

#[test]
fn the_install_screen_inside_the_tui_turns_round_by_artifact_and_applies_the_same_batch() {
    let (dir, workspace, ledger) = shelved(&["collapse", "tanukeys"], &["deploy"]);
    let origin = shelf_origin(&workspace, &ledger);
    let mut app = tui_in(dir.path());
    press_and_run(&mut app, KeyCode::Char('i'));

    press_key(&mut app, KeyCode::Char('v'));
    let drawn = tui_drawn(&app);
    for key in [KeyCode::Right, KeyCode::Char(' '), KeyCode::Down] {
        press_key(&mut app, key);
    }
    press_key(&mut app, KeyCode::Char(' '));
    press_key(&mut app, KeyCode::Char('a'));
    let task = press_and_run(&mut app, KeyCode::Char('y'));

    assert!(drawn.contains("Projects for deploy"), "{drawn}");
    assert!(
        matches!(&task, Some(Task::ApplyInstall { changes }) if changes.len() == 1),
        "one artifact's ticks are one line: {task:?}"
    );
    for project in ["collapse", "tanukeys"] {
        for copy in copies(&dir.path().join(project), "deploy") {
            assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
        }
    }
    assert_eq!(
        app.transcript(),
        format!(
            "$ mindweaver install -p collapse -p tanukeys --from {origin} skill:deploy\n\
             installed deploy into collapse, from {origin}\n\
             installed deploy into tanukeys, from {origin}\n"
        )
    );
}

#[test]
fn the_install_screen_inside_the_tui_installs_where_each_agent_of_a_v3_project_reads() {
    let (dir, _workspace, _ledger) = shelved(&[], &["deploy"]);
    let root = dir.path().join("alpha");
    fs::create_dir_all(root.join(MIND_DIR)).unwrap();
    fs::write(
        root.join(MIND_DIR).join("settings.json"),
        r#"{
  "version": 3,
  "id": "0123456789abcdef0123456789abcdef",
  "name": "alpha",
  "agents": ["claude", "codex", "opencode"],
  "share": "shared",
  "directories": {}
}
"#,
    )
    .unwrap();
    weaver(dir.path(), &["link", "alpha"]).unwrap();
    let mut app = tui_in(dir.path());

    press_and_run(&mut app, KeyCode::Char('i'));
    assert!(tui_drawn(&app).contains("Skills in alpha"));
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Char(' '));
    press_key(&mut app, KeyCode::Char('a'));
    let task = press_and_run(&mut app, KeyCode::Char('y'));

    assert!(
        matches!(&task, Some(Task::ApplyInstall { changes }) if changes.len() == 1),
        "{task:?}"
    );
    let claude = fs::read(root.join(".claude/skills/deploy/SKILL.md")).unwrap();
    let agents = fs::read(root.join(".agents/skills/deploy/SKILL.md")).unwrap();
    assert_eq!(claude, agents);
    // Opened again, now that it links `alpha`.
    let workspace = WeaverWorkspace::open(dir.path()).unwrap();
    let origin = shelf_origin(&workspace, &workspace.ledger_for_reading().unwrap());
    assert!(
        app.transcript().starts_with(&format!(
            "$ mindweaver install -p alpha --from {origin} skill:deploy\n\
             installed deploy into alpha, from {origin}\n"
        )),
        "{}",
        app.transcript()
    );
}

#[test]
fn a_rule_ticked_on_the_install_screen_inside_the_tui_is_installed_from_there() {
    let (dir, _workspace, _ledger) = shelved(&["collapse"], &["deploy"]);
    weaver(
        dir.path(),
        &[
            "add",
            "rule",
            "git/commits",
            "Commit one coherent step at a time",
        ],
    )
    .unwrap();
    let mut app = tui_in(dir.path());

    press_and_run(&mut app, KeyCode::Char('i'));
    press_key(&mut app, KeyCode::Tab);
    let rules = tui_drawn(&app);
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Char(' '));
    let ticked = tui_drawn(&app);
    press_key(&mut app, KeyCode::Char('a'));
    let task = press_and_run(&mut app, KeyCode::Char('y'));

    assert!(rules.contains("Rules in collapse"), "{rules}");
    assert!(rules.contains("[ ] git/commits"), "{rules}");
    assert!(ticked.contains("[x] git/commits  + install"), "{ticked}");
    assert!(ticked.contains("1 to install, 0 to remove"), "{ticked}");
    assert_eq!(
        task,
        Some(Task::ApplyInstall {
            changes: vec![Task::Install {
                projects: vec![String::from("collapse")],
                kind: Kind::Rule,
                name: String::from("git/commits"),
                from: Some(String::from("workspace")),
                yes: false,
            }]
        })
    );
    assert!(fs::read_to_string(dir.path().join("collapse/AGENTS.md"))
        .unwrap()
        .contains("<!-- mindweaver:rule git/commits -->\n# Commit one coherent step at a time\n"));
    assert!(
        app.transcript().starts_with(
            "$ mindweaver install -p collapse --from workspace rule:git/commits\n\
             installed rule git/commits into collapse, from workspace\n"
        ),
        "{}",
        app.transcript()
    );
}

#[test]
fn every_line_the_tui_runs_and_its_install_screen_keep_out_of_the_agents_folders_it_was_given() {
    let (dir, _workspace, _ledger) = shelved(&["collapse"], &["deploy"]);
    // The project's `.agents` stands for the folder several agents read for
    // every project, as a project at the user's home has.
    let collapse = dir.path().join("collapse");
    let lookup = Lookup {
        agents: AgentHomes {
            agents_dir: Some(collapse.join(".agents")),
            ..AgentHomes::default()
        },
        ..Lookup::default()
    };
    let words: Vec<String> = ["mindweaver", "install", "-p", "collapse", "deploy"]
        .iter()
        .map(|word| (*word).to_owned())
        .collect();

    let done = tui::execute(&words, dir.path(), &lookup, &Watch::new());

    assert!(
        done.error
            .as_deref()
            .is_some_and(|error| error.contains("which more than one agent reads")),
        "{done:?}"
    );
    let view = tui::load(dir.path(), &lookup).unwrap();
    let mut app = App::new(dir.path().to_path_buf(), view).with_lookup(lookup);
    press_and_run(&mut app, KeyCode::Char('i'));
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Char(' '));
    press_key(&mut app, KeyCode::Char('a'));
    press_and_run(&mut app, KeyCode::Char('y'));
    assert!(
        app.transcript().contains("which more than one agent reads"),
        "{}",
        app.transcript()
    );
    for copy in copies(&collapse, "deploy") {
        assert!(!copy.exists(), "{}", copy.display());
    }
}

#[test]
fn the_install_screen_scrubs_the_ledger_when_it_applies_and_not_when_it_opens() {
    let (dir, workspace, _ledger) = shelved(&["collapse"], &["deploy"]);
    let database = rusqlite::Connection::open(workspace.ledger_path()).unwrap();
    database
        .execute(
            "UPDATE sources SET url = 'https://me:ghp_t0ken@example.com/acme/skills'",
            [],
        )
        .unwrap();
    let stored = || -> String {
        database
            .query_row("SELECT url FROM sources", [], |row| row.get(0))
            .unwrap()
    };
    let mut app = tui_in(dir.path());

    // Opened and left: a read, which writes nothing and so has nothing to say.
    press_and_run(&mut app, KeyCode::Char('i'));
    assert!(matches!(app.mode, Mode::Installing(_)), "{:?}", app.mode);
    press_key(&mut app, KeyCode::Char('q'));
    assert_eq!(app.transcript(), "");
    assert!(stored().contains("ghp_t0ken"));

    press_and_run(&mut app, KeyCode::Char('i'));
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Char(' '));
    press_key(&mut app, KeyCode::Char('a'));
    press_and_run(&mut app, KeyCode::Char('y'));

    let transcript = app.transcript();
    assert!(
        transcript.starts_with(
            "$ mindweaver install -p collapse --from https://example.com/acme/skills skill:deploy\n\
             installed deploy into collapse, from https://example.com/acme/skills\n"
        ),
        "{transcript}"
    );
    assert!(
        transcript.contains(
            "note: https://example.com/acme/skills: the user name, password or token an older \
             Mindweaver stored in this URL is taken out"
        ),
        "{transcript}"
    );
    assert!(!transcript.contains("ghp_t0ken"), "{transcript}");
    assert_eq!(stored(), "https://example.com/acme/skills");
}

#[test]
fn a_line_of_the_install_screen_that_fails_does_not_keep_the_rest_from_running() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let origin = shelf_origin(&workspace, &ledger);
    weaver(
        dir.path(),
        &["add", "skill", "ours", "Of the workspace's own"],
    )
    .unwrap();
    let mut app = tui_in(dir.path());
    press_and_run(&mut app, KeyCode::Char('i'));
    // The workspace's own first, then the shelf's.
    for key in [
        KeyCode::Right,
        KeyCode::Char(' '),
        KeyCode::Down,
        KeyCode::Char(' '),
        KeyCode::Char('a'),
    ] {
        press_key(&mut app, key);
    }
    // Gone once it was ticked, so its line is refused.
    fs::remove_dir_all(own_directory_for(dir.path(), Kind::Skill).join("ours")).unwrap();

    press_and_run(&mut app, KeyCode::Char('y'));

    for copy in copies(&dir.path().join("collapse"), "deploy") {
        assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
    }
    let transcript = app.transcript();
    assert!(
        transcript
            .starts_with("$ mindweaver install -p collapse --from workspace skill:ours\nerror: "),
        "{transcript}"
    );
    assert!(
        transcript.contains(&format!(
            "$ mindweaver install -p collapse --from {origin} skill:deploy\n\
             installed deploy into collapse, from {origin}\n"
        )),
        "{transcript}"
    );
}

/// Copy everything under `from` into `to`, as it is.
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Every folder and file under `root` by its path from there, with what
/// each file holds; but the ledger, which is compared by what it records
/// rather than by its bytes, which hold when each action was taken, and the
/// workspace's lock.
fn disk(root: &Path) -> std::collections::BTreeMap<String, Option<Vec<u8>>> {
    let mut found = std::collections::BTreeMap::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            let route = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let kept = [".mindweaver/mindweaver.db", ".mindweaver/lock"];
            if kept.iter().any(|kept| route.starts_with(kept)) {
                continue;
            }
            if path.is_dir() {
                folders.push(path);
                found.insert(route, None);
            } else {
                found.insert(route, Some(fs::read(&path).unwrap()));
            }
        }
    }
    found
}

/// What the ledger of the workspace at `root` records of every installation
/// in each of `projects`.
fn recorded(root: &Path, projects: &[&str]) -> Vec<mindweaver_core::ledger::InstallRecord> {
    let ledger = WeaverWorkspace::open(root)
        .unwrap()
        .ledger_for_reading()
        .unwrap();
    let mut records = Vec::new();
    for project in projects {
        for kind in Kind::ALL {
            records.extend(ledger.records(project, kind).unwrap());
        }
    }
    records
}

/// A command line as the transcript shows it, back into its words: split at
/// spaces, but not inside the single quotes `state::shell` puts round a word
/// a shell would split, nor at a quote escaped between two of them (`'\''`).
fn words_of(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let (mut quoted, mut escaped) = (false, false);
    for c in line.chars() {
        let text = word.get_or_insert_with(String::new);
        match c {
            c if escaped => {
                text.push(c);
                escaped = false;
            }
            '\\' if !quoted => escaped = true,
            '\'' => quoted = !quoted,
            ' ' if !quoted => {
                if let Some(done) = word.take().filter(|done| !done.is_empty()) {
                    words.push(done);
                }
            }
            c => text.push(c),
        }
    }
    words.extend(word.filter(|done| !done.is_empty()));
    words
}

#[test]
fn a_line_as_the_transcript_shows_it_reads_back_into_its_words() {
    let words: Vec<String> = [
        "mindweaver",
        "install",
        "--from",
        "it's #1",
        "a b",
        "skill:deploy",
    ]
    .iter()
    .map(|word| (*word).to_owned())
    .collect();

    assert_eq!(words_of(&tui::state::shell(&words)), words);
}

#[test]
fn the_lines_the_install_screen_ran_run_again_on_a_copy_leave_the_same_ledger_and_disk() {
    let (dir, workspace, ledger) = shelved(&["collapse", "tanukeys"], &["deploy", "lint"]);
    let origin = shelf_origin(&workspace, &ledger);
    weaver(
        dir.path(),
        &["add", "rule", "git/commits", "Commit one step at a time"],
    )
    .unwrap();
    weaver(dir.path(), &["install", "-p", "tanukeys", "lint"]).unwrap();
    let copy = TempDir::new().unwrap();
    copy_tree(dir.path(), copy.path());
    let projects = ["collapse", "tanukeys"];
    assert_eq!(disk(dir.path()), disk(copy.path()));

    let mut app = tui_in(dir.path());
    press_and_run(&mut app, KeyCode::Char('i'));
    for key in [
        // collapse: deploy, then git/commits, ticked.
        KeyCode::Right,
        KeyCode::Char(' '),
        KeyCode::Tab,
        KeyCode::Char(' '),
        // tanukeys: git/commits and deploy ticked, lint unticked, back
        // past the MCP servers to the skills.
        KeyCode::Left,
        KeyCode::Down,
        KeyCode::Right,
        KeyCode::Char(' '),
        KeyCode::BackTab,
        KeyCode::Char(' '),
        KeyCode::Down,
        KeyCode::Char(' '),
    ] {
        press_key(&mut app, key);
    }
    let marked = tui_drawn(&app);
    assert!(marked.contains("4 to install, 1 to remove"), "{marked}");
    press_key(&mut app, KeyCode::Char('a'));
    press_and_run(&mut app, KeyCode::Char('y'));

    let transcript = app.transcript();
    let ran: Vec<&str> = transcript
        .lines()
        .filter_map(|line| line.strip_prefix("$ "))
        .collect();
    assert_eq!(
        ran,
        [
            &format!("mindweaver install -p collapse -p tanukeys --from {origin} skill:deploy"),
            "mindweaver uninstall -p tanukeys skill:lint",
            "mindweaver install -p collapse -p tanukeys --from workspace rule:git/commits",
        ],
        "{transcript}"
    );
    assert!(app.done.iter().all(|done| done.ok), "{transcript}");
    assert_ne!(
        disk(dir.path()),
        disk(copy.path()),
        "the screen changed nothing"
    );

    for line in &ran {
        let words = words_of(line);
        assert_eq!(words[0], "mindweaver", "{line}");
        let args: Vec<&str> = words[1..].iter().map(String::as_str).collect();
        let replayed = weaver(copy.path(), &args).unwrap();
        assert!(replayed.ok, "{line}: {replayed:?}");
    }

    assert_eq!(
        recorded(dir.path(), &projects),
        recorded(copy.path(), &projects)
    );
    assert_eq!(disk(dir.path()), disk(copy.path()));
}

#[test]
fn leaving_the_install_screen_goes_back_rather_than_out() {
    let (dir, _workspace, _ledger) = shelved(&["collapse"], &["deploy"]);
    let mut app = tui_in(dir.path());
    press_and_run(&mut app, KeyCode::Char('i'));

    assert_eq!(
        press_key(&mut app, KeyCode::Char('q')),
        tui::state::Step::Stay
    );
    assert!(matches!(app.mode, Mode::Browse));
}

#[test]
fn ctrl_c_leaves_even_from_the_middle_of_a_form() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Char('a'));
    type_text(&mut app, "half");

    let step = app.press(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

    assert_eq!(step, tui::state::Step::Quit);
}

#[test]
fn every_task_the_tui_runs_is_a_line_the_binaries_accept() {
    // Free text that looks like a flag, carries quotes, and a directory that
    // starts with a dash: the shapes that break a naive command line.
    let awkward = String::from("- the fast way, it's 'quoted'");
    let tasks = vec![
        Task::InitWorkspace,
        Task::Link {
            path: String::from("../collapse"),
        },
        Task::InitAndLink {
            path: String::from("-odd"),
        },
        Task::Unlink {
            entry: String::from("alpha"),
        },
        Task::Add {
            holder: Holder::Project(String::from("alpha")),
            kind: Kind::Skill,
            name: String::from("deploy"),
            description: awkward.clone(),
            template: None,
            server: None,
        },
        Task::Show {
            holder: Holder::Project(String::from("alpha")),
            reference: String::from("rule:git/no-force-push"),
        },
        Task::Validate {
            holder: Holder::Workspace,
        },
        Task::Validate {
            holder: Holder::Project(String::from(".")),
        },
        Task::Shelf,
        Task::Gather {
            url: String::from("https://example.com/skills.git"),
            folder: String::from("agents"),
            reference: Some(String::from("-weird")),
        },
        Task::Add {
            holder: Holder::Project(String::from("alpha")),
            kind: Kind::Rule,
            name: String::from("git/no-force-push"),
            description: String::from("Never"),
            template: Some(String::from("policy")),
            server: None,
        },
        Task::Edit {
            holder: Holder::Project(String::from("-odd")),
            kind: Kind::Rule,
            name: String::from("git/no-force-push"),
        },
        Task::Rename {
            holder: Holder::Project(String::from("alpha")),
            kind: Kind::Skill,
            from: String::from("deploy"),
            to: String::from("ship"),
        },
        Task::Remove {
            holder: Holder::Project(String::from("alpha")),
            kind: Kind::Skill,
            name: String::from("deploy"),
            everywhere: false,
            yes: true,
        },
        Task::Add {
            holder: Holder::Workspace,
            kind: Kind::Skill,
            name: String::from("commit-style"),
            description: String::from("Shared"),
            template: Some(String::from("full")),
            server: None,
        },
        Task::Edit {
            holder: Holder::Workspace,
            kind: Kind::Skill,
            name: String::from("commit-style"),
        },
        Task::Remove {
            holder: Holder::Project(String::from(".")),
            kind: Kind::Rule,
            name: String::from("style"),
            everywhere: false,
            yes: true,
        },
        Task::InitProject,
        Task::LinkHere,
        Task::Load {
            path: String::from("../team"),
        },
        Task::Load {
            path: String::from("-odd"),
        },
        Task::LoadHere {
            path: String::from("../team"),
        },
        Task::Unload {
            entry: String::from("../team"),
        },
        Task::UnloadHere {
            path: String::from("/somewhere/team"),
        },
        Task::Remove {
            holder: Holder::Elsewhere(String::from("../collapse")),
            kind: Kind::Skill,
            name: String::from("deploy"),
            everywhere: false,
            yes: true,
        },
        Task::Add {
            holder: Holder::Elsewhere(String::from("-odd")),
            kind: Kind::Skill,
            name: String::from("deploy"),
            description: awkward.clone(),
            template: Some(String::from("-weird")),
            server: None,
        },
        Task::Settings {
            holder: Holder::Project(String::from("alpha")),
            agents: Some(AgentsLine::Named(vec![Agent::Codex, Agent::OpenCode])),
            share: None,
            edit_tracked: false,
        },
        Task::Settings {
            holder: Holder::Elsewhere(String::from("-odd")),
            agents: Some(AgentsLine::Removed(vec![Agent::Claude])),
            share: Some(Share::Local),
            edit_tracked: true,
        },
        Task::Settings {
            holder: Holder::Project(String::from(".")),
            agents: None,
            share: Some(Share::Shared),
            edit_tracked: false,
        },
        Task::Targets {
            holder: Holder::Project(String::from("alpha")),
            kind: Kind::Skill,
            folders: Some(vec![String::from(".skills"), String::from("-odd")]),
        },
        Task::Targets {
            holder: Holder::Elsewhere(String::from("-odd")),
            kind: Kind::Skill,
            folders: None,
        },
        Task::Update {
            holder: Holder::Project(String::from("alpha")),
            reference: None,
            force: false,
        },
        Task::Update {
            holder: Holder::Elsewhere(String::from("-odd")),
            reference: Some(String::from("rule:git/no-force-push")),
            force: true,
        },
        Task::Update {
            holder: Holder::Workspace,
            reference: Some(String::from("skill:commit-style")),
            force: false,
        },
        Task::Remove {
            holder: Holder::Workspace,
            kind: Kind::Rule,
            name: String::from("git/no-force-push"),
            everywhere: true,
            yes: true,
        },
        Task::Install {
            projects: vec![String::from("alpha"), String::from("-odd")],
            kind: Kind::Skill,
            name: String::from("deploy"),
            from: Some(String::from("-weird")),
            yes: false,
        },
        Task::Install {
            projects: vec![String::from("../collapse")],
            kind: Kind::Rule,
            name: String::from("git/no-force-push"),
            from: None,
            yes: false,
        },
        Task::Uninstall {
            projects: vec![String::from("-odd"), String::from("./alpha")],
            kind: Kind::Rule,
            name: String::from("git/no-force-push"),
            force: false,
        },
    ];

    for task in &tasks {
        for line in task.commands() {
            let parsed = match line[0].as_str() {
                "mind" => Cli::try_parse_from(&line).err(),
                "mindweaver" => WeaverCli::try_parse_from(&line).err(),
                other => panic!("{task:?} runs `{other}`"),
            };
            assert!(parsed.is_none(), "{line:?} does not parse: {parsed:?}");
        }
    }

    // And the free text arrives word for word.
    let line = tasks[4].commands().remove(0);
    let cli = Cli::try_parse_from(&line).unwrap();
    let Some(mindweaver_cli::Command::Add { description, .. }) = cli.command else {
        panic!("{line:?}");
    };
    assert_eq!(description, awkward);
}

#[test]
fn the_agents_and_share_toggles_parse_back_into_what_they_were() {
    let task = Task::Settings {
        holder: Holder::Elsewhere(String::from("../collapse")),
        agents: Some(AgentsLine::Named(vec![Agent::OpenCode, Agent::Claude])),
        share: Some(Share::Local),
        edit_tracked: true,
    };
    let lines = task.commands();
    assert_eq!(
        task.describe(),
        "mind -W . -C ../collapse agents --yes opencode claude && \
         mind -W . -C ../collapse share --yes --edit-tracked local"
    );

    let agents = Cli::try_parse_from(&lines[0]).unwrap();
    assert_eq!(agents.directory, Some(PathBuf::from("../collapse")));
    assert_eq!(agents.workspace.as_deref(), Some("."));
    let Some(mindweaver_cli::Command::Agents {
        change: None,
        named,
    }) = agents.command
    else {
        panic!("{:?}", lines[0]);
    };
    assert_eq!(named.agents, [Agent::OpenCode, Agent::Claude]);
    assert!(named.yes);

    let share = Cli::try_parse_from(&lines[1]).unwrap();
    let Some(mindweaver_cli::Command::Share { mode }) = share.command else {
        panic!("{:?}", lines[1]);
    };
    assert_eq!(mode.share, Share::Local);
    assert!(mode.edit_tracked);
    assert!(mode.yes);
}

#[test]
fn the_install_and_uninstall_tasks_parse_back_into_what_they_were() {
    let install = Task::Install {
        projects: vec![String::from("alpha"), String::from("-odd")],
        kind: Kind::Mcp,
        name: String::from("deploy"),
        from: Some(String::from("https://example.com/acme/skills#main")),
        yes: true,
    };
    let uninstall = Task::Uninstall {
        projects: vec![String::from("../collapse")],
        kind: Kind::Rule,
        name: String::from("git/commits"),
        force: false,
    };

    assert_eq!(
        install.describe(),
        "mindweaver install -p alpha -p ./-odd --from 'https://example.com/acme/skills#main' \
         --yes mcp:deploy"
    );
    assert_eq!(
        uninstall.describe(),
        "mindweaver uninstall -p ../collapse rule:git/commits"
    );
    assert!(install.writes() && uninstall.writes());

    let line = install.commands().remove(0);
    let Some(mindweaver_cli::WeaverCommand::Install {
        projects,
        all_projects,
        name,
        from,
        force,
        yes,
    }) = WeaverCli::try_parse_from(&line).unwrap().command
    else {
        panic!("{line:?}");
    };
    assert_eq!(projects, ["alpha", "./-odd"]);
    assert!(!all_projects && !force && yes);
    assert_eq!(name.as_deref(), Some("mcp:deploy"));
    assert_eq!(
        from.as_deref(),
        Some("https://example.com/acme/skills#main")
    );

    let line = uninstall.commands().remove(0);
    let Some(mindweaver_cli::WeaverCommand::Uninstall {
        projects,
        name,
        orphaned,
        force,
        ..
    }) = WeaverCli::try_parse_from(&line).unwrap().command
    else {
        panic!("{line:?}");
    };
    assert_eq!(projects, ["../collapse"]);
    assert_eq!(name.as_deref(), Some("rule:git/commits"));
    assert!(!orphaned && !force);
}

#[test]
fn an_install_task_installs_into_every_project_it_names_and_an_uninstall_takes_it_out() {
    let (dir, _workspace, _ledger) = shelved(&["collapse", "tanukeys"], &["deploy"]);
    let mut app = tui_in(dir.path());
    let projects = vec![String::from("collapse"), String::from("tanukeys")];

    tui::step(
        &mut app,
        Task::Install {
            projects: projects.clone(),
            kind: Kind::Skill,
            name: String::from("deploy"),
            from: None,
            yes: false,
        },
    );
    for project in &projects {
        for copy in copies(&dir.path().join(project), "deploy") {
            assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
        }
    }
    tui::step(
        &mut app,
        Task::Uninstall {
            projects: projects.clone(),
            kind: Kind::Skill,
            name: String::from("deploy"),
            force: false,
        },
    );

    for project in &projects {
        for copy in copies(&dir.path().join(project), "deploy") {
            assert!(!copy.exists(), "{}", copy.display());
        }
    }
    let transcript = app.transcript();
    assert!(
        transcript.starts_with(
            "$ mindweaver install -p collapse -p tanukeys skill:deploy\n\
             installed deploy into collapse, from "
        ),
        "{transcript}"
    );
    assert!(
        transcript.contains(
            "$ mindweaver uninstall -p collapse -p tanukeys skill:deploy\n\
             removed deploy from collapse\n\
             removed deploy from tanukeys\n"
        ),
        "{transcript}"
    );
}

// ---------------------------------------------------------------------------
// The same, from the TUI
// ---------------------------------------------------------------------------

#[test]
fn e_on_an_artifact_is_mind_edit_with_the_terminal_handed_over() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Right);

    let step = press_key(&mut app, KeyCode::Char('e'));

    // Not run here: it would open a real editor.
    let tui::state::Step::Run(task) = step else {
        panic!("{step:?}");
    };
    assert!(task.takes_terminal());
    assert_eq!(task.describe(), "mind -C alpha edit skill:alpha");
}

#[test]
fn keys_for_one_artifact_want_the_artifacts_column() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());

    // The cursor over there is dimmed from here, and deleting what a dimmed
    // cursor rests on is deleting the wrong thing.
    for key in ['e', 'r', 'd'] {
        assert_eq!(
            press_key(&mut app, KeyCode::Char(key)),
            tui::state::Step::Stay
        );
        assert!(matches!(app.mode, Mode::Browse));
    }
    assert!(tui_drawn(&app).contains("pick an artifact first"));
}

#[test]
fn d_asks_before_it_runs_mind_remove() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Right);

    press_key(&mut app, KeyCode::Char('d'));
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("Delete skill alpha?"), "{drawn}");
    assert!(
        drawn.contains("mind -C alpha remove --yes skill:alpha"),
        "{drawn}"
    );
    press_and_run(&mut app, KeyCode::Char('y'));

    assert!(!dir.path().join("alpha/skills/alpha").exists());
    assert!(app
        .transcript()
        .contains("$ mind -C alpha remove --yes skill:alpha\nremoved skill alpha"));
}

#[test]
fn r_renames_from_a_form_that_starts_out_holding_the_old_name() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Right);

    press_key(&mut app, KeyCode::Char('r'));
    let Mode::Renaming(form) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert_eq!(form.name.text(), "alpha");
    // Ctrl-U clears it, as at a shell prompt.
    app.press(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    type_text(&mut app, "first");
    assert!(tui_drawn(&app).contains("mind -C alpha rename skill:alpha first"));
    press_and_run(&mut app, KeyCode::Enter);

    assert!(dir.path().join("alpha/skills/first/SKILL.md").is_file());
    assert_eq!(app.current_item().unwrap().name, "first");
}

#[test]
fn a_refused_rename_leaves_the_form_open_with_the_reason() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Char('r'));

    press_and_run(&mut app, KeyCode::Enter);

    let Mode::Renaming(form) = &app.mode else {
        panic!("the form closed: {:?}", app.mode);
    };
    assert!(form.error.as_deref().unwrap().contains("already called"));
}

#[test]
fn the_add_form_offers_the_templates_the_project_can_use() {
    let dir = workspace_with_two();
    let template = dir.path().join("alpha/.mind/templates/runbook/SKILL.md");
    fs::create_dir_all(template.parent().unwrap()).unwrap();
    fs::write(&template, "---\n---\n\n# {{name}}\n\nFrom the runbook.\n").unwrap();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('a'));
    press_key(&mut app, KeyCode::Up);
    // none, then full (built in), then runbook.
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Down);
    type_text(&mut app, "deploy");
    press_key(&mut app, KeyCode::Down);
    type_text(&mut app, "Ship it");
    assert!(tui_drawn(&app).contains("--template runbook"));
    press_and_run(&mut app, KeyCode::Enter);

    let manifest = fs::read_to_string(dir.path().join("alpha/skills/deploy/SKILL.md")).unwrap();
    assert!(manifest.contains("From the runbook."), "{manifest}");
}

#[test]
fn the_add_form_offers_what_the_workspace_in_your_home_has_to_a_project_elsewhere() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let project = elsewhere.path().join("collapse");
    fs::create_dir(&project).unwrap();
    mind(&project, &["init"]).unwrap();
    weaver_at_home(elsewhere.path(), home.path(), &["link", "collapse"]).unwrap();
    let template = home.path().join(".mindweaver/templates/runbook/SKILL.md");
    fs::create_dir_all(template.parent().unwrap()).unwrap();
    fs::write(&template, "---\n---\n\n# {{name}}\n\nFrom home.\n").unwrap();
    let mut app = App::opened_on(
        Scope::Project,
        project.clone(),
        tui::load_project(&project, &Lookup::at_home(Some(home.path()))).unwrap(),
    )
    .with_lookup(Lookup::at_home(Some(home.path())));
    app.focus_on(&project);

    // The form runs `mind -C <entry> add`, which consults the workspace in
    // your home about a project nothing above manages; it offers the same.
    press_key(&mut app, KeyCode::Char('a'));
    press_key(&mut app, KeyCode::Up);
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Down);
    type_text(&mut app, "deploy");
    press_key(&mut app, KeyCode::Down);
    type_text(&mut app, "Ship it");
    assert!(tui_drawn(&app).contains("--template runbook"));
    press_and_run(&mut app, KeyCode::Enter);

    let manifest = fs::read_to_string(project.join("skills/deploy/SKILL.md")).unwrap();
    assert!(manifest.contains("From home."), "{manifest}");
}

#[test]
fn mind_calls_the_workspace_in_your_home_the_default_only_when_it_is_not_above() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let far = elsewhere.path().join("far");
    let near = home.path().join("near");
    for project in [&far, &near] {
        fs::create_dir(project).unwrap();
        mind(project, &["init"]).unwrap();
    }
    weaver_at_home(elsewhere.path(), home.path(), &["link", "far"]).unwrap();
    weaver_at_home(home.path(), home.path(), &["link", "near"]).unwrap();
    let default =
        |project: &Path| match tui::load_project(project, &Lookup::at_home(Some(home.path())))
            .unwrap()
        {
            View::Workspace(workspace) => workspace.reason == Some(Reason::Home),
            View::Nowhere => panic!("no screen for {}", project.display()),
        };

    // Above `near`, the walk up finds it as it would any other workspace.
    assert!(default(&far));
    assert!(!default(&near));
}

/// The TUI `mindweaver` opens in `dir`, with nothing above it, where the default
/// workspace lives in `home`.
fn tui_at_home(dir: &Path, home: &Path) -> App {
    let lookup = Lookup::at_home(Some(home));
    App::new(dir.to_path_buf(), tui::load(dir, &lookup).unwrap()).with_lookup(lookup)
}

#[test]
fn with_the_default_workspace_not_made_the_tui_opens_on_it_empty() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();

    let app = tui_at_home(elsewhere.path(), home.path());

    let drawn = tui_drawn(&app);
    assert!(drawn.contains("default workspace, not made yet"), "{drawn}");
    assert!(drawn.contains("nothing on the shelf"), "{drawn}");
    assert!(drawn.contains("Projects (0)"), "{drawn}");
    assert!(drawn.contains("nothing here yet"), "{drawn}");
    // Where each kind goes, a kind named as a sentence names more than one.
    assert!(
        drawn.contains("skills go in .mindweaver/library/skills/"),
        "{drawn}"
    );
    assert!(
        drawn.contains("MCP servers go in .mindweaver/library/mcp/"),
        "{drawn}"
    );
    assert!(app
        .message
        .as_ref()
        .is_some_and(|message| message.text.contains("is made by the first change")));
    assert!(!home.path().join(WEAVER_DIR).exists());
}

#[test]
fn reading_from_the_tui_leaves_the_default_workspace_unmade() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let mut app = tui_at_home(elsewhere.path(), home.path());

    assert_eq!(
        press_and_run(&mut app, KeyCode::Char('s')),
        Some(Task::Shelf)
    );
    press_key(&mut app, KeyCode::Esc);
    press_and_run(&mut app, KeyCode::Char('v'));
    press_key(&mut app, KeyCode::Esc);
    assert_eq!(
        press_and_run(&mut app, KeyCode::Char('i')),
        Some(Task::OpenInstall)
    );

    assert_eq!(
        app.message.as_ref().map(|message| message.text.as_str()),
        Some("no projects to install into yet; l links one")
    );
    assert!(!home.path().join(WEAVER_DIR).exists());
    assert_eq!(app.transcript(), "");
}

#[test]
fn the_minds_screen_lists_what_is_under_the_default_workspace_before_it_is_made() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let alpha = home.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    mind(&alpha, &["init"]).unwrap();
    let mut app = tui_at_home(elsewhere.path(), home.path());

    press_key(&mut app, KeyCode::Char('m'));

    assert!(matches!(app.mode, Mode::Minds(_)), "{:?}", app.mode);
    assert!(tui_drawn(&app).contains("alpha"));
    assert!(!home.path().join(WEAVER_DIR).exists());
}

/// The paths the minds screen lists, in its order.
fn minds_rows(app: &App) -> Vec<&str> {
    let Mode::Minds(screen) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    screen.rows.iter().map(|row| row.path.as_str()).collect()
}

/// A home whose default workspace is not made yet, sitting inside a
/// workspace that is: `workspace_with_two`, with `h/gamma`, a mind project,
/// under it, and `h` the home.
fn home_inside_a_workspace() -> (TempDir, PathBuf) {
    let outer = workspace_with_two();
    let home = outer.path().join("h");
    let gamma = home.join("gamma");
    fs::create_dir_all(&gamma).unwrap();
    mind(&gamma, &["init"]).unwrap();
    (outer, home)
}

#[test]
fn weaver_minds_opens_on_the_default_workspace_without_making_it() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let alpha = home.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    mind(&alpha, &["init"]).unwrap();

    let app = tui::minds(elsewhere.path(), &[], &Lookup::at_home(Some(home.path()))).unwrap();

    assert_eq!(minds_rows(&app), vec!["alpha"]);
    assert!(!home.path().join(WEAVER_DIR).exists());
}

#[test]
fn weaver_minds_with_no_workspace_and_no_default_says_so() {
    let elsewhere = TempDir::new().unwrap();

    let error = tui::minds(elsewhere.path(), &[], &Lookup::default()).unwrap_err();

    assert!(matches!(error, CliError::NotInWorkspace(_)), "{error}");
}

#[test]
fn weaver_minds_opens_on_the_default_workspace_not_on_one_above_its_home() {
    let (_outer, home) = home_inside_a_workspace();
    let elsewhere = TempDir::new().unwrap();

    let app = tui::minds(elsewhere.path(), &[], &Lookup::at_home(Some(&home))).unwrap();

    // Not `alpha`, `beta` and `h/gamma`: those are the workspace above.
    assert_eq!(minds_rows(&app), vec!["gamma"]);
    assert_eq!(app.base(), home);
    assert!(!home.join(WEAVER_DIR).exists());
}

#[test]
fn the_install_screen_opens_on_the_default_workspace_not_on_one_above_its_home() {
    let (_outer, home) = home_inside_a_workspace();
    let elsewhere = TempDir::new().unwrap();
    let mut app = tui_at_home(elsewhere.path(), &home);

    press_and_run(&mut app, KeyCode::Char('i'));

    // The workspace above has `alpha` and `beta` to install into; the one on
    // screen has nothing yet.
    assert!(!matches!(app.mode, Mode::Installing(_)), "{:?}", app.mode);
    assert_eq!(
        app.message.as_ref().map(|message| message.text.as_str()),
        Some("no projects to install into yet; l links one")
    );
    assert!(!home.join(WEAVER_DIR).exists());
}

#[test]
fn the_first_change_from_the_tui_makes_the_default_workspace() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let alpha = home.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    mind(&alpha, &["init"]).unwrap();
    let mut app = tui_at_home(elsewhere.path(), home.path());

    press_key(&mut app, KeyCode::Char('l'));
    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(
        task,
        Some(Task::Link {
            path: String::from("alpha")
        })
    );
    assert!(registry(home.path()).contains("\"alpha\""));
    let View::Workspace(workspace) = &app.view else {
        panic!("{:?}", app.view)
    };
    assert!(workspace.made);
    assert_eq!(workspace.reason, Some(Reason::Home));
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("default workspace · "), "{drawn}");
    // Made by a line of its own, where the screen showed it.
    assert_eq!(
        app.transcript(),
        format!(
            "$ mindweaver init\ninitialized weaver workspace `{}` in {}\n\
             $ mindweaver link alpha\nlinked alpha as alpha\n",
            home.path().file_name().unwrap().to_string_lossy(),
            home.path().join(WEAVER_DIR).display()
        )
    );
}

#[test]
fn the_first_change_from_the_tui_lands_in_the_default_workspace_not_in_one_above_its_home() {
    let (outer, home) = home_inside_a_workspace();
    let before = registry(outer.path());
    let elsewhere = TempDir::new().unwrap();
    let mut app = tui_at_home(elsewhere.path(), &home);

    press_key(&mut app, KeyCode::Char('l'));
    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(
        task,
        Some(Task::Link {
            path: String::from("gamma")
        })
    );
    assert!(registry(&home).contains("\"gamma\""));
    assert_eq!(registry(outer.path()), before);
    let View::Workspace(workspace) = &app.view else {
        panic!("{:?}", app.view)
    };
    assert!(workspace.made);
    assert_eq!(workspace.reason, Some(Reason::Home));
    assert_eq!(workspace.root, home);
}

#[test]
fn a_gather_from_the_tui_runs_apart_after_making_the_default_workspace_where_it_is_shown() {
    let (outer, home) = home_inside_a_workspace();
    let elsewhere = TempDir::new().unwrap();
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let url = source.path().to_string_lossy().into_owned();
    let mut app = tui_at_home(elsewhere.path(), &home);

    press_key(&mut app, KeyCode::Char('g'));
    type_text(&mut app, &url);
    let TuiStep::Run(task) = press_key(&mut app, KeyCode::Enter) else {
        panic!("{:?}", app.mode);
    };
    assert!(task.runs_apart());
    tui::step(&mut app, task);
    tui::wait(&mut app);

    // Made first, by a line of its own, on the thread the gather ran on: left
    // to the gather, the walk up from home would have found the workspace
    // above it.
    let transcript = app.transcript();
    assert!(
        transcript.starts_with("$ mindweaver init\n"),
        "{transcript}"
    );
    assert!(
        transcript.contains("$ mindweaver gather git "),
        "{transcript}"
    );
    let workspace = app.workspace().unwrap();
    assert!(workspace.made);
    assert_eq!(workspace.root, home);
    assert_eq!(workspace.shelf, 1);
    let above = weaver(outer.path(), &["gather", "list"]).unwrap();
    assert!(
        above.stdout.starts_with("nothing gathered yet"),
        "{}",
        above.stdout
    );
}

#[test]
fn a_skill_that_cannot_be_read_is_listed_where_it_can_be_dealt_with() {
    let dir = workspace_with_two();
    write_skill(
        &dir.path().join("alpha"),
        "broken",
        "no front matter at all\n",
    );
    let mut app = tui_in(dir.path());

    let drawn = tui_drawn(&app);
    assert!(drawn.contains("broken"), "{drawn}");
    assert!(drawn.contains("cannot be read"), "{drawn}");
    assert!(
        drawn.contains("front matter fence"),
        "the reason too:\n{drawn}"
    );

    // alpha, then broken: the broken one sits where it would have been.
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Down);
    assert_eq!(app.current_item().unwrap().name, "broken");
    press_key(&mut app, KeyCode::Char('d'));
    press_and_run(&mut app, KeyCode::Char('y'));

    assert!(!dir.path().join("alpha/skills/broken").exists());
}

#[test]
fn a_projects_own_skill_that_cannot_be_read_takes_the_row_its_agents_copy_has() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let alpha = dir.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    let directories = mindweaver_core::Directories::default().with(Kind::Skill, "skills");
    MindProject::init_settings(
        &alpha,
        &mindweaver_core::Agent::ALL,
        mindweaver_core::Share::Shared,
        &directories,
    )
    .unwrap();
    weaver(dir.path(), &["link", "alpha"]).unwrap();
    write_skill(&alpha, "deploy", "no front matter at all\n");
    let claudes = alpha.join(".claude/skills/deploy");
    fs::create_dir_all(&claudes).unwrap();
    fs::write(claudes.join("SKILL.md"), skill_file("deploy", "Ship it")).unwrap();
    let mut app = tui_in(dir.path());

    // One row, the project's own, which is what `d` on it removes.
    press_key(&mut app, KeyCode::Right);
    let items = &app.member().unwrap().holdings.as_ref().unwrap().items;
    let rows: Vec<(&str, bool)> = items
        .iter()
        .map(|item| (item.name.as_str(), item.broken.is_some()))
        .collect();
    assert_eq!(rows, [("deploy", true)]);
    press_key(&mut app, KeyCode::Char('d'));
    press_and_run(&mut app, KeyCode::Char('y'));

    assert!(!alpha.join("skills/deploy").exists());
    assert!(claudes.join("SKILL.md").is_file());
}

#[test]
fn the_tui_lists_mcp_servers_as_rows_beside_skills_and_rules_at_both_levels() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let program = [
        "--command",
        "npx",
        "--arg",
        "-y",
        "--secret-env",
        "GITHUB_TOKEN",
    ];
    let mut line = vec!["add", "mcp", "github", "GitHub's issues"];
    line.extend(program);
    weaver(dir.path(), &line).unwrap();
    weaver(dir.path(), &["add", "skill", "deploy", "Ship it"]).unwrap();
    let alpha = dir.path().join("alpha");
    mindweaver_core::mother::ProjectMother::with_mcp(
        &alpha,
        &mindweaver_core::Agent::ALL,
        mindweaver_core::Share::Shared,
    );
    weaver(dir.path(), &["link", "alpha"]).unwrap();
    let mut line = vec!["add", "mcp", "sentry", "Sentry's issues"];
    line.extend([
        "--url",
        "https://mcp.sentry.dev/mcp",
        "--bearer-env",
        "SENTRY_TOKEN",
    ]);
    mind(&alpha, &line).unwrap();
    let mut app = tui_in(dir.path());

    let rows = |app: &App| -> Vec<(Kind, String)> {
        app.member()
            .unwrap()
            .holdings
            .as_ref()
            .unwrap()
            .items
            .iter()
            .map(|item| (item.kind, item.name.clone()))
            .collect()
    };
    assert_eq!(app.member().unwrap().name, "alpha");
    assert_eq!(rows(&app), [(Kind::Mcp, String::from("sentry"))]);
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("mcp   sentry"), "{drawn}");
    assert!(drawn.contains("Sentry's issues"), "{drawn}");

    press_key(&mut app, KeyCode::Up);
    assert!(app.member().unwrap().own);
    assert_eq!(
        rows(&app),
        [
            (Kind::Skill, String::from("deploy")),
            (Kind::Mcp, String::from("github")),
        ]
    );
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("mcp   github"), "{drawn}");
    assert!(drawn.contains("GitHub's issues"), "{drawn}");
}

// ---------------------------------------------------------------------------
// Both levels, in the TUI
// ---------------------------------------------------------------------------

#[test]
fn the_workspaces_own_row_sits_above_its_projects() {
    let dir = workspace_with_own();
    let mut app = tui_in(dir.path());

    // The cursor starts on the first project; the workspace's row is above.
    assert_eq!(app.member().unwrap().name, "alpha");
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("Projects (2)"), "{drawn}");
    assert!(drawn.contains("◆"), "{drawn}");

    press_key(&mut app, KeyCode::Up);
    assert!(app.member().unwrap().own);
    assert!(tui_drawn(&app).contains("the workspace's own"));
    press_key(&mut app, KeyCode::Right);
    let step = press_key(&mut app, KeyCode::Char('e'));

    let tui::state::Step::Run(task) = step else {
        panic!("{step:?}");
    };
    assert_eq!(task.describe(), "mindweaver edit skill:commit-style");
}

#[test]
fn d_on_the_workspaces_own_offers_to_remove_it_everywhere_with_e() {
    let dir = workspace_with_own();
    weaver(dir.path(), &["install", "commit-style", "-p", "alpha"]).unwrap();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Up);
    press_key(&mut app, KeyCode::Right);

    press_key(&mut app, KeyCode::Char('d'));
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("Delete skill commit-style?"), "{drawn}");
    assert!(
        drawn.contains("runs  mindweaver remove --yes skill:commit-style"),
        "{drawn}"
    );
    assert!(
        drawn.contains("e     mindweaver remove --everywhere --yes skill:commit-style"),
        "{drawn}"
    );
    assert!(drawn.contains("e  everywhere"), "{drawn}");
    let task = press_and_run(&mut app, KeyCode::Char('e'));

    assert_eq!(
        task,
        Some(Task::Remove {
            holder: Holder::Workspace,
            kind: Kind::Skill,
            name: String::from("commit-style"),
            everywhere: true,
            yes: true,
        })
    );
    assert!(!own_directory_for(dir.path(), Kind::Skill)
        .join("commit-style")
        .exists());
    for copy in copies(&dir.path().join("alpha"), "commit-style") {
        assert!(!copy.exists(), "{}", copy.display());
    }
    assert!(app.transcript().contains(
        "$ mindweaver remove --everywhere --yes skill:commit-style\nremoved commit-style from alpha\n"
    ));
}

#[test]
fn d_on_a_projects_own_has_no_everywhere_to_offer() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Right);

    press_key(&mut app, KeyCode::Char('d'));
    let drawn = tui_drawn(&app);
    assert!(!drawn.contains("everywhere"), "{drawn}");

    // So `e` there is no answer, and the question stays.
    assert_eq!(press_key(&mut app, KeyCode::Char('e')), TuiStep::Stay);
    assert!(matches!(app.mode, Mode::Confirming(_)));
}

#[test]
fn adding_on_the_workspaces_row_is_weaver_add() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Up);

    press_key(&mut app, KeyCode::Char('a'));
    type_text(&mut app, "commit-style");
    press_key(&mut app, KeyCode::Tab);
    type_text(&mut app, "Shared");
    press_and_run(&mut app, KeyCode::Enter);

    assert!(own_directory_for(dir.path(), Kind::Skill)
        .join("commit-style/SKILL.md")
        .is_file());
    assert!(app
        .transcript()
        .contains("$ mindweaver add skill commit-style Shared\n"));
    assert_eq!(app.current_item().unwrap().name, "commit-style");
}

#[test]
fn the_workspaces_row_has_nothing_to_unlink() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Up);

    assert_eq!(
        press_key(&mut app, KeyCode::Char('U')),
        tui::state::Step::Stay
    );
    assert!(matches!(app.mode, Mode::Browse));
    assert!(tui_drawn(&app).contains("nothing to unlink"));
}

#[test]
fn the_install_screen_offers_the_workspaces_own_skills() {
    let dir = workspace_with_own();
    let mut app = tui_in(dir.path());

    press_and_run(&mut app, KeyCode::Char('i'));

    let drawn = tui_drawn(&app);
    assert!(drawn.contains("commit-style"), "{drawn}");
    assert!(drawn.contains("[workspace]"), "{drawn}");
}

#[test]
fn mind_opens_a_managed_project_inside_its_workspace() {
    let dir = workspace_with_two();

    let app = mind_tui_in(&dir.path().join("beta"));

    assert_eq!(app.scope(), Scope::Workspace);
    assert_eq!(app.member().unwrap().name, "beta");
    assert_eq!(app.focus, tui::state::Focus::Items);
}

#[test]
fn mind_opens_a_project_on_its_own_with_what_only_a_workspace_has_explained() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    mind(dir.path(), &["add", "skill", "deploy", "Ship it"]).unwrap();
    let mut app = mind_tui_in(dir.path());

    assert_eq!(app.scope(), Scope::Project);
    assert!(tui_drawn(&app).contains("a project no workspace manages"));
    for key in ['g', 's', 'i', 'u', 'U'] {
        assert_eq!(
            press_key(&mut app, KeyCode::Char(key)),
            tui::state::Step::Stay
        );
    }
    let tui::state::Step::Run(task) = press_key(&mut app, KeyCode::Char('v')) else {
        panic!("v runs a command");
    };
    assert_eq!(task.describe(), "mind validate");

    press_key(&mut app, KeyCode::Char('a'));
    type_text(&mut app, "review");
    press_key(&mut app, KeyCode::Tab);
    type_text(&mut app, "Review it");
    press_and_run(&mut app, KeyCode::Enter);
    assert!(dir.path().join("skills/review/SKILL.md").is_file());
    assert!(app
        .transcript()
        .contains("$ mind add skill review 'Review it'\n"));
}

#[test]
fn l_on_a_loose_project_links_it_and_the_screen_becomes_the_workspaces() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let alpha = dir.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    mind(&alpha, &["init"]).unwrap();
    let mut app = mind_tui_in(&alpha);
    assert_eq!(app.scope(), Scope::Project);

    let task = press_and_run(&mut app, KeyCode::Char('l'));

    assert_eq!(task, Some(Task::LinkHere));
    assert!(registry(dir.path()).contains("\"alpha\""));
    assert_eq!(app.scope(), Scope::Workspace);
    assert_eq!(app.member().unwrap().name, "alpha");
    assert!(app
        .transcript()
        .starts_with("$ mind link\nlinked alpha as alpha\n"));
}

#[test]
fn mind_with_no_project_offers_to_make_one() {
    let dir = TempDir::new().unwrap();
    let mut app = mind_tui_in(dir.path());
    assert!(matches!(app.view, View::Nowhere));
    assert!(tui_drawn(&app).contains("No mind project here"));

    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(task, Some(Task::InitProject));
    assert!(dir.path().join(MIND_DIR).join(SETTINGS_FILE).is_file());
    assert_eq!(app.scope(), Scope::Project);
}

// ---------------------------------------------------------------------------
// A gather that takes its time
// ---------------------------------------------------------------------------

/// The URL the slow gather is asked for, which it never fetches.
const SLOW: &str = "https://example.com/skills.git";

/// A stand-in for `mindweaver gather git` that takes as long as the test wants:
/// it reports reading each of three folders, and goes on from one to the
/// next only when `go` says so. A cancel stops it before it says it is done,
/// as one stops a real gather before anything is placed.
fn slow_gather(go: mpsc::Receiver<()>) -> Runner {
    let go = Mutex::new(go);
    Runner::new(move |words, _, _, watch| {
        let go = go.lock().unwrap();
        let command = tui::state::shell(words);
        for done in 0..=3 {
            watch.report(Progress::Gather(Gather::Harvesting { done, total: 3 }));
            loop {
                if watch.is_cancelled() {
                    return Done::failed(command, "cancelled, with nothing put on the shelf");
                }
                match go.recv_timeout(Duration::from_millis(5)) {
                    // A test that has gone is not waited for.
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => {}
                }
            }
        }
        Done {
            command,
            stdout: String::from("3 skills: 3 added\n"),
            stderr: Vec::new(),
            error: None,
            ok: true,
        }
    })
}

/// A stand-in for a gather past its last check: it says it is placing
/// three, and finishes when `go` says so, whatever cancel comes first, as a
/// real gather does once it has started placing.
fn placing_gather(go: mpsc::Receiver<()>) -> Runner {
    let go = Mutex::new(go);
    Runner::new(move |words, _, _, watch| {
        let go = go.lock().unwrap();
        watch.report(Progress::Gather(Gather::Placing { done: 0, total: 3 }));
        // A test that has gone is not waited for.
        let _ = go.recv();
        watch.report(Progress::Gather(Gather::Placing { done: 3, total: 3 }));
        Done {
            command: tui::state::shell(words),
            stdout: String::from("3 skills: 3 added\n"),
            stderr: Vec::new(),
            error: None,
            ok: true,
        }
    })
}

/// Ask for the slow gather from the gather form, and let it get as far as
/// reading its first folder.
fn start_slow_gather(app: &mut App) {
    start_gather(app, Progress::Gather(Gather::Harvesting { done: 0, total: 3 }));
}

/// Ask for a gather from the gather form, and let it get as far as `step`.
fn start_gather(app: &mut App, step: Progress) {
    press_key(app, KeyCode::Char('g'));
    type_text(app, SLOW);
    let TuiStep::Run(task) = press_key(app, KeyCode::Enter) else {
        panic!("{:?}", app.mode);
    };
    tui::step(app, task);
    settle_until(app, |app| reported(app) == Some(step));
}

/// The step the task running apart last reported, as the screen has it.
fn reported(app: &App) -> Option<Progress> {
    app.running.as_ref().and_then(|running| running.progress)
}

/// Take in what the task running apart says, as the loop does before each
/// frame, until `until` holds.
fn settle_until(app: &mut App, until: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !until(app) {
        assert!(Instant::now() < deadline, "never came: {:?}", app.running);
        tui::settle(app);
        thread::sleep(Duration::from_millis(1));
    }
}

// ---------------------------------------------------------------------------
// A workspace named for the TUI
// ---------------------------------------------------------------------------

/// Under one directory: `outer`, a workspace, and `hub` beside it, which
/// links `collapse`, a project under neither, and has installed its own
/// `hub-skill` there. The project is made as an older Mindweaver made it,
/// so the copy is in the folder it authors in, where a line run on it
/// reaches it and drops the record of the workspace it consulted.
fn hub_beside_outer() -> (TempDir, PathBuf, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let (outer, hub, collapse) = (
        dir.path().join("outer"),
        dir.path().join("hub"),
        dir.path().join("collapse"),
    );
    for root in [&outer, &hub, &collapse] {
        fs::create_dir(root).unwrap();
    }
    weaver(&outer, &["init"]).unwrap();
    weaver(&hub, &["init"]).unwrap();
    older_project(&collapse);
    weaver(&hub, &["link", "../collapse"]).unwrap();
    weaver(&hub, &["add", "skill", "hub-skill", "The hub's"]).unwrap();
    weaver(&hub, &["install", "-p", "collapse", "hub-skill"]).unwrap();
    (dir, outer, hub, collapse)
}

/// What `hub`'s ledger says it installed into `collapse`.
fn recorded_in_hub(hub: &Path) -> Vec<String> {
    WeaverWorkspace::open(hub)
        .unwrap()
        .ledger()
        .unwrap()
        .installations("../collapse", Kind::Skill)
        .unwrap()
}

#[test]
fn opened_on_a_named_workspace_the_tui_runs_every_line_against_it() {
    let (_dir, outer, hub, collapse) = hub_beside_outer();
    let lookup = Lookup {
        flag: Some(hub.clone()),
        ..Lookup::default()
    };
    // Opened inside `outer`, which walking up would find instead.
    let mut app = App::new(outer.clone(), tui::load(&outer, &lookup).unwrap()).with_lookup(lookup);
    assert_eq!(app.base(), hub);

    // `collapse`, then its one skill.
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Char('d'));
    press_and_run(&mut app, KeyCode::Char('y'));

    // The line consulted the workspace on screen, not whatever is above the
    // project or in a home, so that workspace's record went with the copy.
    assert!(
        app.transcript()
            .starts_with("$ mind -C ../collapse remove --yes skill:hub-skill\n"),
        "{}",
        app.transcript()
    );
    assert!(!collapse.join("skills/hub-skill").exists());
    assert!(recorded_in_hub(&hub).is_empty());
}

#[test]
fn the_header_says_why_the_workspace_on_screen_is_the_one_shown() {
    let (_dir, outer, hub, _collapse) = hub_beside_outer();
    let drawn = |lookup: Lookup| {
        let view = tui::load(&outer, &lookup).unwrap();
        tui_drawn(&App::new(outer.clone(), view).with_lookup(lookup))
    };

    let found = drawn(Lookup::default());
    let named = drawn(Lookup {
        flag: Some(hub.clone()),
        ..Lookup::default()
    });
    let activated = drawn(Lookup {
        activated: Some(hub.clone()),
        ..Lookup::default()
    });

    let header = |drawn: &str| drawn.lines().next().unwrap_or_default().to_owned();
    // The hub links collapse, which an older Mindweaver made, so what the
    // header says of that comes between the hub's name and why it is shown.
    assert!(
        header(&found).contains("outer  · found above · "),
        "{found}"
    );
    assert!(
        header(&named).contains(" hub  · ") && header(&named).contains("  · named with -W · "),
        "{named}"
    );
    assert!(
        header(&activated).contains(" hub  · ") && header(&activated).contains("  · activated · "),
        "{activated}"
    );
}

#[test]
fn the_header_of_a_project_on_its_own_gives_no_reason() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();

    let drawn = tui_drawn(&mind_tui_in(dir.path()));

    let header = drawn.lines().next().unwrap_or_default();
    assert!(
        header.contains("· a project no workspace manages  · "),
        "{drawn}"
    );
}

#[test]
fn the_header_flags_projects_a_mind_toml_marks_with_the_command_that_moves_them() {
    let dir = workspace_with_two();
    let header = |dir: &Path| {
        tui_drawn(&tui_in(dir))
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned()
    };
    assert!(
        !header(dir.path()).contains("mind.toml"),
        "{}",
        header(dir.path())
    );

    for name in ["old", "older"] {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        older_project(&root);
        weaver(dir.path(), &["link", name]).unwrap();
    }

    let flagged = header(dir.path());
    assert!(
        flagged.contains(
            "  · 2 projects marked by mind.toml: `mindweaver migrate --all-projects`  · "
        ),
        "{flagged}"
    );
    // A project on its own is told what moves it from where it is.
    let alone = TempDir::new().unwrap();
    older_project(alone.path());
    let own = tui_drawn(&mind_tui_in(alone.path()));
    assert!(
        own.lines()
            .next()
            .unwrap_or_default()
            .contains("  · marked by mind.toml: `mind migrate`  · "),
        "{own}"
    );

    weaver(dir.path(), &["migrate", "--all-projects", "--yes"]).unwrap();
    assert!(
        !header(dir.path()).contains("mind.toml"),
        "{}",
        header(dir.path())
    );
}

#[test]
fn the_header_flags_projects_a_settings_json_marks_as_it_flags_a_mind_toml() {
    let dir = workspace_with_two();
    let header = |dir: &Path| {
        tui_drawn(&tui_in(dir))
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned()
    };
    let json = dir.path().join("json");
    fs::create_dir(&json).unwrap();
    json_project(&json);
    weaver(dir.path(), &["link", "json"]).unwrap();

    let flagged = header(dir.path());
    assert!(
        flagged.contains(
            "  · 1 project marked by settings.json: `mindweaver migrate --all-projects`  · "
        ),
        "{flagged}"
    );

    // Both kinds, counted together and each named once, the line cut at the
    // screen's edge.
    let old = dir.path().join("old");
    fs::create_dir(&old).unwrap();
    older_project(&old);
    weaver(dir.path(), &["link", "old"]).unwrap();
    let both = header(dir.path());
    assert!(
        both.contains(
            "  · 2 projects marked by mind.toml or settings.json: \
             `mindweaver migrate --all-projects"
        ),
        "{both}"
    );

    // A project on its own is told what moves it from where it is.
    let alone = TempDir::new().unwrap();
    json_project(alone.path());
    let own = tui_drawn(&mind_tui_in(alone.path()));
    assert!(
        own.lines()
            .next()
            .unwrap_or_default()
            .contains("  · marked by settings.json: `mind migrate`  · "),
        "{own}"
    );

    weaver(dir.path(), &["migrate", "--all-projects", "--yes"]).unwrap();
    let moved = header(dir.path());
    assert!(!moved.contains("marked by"), "{moved}");
}

#[test]
fn a_w_among_a_lines_words_is_typed_from_the_workspace_root() {
    let (_dir, outer, _hub, _collapse) = hub_beside_outer();
    let words: Vec<String> = ["mindweaver", "-W", "../hub", "list"]
        .iter()
        .map(|word| (*word).to_owned())
        .collect();

    let done = tui::execute(&words, &outer, &Lookup::default(), &Watch::new());

    assert_eq!(done.error, None);
    assert!(done.stdout.contains("hub-skill"), "{}", done.stdout);
}

// ---------------------------------------------------------------------------
// A project the workspace on screen manages from elsewhere
// ---------------------------------------------------------------------------

#[test]
fn a_project_linked_from_beside_runs_its_lines_against_the_workspace_on_screen() {
    let (_dir, _outer, hub, collapse) = hub_beside_outer();
    // Nothing named: the screen is the workspace found where it was opened.
    let mut app = tui_in(&hub);

    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Char('d'));
    let drawn = tui_drawn(&app);
    assert!(
        drawn.contains("mind -W . -C ../collapse remove --yes skill:hub-skill"),
        "{drawn}"
    );
    press_and_run(&mut app, KeyCode::Char('y'));

    // Without `-W .`, walking up from `collapse` finds no workspace at all,
    // and the hub would keep a record of a copy that is gone.
    assert!(!collapse.join("skills/hub-skill").exists());
    assert!(recorded_in_hub(&hub).is_empty());
}

#[test]
fn a_project_linked_from_beside_is_offered_the_screens_templates() {
    let (_dir, _outer, hub, collapse) = hub_beside_outer();
    let template = hub.join(".mindweaver/templates/runbook/SKILL.md");
    fs::create_dir_all(template.parent().unwrap()).unwrap();
    fs::write(&template, "---\n---\n\n# {{name}}\n\nThe hub's runbook.\n").unwrap();
    let mut app = tui_in(&hub);

    press_key(&mut app, KeyCode::Char('a'));
    press_key(&mut app, KeyCode::Up);
    // none, then full (built in), then runbook.
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Down);
    type_text(&mut app, "deploy");
    press_key(&mut app, KeyCode::Down);
    type_text(&mut app, "Ship it");
    assert!(
        tui_drawn(&app).contains("mind -W . -C ../collapse add --template runbook"),
        "{}",
        tui_drawn(&app)
    );
    press_and_run(&mut app, KeyCode::Enter);

    let manifest = fs::read_to_string(collapse.join("skills/deploy/SKILL.md")).unwrap();
    assert!(manifest.contains("The hub's runbook."), "{manifest}");
}

#[test]
fn the_keys_help_says_which_rows_run_their_lines_with_w() {
    let (_dir, _outer, hub, _collapse) = hub_beside_outer();
    let mut app = tui_in(&hub);

    press_key(&mut app, KeyCode::Char('?'));

    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert!(
        reader
            .lines
            .iter()
            .any(|(_, line)| line.contains("`mind -W . -C <project> <cmd>`")),
        "{:?}",
        reader.lines
    );
}

#[test]
fn a_project_under_the_workspace_on_screen_keeps_the_plain_line() {
    let dir = workspace_with_two();
    let app = tui_in(dir.path());

    assert_eq!(
        app.member().map(|member| member.holder()),
        Some(Holder::Project(String::from("alpha")))
    );
}

// ---------------------------------------------------------------------------
// A project's agents and sharing
// ---------------------------------------------------------------------------

#[test]
fn t_on_a_project_opens_its_agents_and_share_toggles_and_runs_the_command() {
    let dir = workspace_with_own();
    weaver(dir.path(), &["install", "-p", "alpha", "commit-style"]).unwrap();
    let mut app = tui_in(dir.path());
    assert_eq!(app.member().unwrap().name, "alpha");

    press_key(&mut app, KeyCode::Char('t'));
    let Mode::Settings(form) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    // Ticked as the project's settings say: `mind init` names every agent.
    assert!(form.ticked.iter().all(|(_, ticked)| *ticked));
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("Agents and sharing for alpha"), "{drawn}");
    assert!(drawn.contains("[local]"), "{drawn}");

    // Untick Claude: the form shows the line it would run, as typed.
    press_key(&mut app, KeyCode::Char(' '));
    let drawn = tui_drawn(&app);
    assert!(
        drawn.contains("mind -C alpha agents --yes codex opencode"),
        "{drawn}"
    );

    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(
        task,
        Some(Task::Settings {
            holder: Holder::Project(String::from("alpha")),
            agents: Some(AgentsLine::Named(vec![Agent::Codex, Agent::OpenCode])),
            share: None,
            edit_tracked: false,
        })
    );
    // What it said, to read in full: the copy Claude read is gone.
    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    let said: Vec<&str> = reader.lines.iter().map(|(_, line)| line.as_str()).collect();
    assert!(
        said.contains(&"removed commit-style from .claude/skills/commit-style"),
        "{said:?}"
    );
    let alpha = dir.path().join("alpha");
    assert!(!alpha.join(".claude/skills/commit-style").exists());
    assert_eq!(
        MindProject::open(&alpha)
            .unwrap()
            .settings()
            .unwrap()
            .agents,
        [Agent::Codex, Agent::OpenCode]
    );
    assert!(app
        .transcript()
        .starts_with("$ mind -C alpha agents --yes codex opencode\n"));
}

#[test]
fn the_share_toggle_runs_mind_share_and_nothing_toggled_runs_nothing() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Char('t'));

    // Applied as it opened, there is nothing to run.
    assert_eq!(press_key(&mut app, KeyCode::Enter), TuiStep::Stay);
    assert!(tui_drawn(&app).contains("nothing toggled yet"));

    for _ in 0..Agent::ALL.len() {
        press_key(&mut app, KeyCode::Down);
    }
    press_key(&mut app, KeyCode::Right);
    let task = press_and_run(&mut app, KeyCode::Enter).unwrap();

    assert_eq!(task.describe(), "mind -C alpha share --yes shared");
    assert_eq!(
        MindProject::open(dir.path().join("alpha"))
            .unwrap()
            .settings()
            .unwrap()
            .share,
        Share::Shared
    );
}

#[test]
fn unticking_every_agent_is_refused_by_the_command_in_the_form() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Char('t'));
    for _ in 0..Agent::ALL.len() {
        press_key(&mut app, KeyCode::Char(' '));
        press_key(&mut app, KeyCode::Down);
    }

    let task = press_and_run(&mut app, KeyCode::Enter).unwrap();

    assert_eq!(
        task.describe(),
        "mind -C alpha agents remove --yes claude codex opencode"
    );
    // The command's own refusal, in the form, where it can be put right.
    let Mode::Settings(form) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    let error = form.error.as_deref().unwrap_or_default();
    assert!(error.contains("would name no agent"), "{error}");
}

#[test]
fn t_on_the_workspaces_own_row_or_an_older_project_says_why_not() {
    let dir = workspace_with_two();
    let old = dir.path().join("old");
    fs::create_dir(&old).unwrap();
    older_project(&old);
    weaver(dir.path(), &["link", "old"]).unwrap();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Up);
    assert!(app.member().unwrap().own);
    press_key(&mut app, KeyCode::Char('t'));
    assert!(matches!(app.mode, Mode::Browse));
    assert!(app
        .message
        .as_ref()
        .unwrap()
        .text
        .contains("reach agents by being installed"));

    while app.member().unwrap().name != "old" {
        press_key(&mut app, KeyCode::Down);
    }
    press_key(&mut app, KeyCode::Char('t'));
    assert!(matches!(app.mode, Mode::Browse));
    assert!(app
        .message
        .as_ref()
        .unwrap()
        .text
        .contains("`mind migrate`"));
}

#[test]
fn t_and_capital_t_on_a_project_a_settings_json_marks_say_why_not() {
    let dir = workspace_with_two();
    let json = dir.path().join("json");
    fs::create_dir(&json).unwrap();
    json_project(&json);
    weaver(dir.path(), &["link", "json"]).unwrap();
    let mut app = tui_in(dir.path());
    while app.member().unwrap().name != "json" {
        press_key(&mut app, KeyCode::Down);
    }

    for key in ['t', 'T'] {
        press_key(&mut app, KeyCode::Char(key));

        assert!(matches!(app.mode, Mode::Browse), "{key}: {:?}", app.mode);
        let said = &app.message.as_ref().unwrap().text;
        assert_eq!(
            said,
            "json is marked by a settings.json, which is read as it was written and not \
             changed; `mind migrate` moves it to a settings.toml",
            "{key}"
        );
    }
}

// ---------------------------------------------------------------------------
// Where a project installs skills
// ---------------------------------------------------------------------------

#[test]
fn a_project_installing_where_no_agent_reads_is_marked_and_its_pane_says_why() {
    let dir = workspace_with_own();
    let alpha = dir.path().join("alpha");
    mind(&alpha, &["targets", "set", "skills", ".skills", "-y"]).unwrap();
    weaver(dir.path(), &["install", "-p", "alpha", "commit-style"]).unwrap();

    let app = tui_in(dir.path());

    let holdings = app.member().unwrap().holdings.as_ref().unwrap();
    assert_eq!(
        holdings.unreached,
        [String::from(
            "claude, codex and opencode read none of the folders alpha installs skills in \
             (.skills), so none of them sees what is installed there until pointed at one"
        )]
    );
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("alpha  2  ⚠"), "{drawn}");
    assert!(drawn.contains("Not seen by every agent"), "{drawn}");
    // Wrapped whole, to the end that says what to do.
    assert!(drawn.contains("│one "), "{drawn}");
    // Its neighbour installs where its agents read, and is not marked.
    assert!(drawn.contains("beta  1 "), "{drawn}");
    assert!(!drawn.contains("beta  1  ⚠"), "{drawn}");
}

#[test]
fn a_project_given_a_rule_some_of_its_agents_read_nowhere_is_marked_and_its_pane_says_so() {
    let dir = workspace_with_own();
    let scoped = own_directory_for(dir.path(), Kind::Rule).join("style/comments.md");
    fs::create_dir_all(scoped.parent().unwrap()).unwrap();
    fs::write(
        &scoped,
        "---\npaths:\n  - \"**/*.rs\"\n---\n\n# Comments say why, not what\n",
    )
    .unwrap();
    weaver(dir.path(), &["install", "-p", "alpha", "style/comments"]).unwrap();

    let app = tui_in(dir.path());

    let holdings = app.member().unwrap().holdings.as_ref().unwrap();
    assert_eq!(
        holdings.unreached,
        ["codex and opencode do not read rule `style/comments` in alpha"]
    );
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("⚠"), "{drawn}");
    assert!(drawn.contains("Not seen by every agent"), "{drawn}");
}

#[test]
fn a_project_doctor_finds_something_in_is_marked_with_how_much_and_its_pane_says_what() {
    let dir = workspace_with_own();
    let stage = dir.path().join("alpha/.mind/tmp/4242-00112233445566ff");
    fs::create_dir_all(&stage).unwrap();

    let app = tui_in(dir.path());

    let holdings = app.member().unwrap().holdings.as_ref().unwrap();
    assert_eq!(holdings.found.len(), 1, "{:?}", holdings.found);
    assert!(
        holdings.found[0].starts_with(&format!("{}: ", stage.display())),
        "{:?}",
        holdings.found
    );
    assert!(
        holdings.found[0].contains("; fix: "),
        "{:?}",
        holdings.found
    );
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("✚ 1"), "{drawn}");
    assert!(drawn.contains("Found by doctor"), "{drawn}");
}

#[test]
fn what_doctor_could_not_read_to_tell_is_said_beside_what_could_not_be_read() {
    let dir = workspace_with_own();
    let codex = dir.path().join("home/.codex");
    fs::create_dir_all(&codex).unwrap();
    let config = codex.join("config.toml");
    fs::write(&config, "[projects\n").unwrap();
    let lookup = Lookup {
        agents: AgentHomes {
            codex_home: Some(codex),
            ..AgentHomes::default()
        },
        ..Lookup::default()
    };

    let view = tui::load(dir.path(), &lookup).unwrap();
    let app = App::new(dir.path().to_path_buf(), view).with_lookup(lookup);

    let member = app.member().unwrap();
    let holdings = member.holdings.as_ref().unwrap();
    let said: Vec<&String> = holdings
        .warnings
        .iter()
        .filter(|line| line.starts_with(&format!("{}: ", config.display())))
        .collect();
    assert_eq!(said.len(), 1, "{:?}", holdings.warnings);
    assert!(
        member.problems() > 0,
        "a check left unmade is no clean bill"
    );
    assert!(tui_drawn(&app).contains("Could not be read"));
}

#[test]
fn a_project_no_workspace_links_is_marked_for_what_doctor_finds_in_it_too() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let project = elsewhere.path().join("collapse");
    fs::create_dir(&project).unwrap();
    new_project(&project);
    let stage = project.join(".mind/tmp/4242-00112233445566ff");
    fs::create_dir_all(&stage).unwrap();

    let View::Workspace(workspace) =
        tui::load_project(&project, &Lookup::at_home(Some(home.path()))).unwrap()
    else {
        panic!("no screen for {}", project.display());
    };

    let holdings = workspace.members[0].holdings.as_ref().unwrap();
    assert_eq!(holdings.found.len(), 1, "{:?}", holdings.found);
    assert!(
        holdings.found[0].starts_with(&format!("{}: ", stage.display())),
        "{:?}",
        holdings.found
    );
}

#[test]
fn a_strip_wraps_by_how_wide_its_words_are_drawn() {
    let dir = workspace_with_own();
    let alpha = dir.path().join("alpha");
    // Each character two columns wide, so a line of as many characters as
    // the strip has columns would be cut at its border.
    let folder = format!(".{}", "技".repeat(16));
    mind(&alpha, &["targets", "set", "skills", &folder, "-y"]).unwrap();
    let app = tui_in(dir.path());

    for width in [80, 100, 110] {
        let mut terminal = Terminal::new(TestBackend::new(width, 30)).unwrap();
        terminal.draw(|frame| tui::ui::draw(&app, frame)).unwrap();
        let drawn: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        // Once in the strip of what its agents do not see, and twice in
        // doctor's, by the folder's path and among the folders.
        assert_eq!(drawn.matches('技').count(), 48, "{width}: {drawn}");
        assert!(drawn.contains("until"), "{width}: {drawn}");
        assert!(drawn.contains("pointed"), "{width}: {drawn}");
    }
}

#[test]
fn a_strip_is_drawn_on_a_terminal_too_short_for_it() {
    let dir = workspace_with_own();
    let alpha = dir.path().join("alpha");
    mind(&alpha, &["targets", "set", "skills", ".skills", "-y"]).unwrap();
    let app = tui_in(dir.path());
    assert!(!app
        .member()
        .unwrap()
        .holdings
        .as_ref()
        .unwrap()
        .unreached
        .is_empty());

    // Its pane shorter than the three rows a strip takes at least, which
    // must not end the process.
    for height in 1..10 {
        let mut terminal = Terminal::new(TestBackend::new(110, height)).unwrap();
        terminal.draw(|frame| tui::ui::draw(&app, frame)).unwrap();
    }
}

#[test]
fn capital_t_on_a_project_opens_where_it_installs_skills_and_runs_targets_set() {
    let dir = workspace_with_own();
    weaver(dir.path(), &["install", "-p", "alpha", "commit-style"]).unwrap();
    let mut app = tui_in(dir.path());
    assert_eq!(app.member().unwrap().name, "alpha");

    press_key(&mut app, KeyCode::Char('T'));
    let Mode::Targets(form) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    // Empty: the project installs where its agents read.
    assert_eq!(form.folders.text(), "");
    assert!(tui_drawn(&app).contains("Where alpha installs skills"));

    type_text(&mut app, ".skills");
    let drawn = tui_drawn(&app);
    assert!(
        drawn.contains("mind -C alpha targets set --yes skills .skills"),
        "{drawn}"
    );
    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(
        task,
        Some(Task::Targets {
            holder: Holder::Project(String::from("alpha")),
            kind: Kind::Skill,
            folders: Some(vec![String::from(".skills")]),
        })
    );
    // What it said, to read in full: the copies moved.
    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    let said: Vec<&str> = reader.lines.iter().map(|(_, line)| line.as_str()).collect();
    assert!(
        said.contains(&"removed commit-style from .claude/skills/commit-style"),
        "{said:?}"
    );
    let alpha = dir.path().join("alpha");
    assert!(alpha.join(".skills/commit-style/SKILL.md").is_file());
    assert!(app
        .transcript()
        .starts_with("$ mind -C alpha targets set --yes skills .skills\n"));

    // Back on the project, its row and its pane say no agent sees it.
    press_key(&mut app, KeyCode::Esc);
    let holdings = app.member().unwrap().holdings.as_ref().unwrap();
    assert_eq!(holdings.targets, Some(vec![String::from(".skills")]));
    assert_eq!(
        holdings.unreached,
        [String::from(
            "claude, codex and opencode read none of the folders alpha installs skills in \
             (.skills), so none of them sees what is installed there until pointed at one"
        )]
    );
    assert!(tui_drawn(&app).contains("alpha  2  ⚠"));
}

#[test]
fn clearing_the_folders_runs_targets_reset_and_leaving_them_runs_nothing() {
    let dir = workspace_with_own();
    mind(
        &dir.path().join("alpha"),
        &["targets", "set", "skills", ".skills", "-y"],
    )
    .unwrap();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Char('T'));
    let Mode::Targets(form) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    assert_eq!(form.folders.text(), ".skills");

    // As it opened, there is nothing to run.
    assert_eq!(press_key(&mut app, KeyCode::Enter), TuiStep::Stay);
    assert!(tui_drawn(&app).contains("nothing changed yet"));

    for _ in 0..".skills".len() {
        press_key(&mut app, KeyCode::Backspace);
    }
    let task = press_and_run(&mut app, KeyCode::Enter).unwrap();

    assert_eq!(task.describe(), "mind -C alpha targets reset --yes skills");
    let settings = MindProject::open(dir.path().join("alpha")).unwrap();
    assert!(settings.settings().unwrap().targets.is_empty());
}

#[test]
fn a_folder_no_install_may_use_is_refused_by_the_command_in_the_form() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Char('T'));
    type_text(&mut app, "../shared");

    press_and_run(&mut app, KeyCode::Enter).unwrap();

    let Mode::Targets(form) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    let error = form.error.as_deref().unwrap_or_default();
    assert!(error.contains("is not inside the project"), "{error}");
}

#[test]
fn capital_t_on_the_workspaces_own_row_or_an_older_project_says_why_not() {
    let dir = workspace_with_two();
    let old = dir.path().join("old");
    fs::create_dir(&old).unwrap();
    older_project(&old);
    weaver(dir.path(), &["link", "old"]).unwrap();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Up);
    assert!(app.member().unwrap().own);
    press_key(&mut app, KeyCode::Char('T'));
    assert!(matches!(app.mode, Mode::Browse));
    assert!(app
        .message
        .as_ref()
        .unwrap()
        .text
        .contains("reach agents by being installed"));

    while app.member().unwrap().name != "old" {
        press_key(&mut app, KeyCode::Down);
    }
    press_key(&mut app, KeyCode::Char('T'));
    assert!(matches!(app.mode, Mode::Browse));
    assert!(app
        .message
        .as_ref()
        .unwrap()
        .text
        .contains("`mind migrate`"));
}

#[test]
fn w_lists_the_workspaces_this_machine_knows_with_the_one_on_screen_marked() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();
    weaver_at_home(&home, &home, &["init"]).unwrap();
    weaver_at_home(&home, &home, &["init", "weavers/writing"]).unwrap();
    let writing = home.join("weavers/writing");
    let mut app = tui_at_home(&writing, &home);

    assert_eq!(
        press_and_run(&mut app, KeyCode::Char('w')),
        Some(Task::Workspaces)
    );

    let drawn = tui_drawn(&app);
    assert!(drawn.contains(" mindweaver workspaces "), "{drawn}");
    let default = format!(
        "  home     {}                  no projects; the default one",
        home.display()
    );
    let in_use = format!("* writing  {}  no projects", writing.display());
    assert!(drawn.contains(&default), "{drawn}");
    assert!(drawn.contains(&in_use), "{drawn}");
    assert!(app.transcript().is_empty(), "a read is no change to print");

    press_key(&mut app, KeyCode::Esc);
    assert!(matches!(app.mode, Mode::Browse), "{:?}", app.mode);
}

#[test]
fn w_on_a_project_on_its_own_marks_the_workspace_its_commands_would_use() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();
    weaver_at_home(&home, &home, &["init"]).unwrap();
    let project = home.join("collapse");
    fs::create_dir(&project).unwrap();
    new_project(&project);
    let lookup = Lookup::at_home(Some(&home));
    let mut app = App::opened_on(
        Scope::Project,
        project.clone(),
        tui::load_project(&project, &lookup).unwrap(),
    )
    .with_lookup(lookup);

    press_and_run(&mut app, KeyCode::Char('w'));

    let drawn = tui_drawn(&app);
    let marked = format!("* home  {}  no projects; the default one", home.display());
    assert!(drawn.contains(&marked), "{drawn}");
}

#[test]
fn the_keys_help_names_the_workspaces_list_and_its_command() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('?'));

    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    let lines: Vec<&str> = reader.lines.iter().map(|(_, line)| line.as_str()).collect();
    let at = lines
        .iter()
        .position(|line| line.starts_with("  w      the workspaces this machine knows"))
        .unwrap_or_else(|| panic!("{lines:?}"));
    assert_eq!(lines[at + 1].trim(), "mindweaver workspaces");
}

/// A line that serves the panel would hold the screen until a Ctrl-C the
/// screen takes as a key, so it is refused, from either binary, and nothing
/// listens. Each says `--no-open`, so a refusal that broke would serve
/// rather than open the developer's browser.
#[cfg(feature = "ui")]
#[test]
fn a_line_that_serves_the_panel_is_not_run_from_the_screen() {
    let dir = workspace_with_two();

    for said in [
        &["mindweaver", "ui", "--no-open"][..],
        &["mind", "ui", "--no-open"],
    ] {
        let words: Vec<String> = said.iter().map(|word| (*word).to_owned()).collect();
        let done = tui::execute(&words, dir.path(), &Lookup::default(), &Watch::new());

        assert_eq!(
            done,
            Done::failed(
                said.join(" "),
                "serves the panel until Ctrl-C, which this screen keeps for itself, \
                 so it is not run from inside it"
            )
        );
    }
}

/// A line that reads standard input would read the keys the screen does,
/// in raw mode, where none ends it, so it is refused, and nothing is
/// edited; one that reads a file runs.
#[test]
fn a_line_that_reads_standard_input_is_not_run_from_the_screen() {
    let dir = workspace_with_two();
    let manifest = dir.path().join("alpha/skills/alpha/SKILL.md");
    let held = fs::read_to_string(&manifest).unwrap();
    let words =
        |said: &[&str]| -> Vec<String> { said.iter().map(|word| (*word).to_owned()).collect() };

    let said = [
        "mindweaver",
        "edit",
        "-p",
        "alpha",
        "alpha",
        "--contents",
        "-",
    ];
    let done = tui::execute(&words(&said), dir.path(), &Lookup::default(), &Watch::new());

    assert_eq!(
        done,
        Done::failed(
            said.join(" "),
            "reads standard input, the keys this screen reads, so it is not run from inside it"
        )
    );
    assert_eq!(fs::read_to_string(&manifest).unwrap(), held);

    let rewritten = skill_file("alpha", "A skill, rewritten");
    fs::write(dir.path().join("draft.md"), &rewritten).unwrap();
    let said = [
        "mindweaver",
        "edit",
        "-p",
        "alpha",
        "alpha",
        "--contents",
        "draft.md",
    ];
    let done = tui::execute(&words(&said), dir.path(), &Lookup::default(), &Watch::new());
    assert!(done.ok, "{done:?}");
    assert_eq!(fs::read_to_string(&manifest).unwrap(), rewritten);
}

/// A project's rows show the MCP servers it received, each with a badge
/// in each agent's place it reaches, and each somebody keeps in an agent's
/// config file as foreign, as `status` lists them.
#[test]
fn a_projects_rows_show_its_mcp_servers_installed_and_somebodys() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let collapse = dir.path().join("collapse");
    fs::create_dir(&collapse).unwrap();
    new_project(&collapse);
    weaver(dir.path(), &["link", "collapse"]).unwrap();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("github.toml"),
        mindweaver_core::mother::McpMother::plain_program("github"),
    )
    .unwrap();
    weaver(dir.path(), &["install", "-p", "collapse", "github"]).unwrap();
    let claude = fs::read_to_string(collapse.join(".mcp.json")).unwrap();
    fs::write(
        collapse.join(".mcp.json"),
        claude.replacen(
            "\"mcpServers\": {",
            "\"mcpServers\": {\n    \"theirs\": {\"type\": \"http\", \"url\": \"https://example.com/mcp\"},",
            1,
        ),
    )
    .unwrap();
    let app = tui_in(dir.path());

    let drawn = tui_drawn(&app);

    for row in [
        "mcp   github  current  CC CX OC",
        "mcp   theirs  foreign  CC",
    ] {
        assert!(drawn.contains(row), "{row}\n{drawn}");
    }
}
