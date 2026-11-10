//! Unit tests for `task.rs`, kept beside it rather than inside it.

use std::collections::BTreeSet;

use clap::parser::ValueSource;
use clap::{ArgMatches, CommandFactory, Parser};

use mindweaver_core::finder::Finder;
use mindweaver_core::value::Redacted;
use mindweaver_core::DEFAULT_SUBDIRECTORY;

use super::*;
use crate::tui::minds::{Minds, Row, Standing};
use crate::{Cli, Credential, WeaverCli};

/// Every task's name in JSON, as a front end sends it: a contract, pinned
/// here word for word, so renaming a variant fails a test rather than every
/// front end that sends the old name.
const TAGS: [&str; 44] = [
    "init_workspace",
    "init_project",
    "link",
    "link_here",
    "init_and_link",
    "unlink",
    "relink",
    "load",
    "load_here",
    "unload",
    "unload_here",
    "add",
    "show",
    "edit",
    "save",
    "edit_server",
    "rename",
    "remove",
    "validate",
    "minds",
    "marks",
    "shelf",
    "workspaces",
    "search_skills",
    "take_skill",
    "search_servers",
    "import_server",
    "gather",
    "update_sources",
    "remove_source",
    "export",
    "backup",
    "targets",
    "settings",
    "toggled",
    "update",
    "install",
    "uninstall",
    "uninstall_orphaned",
    "adopt",
    "migrate",
    "open_install",
    "apply_install",
    "refresh",
];

/// Values for every field that takes one: plain, and shaped to break a naive
/// command line or a naive reader.
const VALUES: [&str; 8] = [
    "plain",
    "-odd",
    "--yes",
    "it's 'quoted'",
    "a b\nc",
    ";",
    "",
    "\u{e9}t\u{e9}",
];

/// Every holder a task can name, `entry` the one it names a project by.
fn holders(entry: &str) -> [Holder; 4] {
    [
        Holder::Workspace,
        Holder::Project(entry.to_owned()),
        Holder::Project(String::from(".")),
        Holder::Elsewhere(entry.to_owned()),
    ]
}

/// A task of every variant, with `value` in every field that takes free text,
/// a path or a name, and each holder in turn where a variant takes one.
///
/// What the tests that hold for every task are run over, so a variant left
/// out of it is one no test covers: [`TAGS`] is checked against the names it
/// builds, and against every name serde knows, so a new variant fails until
/// it is here.
fn every(value: &str) -> Vec<Task> {
    let owned = || value.to_owned();
    let mut tasks = vec![
        Task::InitWorkspace,
        Task::InitProject,
        Task::Link { path: owned() },
        Task::LinkHere,
        Task::InitAndLink { path: owned() },
        Task::Unlink { entry: owned() },
        Task::Relink {
            old: owned(),
            new: owned(),
        },
        Task::Load { path: owned() },
        Task::LoadHere { path: owned() },
        Task::Unload { entry: owned() },
        Task::UnloadHere { path: owned() },
        Task::Minds {
            init: vec![owned()],
            link: vec![owned(), String::from("plain")],
            unlink: vec![owned()],
        },
        Task::Marks {
            marks: vec![
                Mark {
                    path: owned(),
                    change: MindsChange::InitAndLink,
                },
                Mark {
                    path: String::from("plain"),
                    change: MindsChange::Link,
                },
                Mark {
                    path: owned(),
                    change: MindsChange::Unlink,
                },
            ],
        },
        Task::Shelf,
        Task::Workspaces,
        Task::SearchSkills { query: owned() },
        Task::TakeSkill {
            hit: hit(value, true),
        },
        Task::TakeSkill {
            hit: hit(value, false),
        },
        Task::SearchServers { query: owned() },
        Task::ImportServer {
            server: owned(),
            version: Some(owned()),
            yes: false,
        },
        Task::ImportServer {
            server: owned(),
            version: None,
            yes: true,
        },
        Task::Gather {
            url: owned(),
            folder: owned(),
            reference: Some(owned()),
            kind: Kind::Skill,
        },
        Task::Gather {
            url: owned(),
            folder: String::from(DEFAULT_SUBDIRECTORY),
            reference: None,
            kind: Kind::Skill,
        },
        Task::Gather {
            url: owned(),
            folder: owned(),
            reference: None,
            kind: Kind::Rule,
        },
        Task::Gather {
            url: owned(),
            folder: String::from(DEFAULT_SUBDIRECTORY),
            reference: Some(owned()),
            kind: Kind::Rule,
        },
        Task::UpdateSources {
            names: vec![owned(), String::from("plain")],
        },
        Task::UpdateSources { names: Vec::new() },
        Task::RemoveSource {
            name: owned(),
            yes: false,
        },
        Task::RemoveSource {
            name: owned(),
            yes: true,
        },
        Task::Export {
            folder: owned(),
            kinds: Vec::new(),
            yes: false,
        },
        Task::Export {
            folder: owned(),
            kinds: vec![Kind::Skill, Kind::Mcp],
            yes: true,
        },
        Task::Backup {
            url: owned(),
            branch: Some(owned()),
            kinds: vec![Kind::Rule],
            message: Some(owned()),
            token_env: Some(owned()),
        },
        Task::Backup {
            url: owned(),
            branch: None,
            kinds: Vec::new(),
            message: None,
            token_env: None,
        },
        Task::Install {
            projects: vec![owned(), String::from("plain")],
            kind: Kind::Skill,
            name: owned(),
            from: Some(owned()),
            yes: false,
        },
        Task::Uninstall {
            projects: vec![owned()],
            kind: Kind::Rule,
            name: owned(),
            force: false,
        },
        Task::Adopt {
            project: owned(),
            kind: Kind::Skill,
            name: owned(),
        },
        Task::Migrate {
            project: owned(),
            yes: false,
        },
        Task::Migrate {
            project: owned(),
            yes: true,
        },
        Task::OpenInstall,
        Task::Refresh,
        // `--everywhere` is the workspace's alone.
        Task::Remove {
            holder: Holder::Workspace,
            kind: Kind::Skill,
            name: owned(),
            everywhere: true,
            yes: true,
        },
        Task::Remove {
            holder: Holder::Workspace,
            kind: Kind::Rule,
            name: owned(),
            everywhere: true,
            yes: false,
        },
        Task::Update {
            holder: Holder::Workspace,
            reference: None,
            force: false,
        },
        Task::Update {
            holder: Holder::Workspace,
            reference: None,
            force: true,
        },
    ];
    tasks.push(Task::ApplyInstall {
        changes: tasks
            .iter()
            .filter(|task| matches!(task, Task::Install { .. } | Task::Uninstall { .. }))
            .cloned()
            .collect(),
    });
    // After the batch, which the install screen and the matrix send
    // unforced: forced, from the page of updates, it is one line alone.
    tasks.push(Task::Uninstall {
        projects: vec![owned(), String::from("plain")],
        kind: Kind::Skill,
        name: owned(),
        force: true,
    });
    for (at, holder) in holders(value).into_iter().enumerate() {
        let kind = Kind::ALL[at % Kind::ALL.len()];
        tasks.extend([
            Task::Add {
                holder: holder.clone(),
                kind,
                name: owned(),
                description: owned(),
                template: (at % 2 == 0).then(owned),
                server: None,
            },
            // A server's options, which a template refuses beside them: a
            // program's, or an address's, every field the value.
            Task::Add {
                holder: holder.clone(),
                kind: Kind::Mcp,
                name: owned(),
                description: owned(),
                template: None,
                server: Some(Box::new(server(value, at % 2 == 0))),
            },
            Task::EditServer {
                holder: holder.clone(),
                name: owned(),
                server: Box::new(server(value, at % 2 == 1)),
            },
            Task::Show {
                holder: holder.clone(),
                reference: owned(),
            },
            Task::Edit {
                holder: holder.clone(),
                kind,
                name: owned(),
            },
            // Its text reaches no line, only the route named by its hash,
            // so it is not one of the values a line is held to.
            Task::Save {
                holder: holder.clone(),
                kind,
                name: owned(),
                text: String::from("---\nname: plain\n---\n"),
                hash: owned(),
            },
            Task::Rename {
                holder: holder.clone(),
                kind,
                from: owned(),
                to: owned(),
            },
            Task::Remove {
                holder: holder.clone(),
                kind,
                name: owned(),
                everywhere: false,
                yes: at % 2 == 0,
            },
            Task::Validate {
                holder: holder.clone(),
            },
            Task::UninstallOrphaned {
                holder: holder.clone(),
            },
            Task::Update {
                holder: holder.clone(),
                reference: Some(owned()),
                force: at % 2 == 1,
            },
        ]);
        // A project's settings, never the workspace's: its own reach agents
        // by being installed, and a line naming no project is refused for
        // that, by the parser or the command.
        if holder == Holder::Workspace {
            continue;
        }
        tasks.extend([
            Task::Targets {
                holder: holder.clone(),
                kind: Kind::Skill,
                folders: Some(vec![owned(), String::from("plain")]),
            },
            Task::Targets {
                holder: holder.clone(),
                kind: Kind::Skill,
                folders: None,
            },
            Task::Settings {
                holder: holder.clone(),
                agents: Some(AgentsLine::Named(vec![Agent::OpenCode, Agent::Claude])),
                share: Some(Share::Local),
                edit_tracked: true,
            },
            Task::Settings {
                holder: holder.clone(),
                agents: Some(AgentsLine::Removed(vec![Agent::Codex])),
                share: None,
                edit_tracked: false,
            },
            Task::Settings {
                holder: holder.clone(),
                agents: None,
                share: Some(Share::Shared),
                edit_tracked: false,
            },
            Task::Toggled {
                holder: holder.clone(),
                was: toggles(&[Agent::OpenCode, Agent::Claude], Share::Shared, false),
                now: toggles(&[Agent::Codex, Agent::Claude], Share::Local, true),
            },
            Task::Toggled {
                holder: holder.clone(),
                was: toggles(&[Agent::Codex], Share::Local, false),
                now: toggles(&[], Share::Local, false),
            },
        ]);
    }
    tasks
}

/// A skill a search found, `value` in every field a line names: its
/// repository, and, when `placed`, its folder and its branch, which a
/// result at the root of its repository's default branch names neither of.
fn hit(value: &str, placed: bool) -> Hit {
    Hit {
        name: String::from("plain"),
        repository: Redacted::new(value),
        path: placed.then(|| value.to_owned()),
        reference: placed.then(|| value.to_owned()),
        installs: Some(56),
        stars: None,
        finders: vec![Finder::SkillsSh],
    }
}

/// How a server is started, `value` in every field, as a program, or as an
/// address reached over server-sent events.
fn server(value: &str, program: bool) -> mcp::Given {
    let owned = || value.to_owned();
    if program {
        mcp::Given::Program(mcp::Program {
            command: owned(),
            args: vec![owned(), String::from("plain")],
            env: vec![(owned(), owned())],
            secret_env: vec![value.into()],
            arg_env: vec![value.into()],
        })
    } else {
        mcp::Given::Address(mcp::Address {
            url: owned(),
            events: true,
            headers: vec![(owned(), owned())],
            secret_headers: vec![(owned(), value.into())],
            bearer: Some(value.into()),
            url_vars: vec![(value.into(), value.into())],
        })
    }
}

/// The name a task goes by in JSON.
fn tag(task: &Task) -> String {
    let json = serde_json::to_value(task).unwrap();
    json["type"].as_str().unwrap().to_owned()
}

/// `paths` as the owned words a task holds.
fn owned(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| (*path).to_owned()).collect()
}

#[test]
fn a_directory_made_a_project_is_linked_with_the_rest_in_the_order_given() {
    let task = Task::minds([
        (String::from("beta"), MindsChange::Link),
        (String::from("gamma"), MindsChange::InitAndLink),
        (String::from("alpha"), MindsChange::Unlink),
        (String::from("delta"), MindsChange::InitAndLink),
    ]);

    assert_eq!(
        task,
        Some(Task::Minds {
            init: owned(&["gamma", "delta"]),
            link: owned(&["beta", "gamma", "delta"]),
            unlink: owned(&["alpha"]),
        })
    );
    assert_eq!(
        task.unwrap().commands(),
        [
            owned(&["mind", "-C", "gamma", "init"]),
            owned(&["mind", "-C", "delta", "init"]),
            owned(&["mindweaver", "link", "beta", "gamma", "delta"]),
            owned(&["mindweaver", "unlink", "alpha"]),
        ]
    );
}

#[test]
fn nothing_to_link_or_unlink_is_no_task() {
    assert_eq!(Task::minds([]), None);
    assert_eq!(
        Task::minds([(String::from("alpha"), MindsChange::Unlink)]),
        Some(Task::Minds {
            init: Vec::new(),
            link: Vec::new(),
            unlink: owned(&["alpha"]),
        })
    );
}

/// Toggles of `agents`, shared and with tracked files left alone.
fn toggles(agents: &[Agent], share: Share, edit_tracked: bool) -> Toggles {
    Toggles {
        agents: agents.to_vec(),
        share,
        edit_tracked,
    }
}

#[test]
fn toggles_keep_the_agents_a_project_names_in_its_order_and_name_one_ticked_after_them() {
    let holder = Holder::Project(String::from("alpha"));
    let was = toggles(&[Agent::OpenCode, Agent::Claude], Share::Shared, false);
    // Ticked in any order: what the settings name keeps theirs, and one
    // more comes after them, in report order.
    let now = toggles(
        &[Agent::Codex, Agent::Claude, Agent::OpenCode],
        Share::Shared,
        false,
    );

    assert_eq!(
        Task::settings(holder.clone(), &was, &now),
        Some(Task::Settings {
            holder,
            agents: Some(AgentsLine::Named(vec![
                Agent::OpenCode,
                Agent::Claude,
                Agent::Codex,
            ])),
            share: None,
            edit_tracked: false,
        })
    );
}

#[test]
fn unticking_every_agent_is_removing_every_one_the_project_names() {
    let holder = Holder::Project(String::from("alpha"));
    let was = toggles(&[Agent::Claude, Agent::Codex], Share::Local, false);
    let now = toggles(&[], Share::Local, false);

    assert_eq!(
        Task::settings(holder.clone(), &was, &now),
        Some(Task::Settings {
            holder,
            agents: Some(AgentsLine::Removed(vec![Agent::Claude, Agent::Codex])),
            share: None,
            edit_tracked: false,
        })
    );
}

#[test]
fn a_share_line_comes_when_the_mode_or_edit_tracked_changes_and_toggles_left_as_they_were_are_no_task(
) {
    let holder = Holder::Project(String::from("alpha"));
    let was = toggles(&[Agent::Claude], Share::Local, false);

    assert_eq!(Task::settings(holder.clone(), &was, &was), None);
    // The same agents in another order are the same agents.
    let mut reordered = toggles(&[Agent::Claude, Agent::Claude], Share::Local, false);
    assert_eq!(Task::settings(holder.clone(), &was, &reordered), None);
    reordered.edit_tracked = true;
    assert_eq!(
        Task::settings(holder.clone(), &was, &reordered),
        Some(Task::Settings {
            holder,
            agents: None,
            share: Some(Share::Local),
            edit_tracked: true,
        })
    );
}

/// The minds screen, holding a row for each of `rows`, a path standing
/// as given, ticked as given.
fn screen(rows: &[(&str, Standing, bool)]) -> Minds {
    Minds {
        rows: rows
            .iter()
            .map(|(path, standing, ticked)| Row {
                path: (*path).to_owned(),
                standing: *standing,
                ticked: *ticked,
                why: String::new(),
            })
            .collect(),
        cursor: 0,
    }
}

/// What a front end sends for the marks the minds screen's rows hold, in
/// JSON as the panel sends them, each row turned over by what it offers.
fn marked(screen: &Minds) -> Task {
    let marks: Vec<serde_json::Value> = screen
        .rows
        .iter()
        .filter_map(|row| Some(serde_json::json!({"path": row.path, "change": row.change()?})))
        .collect();
    serde_json::from_value(serde_json::json!({"type": "marks", "marks": marks})).unwrap()
}

#[test]
fn marks_sent_as_json_make_the_lines_the_minds_screen_makes_for_the_same_selection() {
    let found = |mind| Standing::Found { mind };
    let linked = Standing::Linked { opens: true };
    for rows in [
        vec![
            ("/work/beta", found(true), true),
            ("/work/gamma", found(false), true),
            ("/work/alpha", linked, false),
            ("/work/delta", found(false), true),
            ("/work/kept", linked, true),
            ("/work/left", found(true), false),
        ],
        vec![("-odd", found(false), true), ("it's", found(true), true)],
        vec![("/work/alpha", linked, false)],
    ] {
        let screen = screen(&rows);

        let sent = marked(&screen);

        assert_eq!(
            sent.commands(),
            screen.task().unwrap().commands(),
            "{rows:?}"
        );
        assert_eq!(sent.cautions(), screen.task().unwrap().cautions());
    }
    assert_eq!(marked(&screen(&[])).commands(), Vec::<Vec<String>>::new());
}

#[test]
fn toggles_sent_as_json_make_the_lines_the_task_module_builds_from_them() {
    let holder = Holder::Project(String::from("alpha"));
    let was = toggles(&[Agent::OpenCode, Agent::Claude], Share::Shared, false);
    let now = toggles(&[Agent::Codex, Agent::Claude], Share::Local, true);

    let sent: Task = serde_json::from_value(serde_json::json!({
        "type": "toggled",
        "holder": {"type": "project", "entry": "alpha"},
        "was": {"agents": ["opencode", "claude"], "share": "shared", "edit_tracked": false},
        "now": {"agents": ["codex", "claude"], "share": "local", "edit_tracked": true},
    }))
    .unwrap();

    assert_eq!(
        sent.commands(),
        [
            owned(&["mind", "-C", "alpha", "agents", "--yes", "claude", "codex"]),
            owned(&[
                "mind",
                "-C",
                "alpha",
                "share",
                "--yes",
                "--edit-tracked",
                "local"
            ]),
        ]
    );
    assert_eq!(
        sent.commands(),
        Task::settings(holder.clone(), &was, &now)
            .unwrap()
            .commands()
    );
    let unchanged = Task::Toggled {
        holder,
        was: was.clone(),
        now: was,
    };
    assert_eq!(unchanged.commands(), Vec::<Vec<String>>::new());
}

#[test]
fn a_migration_is_a_dry_run_until_it_is_told_and_only_then_a_write() {
    let migrate = |yes| Task::Migrate {
        project: String::from("legacy"),
        yes,
    };

    assert_eq!(
        migrate(false).commands(),
        [owned(&["mindweaver", "migrate", "-p", "legacy"])]
    );
    assert!(!migrate(false).writes());
    assert_eq!(
        migrate(true).commands(),
        [owned(&["mindweaver", "migrate", "-p", "legacy", "--yes"])]
    );
    assert!(migrate(true).writes());
    assert_eq!(migrate(true).cautions(), Vec::new());
}

/// What the panel's page of one of the library's own shows before it asks:
/// removing it everywhere as a dry run, which writes and takes nothing, then
/// the same line told `--yes`, which takes it out of every project and
/// deletes it, and is asked about for both.
#[test]
fn a_removal_is_a_dry_run_until_it_is_told_and_only_then_asked_about() {
    let remove = |everywhere, yes| Task::Remove {
        holder: Holder::Workspace,
        kind: Kind::Skill,
        name: String::from("lint"),
        everywhere,
        yes,
    };

    assert_eq!(
        remove(true, false).commands(),
        [line("mindweaver remove --everywhere skill:lint")]
    );
    assert!(!remove(true, false).writes());
    assert_eq!(remove(true, false).cautions(), Vec::new());
    assert_eq!(
        remove(true, true).commands(),
        [line("mindweaver remove --everywhere --yes skill:lint")]
    );
    assert!(remove(true, true).writes());
    assert_eq!(
        remove(true, true).cautions(),
        [(0, Caution::Removes), (0, Caution::Uninstalls)]
    );
    assert_eq!(
        remove(false, false).commands(),
        [line("mindweaver remove skill:lint")]
    );
}

/// Taking out what nothing offers any more, from every project the
/// workspace links or from one, as somebody would type it: asked about
/// first, since it takes copies away.
#[test]
fn what_nothing_offers_is_taken_out_of_every_project_or_of_one() {
    let orphans = |holder| Task::UninstallOrphaned { holder };

    for (task, said) in [
        (
            orphans(Holder::Workspace),
            "mindweaver uninstall --all-projects --orphaned",
        ),
        (
            orphans(Holder::Project(String::from("/work/alpha"))),
            "mind -C /work/alpha uninstall --orphaned",
        ),
        (
            orphans(Holder::Elsewhere(String::from("/work/alpha"))),
            "mind -W . -C /work/alpha uninstall --orphaned",
        ),
    ] {
        assert_eq!(task.commands(), [line(said)], "{task:?}");
        assert!(task.writes(), "{task:?}");
        assert_eq!(task.cautions(), [(0, Caution::Uninstalls)], "{task:?}");
        assert_eq!(task.refusal(), None, "{task:?}");
    }
}

/// One artifact taken out of projects, as somebody would type it, and
/// forced only when asked: what the page of updates sends for a copy
/// changed since it was installed that nothing offers any more, which an
/// uninstall not forced leaves. Asked about first either way, and for the
/// edit too when forced.
#[test]
fn an_uninstall_is_the_line_the_cli_takes_and_forced_only_when_asked() {
    let uninstall = |projects: &[&str], force| Task::Uninstall {
        projects: owned(projects),
        kind: Kind::Skill,
        name: String::from("-lint"),
        force,
    };

    for (task, said, cautions) in [
        (
            uninstall(&["/work/alpha"], false),
            "mindweaver uninstall -p /work/alpha skill:-lint",
            vec![(0, Caution::Uninstalls)],
        ),
        (
            uninstall(&["/work/alpha", "-odd"], true),
            "mindweaver uninstall -p /work/alpha -p ./-odd --force skill:-lint",
            vec![(0, Caution::Uninstalls), (0, Caution::Forces)],
        ),
    ] {
        assert_eq!(task.commands(), [line(said)], "{task:?}");
        assert!(task.writes(), "{task:?}");
        assert_eq!(task.cautions(), cautions, "{task:?}");
        assert_eq!(task.refusal(), None, "{task:?}");
    }
}

/// What the panel's page of updates asks for, each the line somebody would
/// type: every project, one, one artifact in one, and that one forced,
/// which alone is asked about first.
#[test]
fn an_update_is_the_line_the_cli_takes_and_forced_only_when_asked() {
    let update = |holder, reference: Option<&str>, force| Task::Update {
        holder,
        reference: reference.map(str::to_owned),
        force,
    };
    let alpha = || Holder::Project(String::from("/work/alpha"));

    for (task, said, cautions) in [
        (
            update(Holder::Workspace, None, false),
            "mindweaver update --all --yes",
            Vec::new(),
        ),
        (
            update(alpha(), None, false),
            "mind -C /work/alpha update --yes",
            Vec::new(),
        ),
        (
            update(alpha(), Some("skill:lint"), false),
            "mind -C /work/alpha update --yes skill:lint",
            Vec::new(),
        ),
        (
            update(alpha(), Some("skill:lint"), true),
            "mind -C /work/alpha update --yes --force skill:lint",
            vec![(0, Caution::Forces)],
        ),
        (
            update(Holder::Workspace, Some("-odd"), true),
            "mindweaver update --all --yes --force -- -odd",
            vec![(0, Caution::Forces)],
        ),
    ] {
        assert_eq!(task.commands(), [line(said)], "{task:?}");
        assert!(task.writes(), "{task:?}");
        assert_eq!(task.cautions(), cautions, "{task:?}");
    }
}

/// What the TUI's sources screen asks for, each the line somebody would
/// type: one source gathered again, several, every one, and one forgotten,
/// which alone is asked about first, its dry run not.
#[test]
fn a_source_s_task_is_the_line_the_cli_takes() {
    let names = |names: &[&str]| Task::UpdateSources {
        names: owned(names),
    };
    let remove = |name: &str, yes| Task::RemoveSource {
        name: name.to_owned(),
        yes,
    };

    for (task, said, writes, cautions) in [
        (
            names(&["acme-skills"]),
            "mindweaver gather update acme-skills",
            true,
            Vec::new(),
        ),
        (
            names(&["acme-skills", "-odd"]),
            "mindweaver gather update -- acme-skills -odd",
            true,
            Vec::new(),
        ),
        (
            names(&[]),
            "mindweaver gather update --all",
            true,
            Vec::new(),
        ),
        (
            remove("acme-skills", false),
            "mindweaver gather remove acme-skills",
            false,
            Vec::new(),
        ),
        (
            remove("acme-skills", true),
            "mindweaver gather remove --yes acme-skills",
            true,
            vec![(0, Caution::Forgets)],
        ),
        (
            remove("-odd", true),
            "mindweaver gather remove --yes -- -odd",
            true,
            vec![(0, Caution::Forgets)],
        ),
    ] {
        assert_eq!(task.commands(), [line(said)], "{task:?}");
        assert_eq!(task.writes(), writes, "{task:?}");
        assert_eq!(task.cautions(), cautions, "{task:?}");
        assert_eq!(task.refusal(), None, "{task:?}");
    }
}

#[test]
fn every_variant_is_among_the_tasks_the_tests_build() {
    let built: BTreeSet<String> = every("plain").iter().map(tag).collect();

    let pinned: BTreeSet<String> = TAGS.iter().map(|tag| (*tag).to_owned()).collect();
    assert_eq!(built, pinned);
}

#[test]
fn every_name_a_task_goes_by_in_json_is_the_one_pinned() {
    // What serde says it expected when handed a name no task has: the name
    // of every variant, which a list kept here could miss one of.
    let error = serde_json::from_str::<Task>(r#"{"type": "?"}"#)
        .unwrap_err()
        .to_string();
    let (_, expected) = error
        .split_once("expected one of ")
        .unwrap_or_else(|| panic!("{error}"));
    let known: Vec<&str> = expected.split('`').skip(1).step_by(2).collect();

    assert_eq!(known, TAGS, "{error}");
}

#[test]
fn every_task_reads_back_from_its_json_as_it_was() {
    for value in VALUES {
        for task in every(value) {
            let json = serde_json::to_string(&task).unwrap();

            let read: Task =
                serde_json::from_str(&json).unwrap_or_else(|error| panic!("{json}: {error}"));

            assert_eq!(read, task, "{json}");
        }
    }
}

#[test]
fn a_task_is_one_flat_record_tagged_by_its_name() {
    let task = Task::Settings {
        holder: Holder::Elsewhere(String::from("../collapse")),
        agents: Some(AgentsLine::Named(vec![Agent::OpenCode])),
        share: Some(Share::Local),
        edit_tracked: false,
    };

    assert_eq!(
        serde_json::to_value(&task).unwrap(),
        serde_json::json!({
            "type": "settings",
            "holder": {"type": "elsewhere", "entry": "../collapse"},
            "agents": {"type": "named", "agents": ["opencode"]},
            "share": "local",
            "edit_tracked": false,
        })
    );
    assert_eq!(
        serde_json::to_value(Task::Add {
            holder: Holder::Workspace,
            kind: Kind::Rule,
            name: String::from("git/commits"),
            description: String::from("One step at a time"),
            template: None,
            server: None,
        })
        .unwrap(),
        serde_json::json!({
            "type": "add",
            "holder": {"type": "workspace"},
            "kind": "rule",
            "name": "git/commits",
            "description": "One step at a time",
            "template": null,
            "server": null,
        })
    );
    assert_eq!(
        serde_json::to_value(Task::EditServer {
            holder: Holder::Workspace,
            name: String::from("github"),
            server: Box::new(mcp::Given::Program(mcp::Program {
                command: String::from("npx"),
                secret_env: vec!["GITHUB_TOKEN".into()],
                ..mcp::Program::default()
            })),
        })
        .unwrap(),
        serde_json::json!({
            "type": "edit_server",
            "holder": {"type": "workspace"},
            "name": "github",
            "server": {
                "type": "program",
                "command": "npx",
                "args": [],
                "env": [],
                "secret_env": ["GITHUB_TOKEN"],
                "arg_env": [],
            },
        })
    );
    // An add sent as it was before a server could be told, with no
    // `server`, is the same add.
    let sent: Task = serde_json::from_value(serde_json::json!({
        "type": "add", "holder": {"type": "workspace"}, "kind": "skill",
        "name": "deploy", "description": "Ship it", "template": null,
    }))
    .unwrap();
    assert!(matches!(sent, Task::Add { server: None, .. }), "{sent:?}");
    assert_eq!(
        serde_json::to_value(MindsChange::InitAndLink).unwrap(),
        "init_and_link"
    );
}

#[test]
fn a_task_sent_with_a_field_it_does_not_have_is_refused() {
    for (json, field) in [
        (
            r#"{"type": "link", "path": "beta", "force": true}"#,
            "force",
        ),
        (
            r#"{"type": "validate", "holder": {"type": "project", "entry": "alpha", "path": "x"}}"#,
            "path",
        ),
        (
            r#"{"type": "settings", "holder": {"type": "workspace"},
                "agents": {"type": "named", "agents": ["claude"], "yes": true},
                "share": null, "edit_tracked": false}"#,
            "yes",
        ),
    ] {
        let error = serde_json::from_str::<Task>(json).unwrap_err();

        assert!(
            error.to_string().contains(&format!("\"{field}\""))
                || error.to_string().contains(&format!("`{field}`")),
            "{json}: {error}"
        );
    }
}

/// A gather task says its kind as `--kind` does, and leaves out only the
/// kind a gather takes unsaid and the folder a gather of its kind looks in
/// unsaid: rules from the whole repository say `--path .`, which core asks
/// of them (#111).
#[test]
fn a_gather_task_says_its_kind_and_the_folder_a_gather_of_rules_is_told() {
    let gather = |folder: &str, kind: Kind| Task::Gather {
        url: String::from("acme/rules"),
        folder: folder.to_owned(),
        reference: None,
        kind,
    };
    let line = |words: &[&str]| -> Vec<Vec<String>> {
        vec![words.iter().map(|word| (*word).to_owned()).collect()]
    };

    assert_eq!(
        gather(".", Kind::Skill).commands(),
        line(&["mindweaver", "gather", "git", "acme/rules"])
    );
    assert_eq!(
        gather("rules", Kind::Rule).commands(),
        line(&[
            "mindweaver",
            "gather",
            "git",
            "--kind",
            "rules",
            "--path",
            "rules",
            "acme/rules"
        ])
    );
    assert_eq!(
        gather(".", Kind::Rule).commands(),
        line(&[
            "mindweaver",
            "gather",
            "git",
            "--kind",
            "rules",
            "--path",
            ".",
            "acme/rules"
        ])
    );
}

/// A gather task sent before it carried a kind, or leaving it out, gathers
/// skills, as the command does without `--kind`.
#[test]
fn a_gather_task_that_names_no_kind_gathers_skills() {
    let sent = r#"{"type": "gather", "url": "acme/skills", "folder": ".", "reference": null}"#;

    let task: Task = serde_json::from_str(sent).unwrap();

    assert!(
        matches!(
            &task,
            Task::Gather {
                kind: Kind::Skill,
                ..
            }
        ),
        "{task:?}"
    );
    let rules: Task = serde_json::from_str(
        r#"{"type": "gather", "url": "acme/rules", "folder": "rules", "reference": null,
            "kind": "rule"}"#,
    )
    .unwrap();
    assert!(
        matches!(
            &rules,
            Task::Gather {
                kind: Kind::Rule,
                ..
            }
        ),
        "{rules:?}"
    );
}

/// The panel enters no credential (D-16): a gather it sends names the
/// repository, the folder and the branch, and a token beside them, by
/// whatever name, is a field the task does not have.
#[test]
fn a_gather_task_carries_no_token_by_any_name() {
    for field in ["token", "token_env", "gh", "credential"] {
        let json = format!(
            r#"{{"type": "gather", "url": "acme/private", "folder": ".", "reference": null,
                "{field}": "GITHUB_TOKEN"}}"#
        );

        let error = serde_json::from_str::<Task>(&json).unwrap_err();

        assert!(error.to_string().contains(field), "{json}: {error}");
    }
}

/// Nor do the lines a gather task is spelled as hand one over, whatever a
/// browser fills its values with: each reaches the line as a value, never
/// as `--token-env` or `--gh`, so no line of it is refused for one.
#[test]
fn a_gather_task_s_lines_hand_over_no_token_whatever_its_values() {
    for value in ["--gh", "--token-env=HOME", "--token-env", "-gh"] {
        for task in [
            Task::Gather {
                url: value.to_owned(),
                folder: String::from(DEFAULT_SUBDIRECTORY),
                reference: None,
                kind: Kind::Skill,
            },
            Task::Gather {
                url: String::from("acme/private"),
                folder: value.to_owned(),
                reference: Some(value.to_owned()),
                kind: Kind::Rule,
            },
        ] {
            for words in task.commands() {
                let parsed = WeaverCli::try_parse_from(&words)
                    .unwrap_or_else(|error| panic!("{words:?}: {error}"));
                let Some(WeaverCommand::Gather(GatherCommand::Git { credential, .. })) =
                    parsed.command
                else {
                    panic!("{words:?} is no gather");
                };
                assert_eq!(credential, Credential::default(), "{words:?}");
            }
            assert_eq!(task.refusal(), None, "{task:?}");
        }
    }
}

#[test]
fn a_task_holding_a_word_no_command_line_takes_is_refused() {
    let error = serde_json::from_str::<Task>(
        r#"{"type": "edit", "holder": {"type": "workspace"}, "kind": "agent", "name": "x"}"#,
    )
    .unwrap_err();

    assert!(
        error.to_string().contains("unknown kind `agent`"),
        "{error}"
    );
}

/// Values that start with a dash, each one a flag or an option somewhere in
/// the grammar, or the `--` that ends them, or shaped like an option given
/// its value; then [`dashed`]'s, made up from the characters a flag is made
/// of and those a shell treats specially.
const FLAGS: [&str; 24] = [
    "-",
    "--",
    "-p",
    "-C",
    "-W",
    "-y",
    "-h",
    "-V",
    "--yes",
    "--help",
    "--version",
    "--all",
    "--force",
    "--everywhere",
    "--edit-tracked",
    "--template=full",
    "--home=",
    "-p=alpha",
    "--=",
    "-x y",
    "-;",
    "-\nmindweaver init",
    "--\n--yes",
    "-'quoted'",
];

/// `count` values that start with a dash, each up to a dozen characters
/// more, from a fixed seed so a failure happens again: what a property test
/// crate would generate, from no crate, since one loop over every variant is
/// all this asks of it.
///
/// No `@` among them, which with a `://` before it is a URL's credential,
/// taken out of a line where it is shown.
fn dashed(count: usize) -> Vec<String> {
    const ALPHABET: &[char] = &[
        '-', '-', '-', '=', 'p', 'C', 'W', 'y', 'e', 's', 'a', 'l', ' ', ';', '\n', '\'', '"', '&',
        '|', '$', '`', '.', '/', ':', '\\', '*', '#', '~', '\u{e9}',
    ];
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    (0..count)
        .map(|_| {
            let length = (next() % 13) as usize;
            let mut value = String::from("-");
            value.extend((0..length).map(|_| ALPHABET[(next() % ALPHABET.len() as u64) as usize]));
            value
        })
        .collect()
}

/// Every value starting with a dash the property below is held to.
fn hostile() -> Vec<String> {
    let mut values: Vec<String> = FLAGS.iter().map(|value| (*value).to_owned()).collect();
    values.extend(dashed(200));
    values
}

/// The two parsers, built once: building one is most of what parsing a line
/// costs, and the property below parses some tens of thousands.
struct Parsers {
    mind: clap::Command,
    weaver: clap::Command,
}

impl Parsers {
    fn new() -> Self {
        Parsers {
            mind: Cli::command(),
            weaver: WeaverCli::command(),
        }
    }

    /// What `line` asks for, as the parser of the binary it names reads it
    /// ([`read`]).
    fn read(&mut self, line: &[String]) -> Read {
        let command = match line[0].as_str() {
            "mind" => &mut self.mind,
            "mindweaver" => &mut self.weaver,
            other => panic!("{line:?} runs `{other}`"),
        };
        let matches = command
            .try_get_matches_from_mut(line)
            .unwrap_or_else(|error| panic!("{line:?} does not parse: {error}"));
        read(&[command], &line[0], &matches)
    }
}

/// What a line asked for ([`read`]): each command in turn by its name, and
/// under it each argument given.
type Read = Vec<(String, Vec<Given>)>;

/// One argument a line gave.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Given {
    id: String,
    /// The short flag that gives it, when it is an option that has one: by
    /// the flag rather than the id, since `mindweaver link` takes its
    /// directories as `projects` too, behind no flag and no `./`.
    short: Option<char>,
    values: Vec<String>,
}

/// What `matches` says a line asked for, `commands` the command it was read
/// by and each above it: each command in turn by its name, and under each,
/// every argument the line gave, by its id, with the values it took, as
/// given.
///
/// Two lines alike in this asked the same command for the same options in
/// the same places, each holding the same values: one that read a value as
/// an option would have an argument more, or a value fewer, one that read
/// it as a command would name another, and one that changed a value on its
/// way, a dash dropped from `--from=-odd`, would hold another under that
/// option's id, whatever the line's other fields hold.
fn read(commands: &[&clap::Command], name: &str, matches: &ArgMatches) -> Read {
    // The innermost first, and then those above it, where a global option
    // such as `-C` is declared.
    let short = |id: &str| {
        commands
            .iter()
            .rev()
            .find_map(|command| command.get_arguments().find(|arg| arg.get_id() == id))
            .and_then(clap::Arg::get_short)
    };
    let mut given: Vec<Given> = matches
        .ids()
        .filter(|id| matches.value_source(id.as_str()) == Some(ValueSource::CommandLine))
        .map(|id| Given {
            id: id.as_str().to_owned(),
            short: short(id.as_str()),
            values: matches
                .try_get_raw(id.as_str())
                .ok()
                .flatten()
                .map_or_else(Vec::new, |raw| {
                    raw.map(|value| value.to_string_lossy().into_owned())
                        .collect()
                }),
        })
        .collect();
    given.sort();
    let mut read = vec![(name.to_owned(), given)];
    if let Some((name, matches)) = matches.subcommand() {
        let inner = commands[commands.len() - 1]
            .find_subcommand(name)
            .unwrap_or_else(|| panic!("no command `{name}`"));
        let mut commands = commands.to_vec();
        commands.push(inner);
        read.extend(self::read(&commands, name, matches));
    }
    read
}

/// What the property below builds every task with in place of a value that
/// starts with a dash, to find where each field's value lands: plain, and a
/// word no fixed part of a line holds, as `plain`, which [`every`] puts
/// beside the value in some lists, does.
const GIVEN: &str = "given";

/// The short flags whose value names a project's directory: `-C`, and the
/// `-p` of an install, an uninstall and an adoption. A value that starts
/// with a dash reaches them behind a `./`, which names the same directory
/// and so the same project ([`directory`]).
const DIRECTORIES: [char; 2] = ['C', 'p'];

/// What a line built with `value` wherever [`GIVEN`] was is to be read as,
/// given how the line built with [`GIVEN`] was read (`read`): the same
/// commands, the same arguments under the same ids, and in each place
/// [`GIVEN`] was, `value` as given, behind its `./` under an option that
/// names a directory.
fn holding(read: &Read, value: &str) -> Read {
    read.iter()
        .map(|(name, given)| {
            let given = given
                .iter()
                .map(|given| {
                    let directory = given
                        .short
                        .is_some_and(|short| DIRECTORIES.contains(&short));
                    let values = given
                        .values
                        .iter()
                        .map(|held| {
                            let held = held.replace(GIVEN, value);
                            if directory && held.starts_with('-') {
                                format!("./{held}")
                            } else {
                                held
                            }
                        })
                        .collect();
                    Given {
                        values,
                        ..given.clone()
                    }
                })
                .collect();
            (name.clone(), given)
        })
        .collect()
}

#[test]
fn every_variant_s_lines_parse_with_cli_or_weaver_cli() {
    // Not the empty value: the parser refuses an empty path for what it is,
    // which is where one is to be refused.
    for value in VALUES.into_iter().filter(|value| !value.is_empty()) {
        for task in every(value) {
            for line in task.commands() {
                let error = match line[0].as_str() {
                    "mind" => Cli::try_parse_from(&line).err(),
                    "mindweaver" => WeaverCli::try_parse_from(&line).err(),
                    other => panic!("{task:?} runs `{other}`"),
                };
                assert!(error.is_none(), "{line:?} does not parse: {error:?}");
            }
        }
    }
}

#[test]
fn no_value_starting_with_a_dash_is_read_as_an_option_or_a_command() {
    let mut parsers = Parsers::new();
    // How each line built with a plain value is read, once.
    let plain: Vec<Vec<(Vec<String>, Read)>> = every(GIVEN)
        .iter()
        .map(|task| {
            task.commands()
                .into_iter()
                .map(|line| {
                    let read = parsers.read(&line);
                    (line, read)
                })
                .collect()
        })
        .collect();
    // Held to something: a field the property never reaches would pass it
    // whatever its line did with a value.
    let reached = plain
        .iter()
        .flatten()
        .flat_map(|(_, read)| read.iter().flat_map(|(_, given)| given))
        .flat_map(|given| &given.values)
        .filter(|value| value.contains(GIVEN))
        .count();
    assert!(reached > 0);
    for value in hostile() {
        for (task, expected) in every(&value).iter().zip(&plain) {
            let lines = task.commands();
            assert_eq!(lines.len(), expected.len(), "{task:?}");
            for (line, (plainly, read)) in lines.iter().zip(expected) {
                assert_eq!(
                    parsers.read(line),
                    holding(read, &value),
                    "{line:?} is not read as {plainly:?} is, holding {value:?}"
                );
            }
        }
    }
}

impl Parsers {
    /// What `line`, an `add` or an `edit`, tells its command of how a
    /// server is started or reached, as the command's own options read it:
    /// each pair split where its option splits it.
    fn told(&mut self, line: &[String]) -> Option<mcp::Given> {
        use clap::FromArgMatches as _;

        let command = match line[0].as_str() {
            "mind" => &mut self.mind,
            "mindweaver" => &mut self.weaver,
            other => panic!("{line:?} runs `{other}`"),
        };
        let matches = command
            .try_get_matches_from_mut(line)
            .unwrap_or_else(|error| panic!("{line:?} does not parse: {error}"));
        let mut at = &matches;
        while let Some((name, inner)) = at.subcommand() {
            at = inner;
            if name == "add" || name == "edit" {
                break;
            }
        }
        crate::ServerOptions::from_arg_matches(at)
            .unwrap_or_else(|error| panic!("{line:?}: {error}"))
            .given()
    }
}

/// `given`, each name it gives beside a value without the `=` its option
/// would end the name at: what a request may send of it, the rest refused
/// before a line is built (`policy::sayable`).
fn sayable(given: mcp::Given) -> mcp::Given {
    let name = |key: &str| key.replace('=', "");
    match given {
        mcp::Given::Program(mut program) => {
            for (key, _) in &mut program.env {
                *key = name(key);
            }
            mcp::Given::Program(program)
        }
        mcp::Given::Address(mut address) => {
            for (key, _) in &mut address.headers {
                *key = name(key);
            }
            for (header, _) in &mut address.secret_headers {
                *header = name(header);
            }
            for (input, _) in &mut address.url_vars {
                *input = name(input.as_str()).into();
            }
            mcp::Given::Address(address)
        }
    }
}

/// The property above compares the words a line gives each option, which
/// a pair's are whatever its option splits them into: this one compares
/// what the command is told, each pair as the option splits it, for every
/// server a request may send, which is every one but those naming
/// something with an `=` in the name.
#[test]
fn a_server_s_every_option_reaches_its_command_as_the_value_given() {
    let mut parsers = Parsers::new();
    let values = VALUES
        .into_iter()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .chain(hostile());
    for value in values {
        for program in [true, false] {
            let sent = server(&value, program);
            assert_eq!(
                mindweaver_core::policy::sayable(&sent).is_ok(),
                !value.contains('='),
                "{sent:?}"
            );
            let given = sayable(sent);
            let tasks = [
                Task::Add {
                    holder: Holder::Workspace,
                    kind: Kind::Mcp,
                    name: String::from("acme"),
                    description: String::from("Acme's tools"),
                    template: None,
                    server: Some(Box::new(given.clone())),
                },
                Task::EditServer {
                    holder: Holder::Workspace,
                    name: String::from("acme"),
                    server: Box::new(given.clone()),
                },
            ];
            for task in tasks {
                for line in task.commands() {
                    assert_eq!(
                        parsers.told(&line),
                        Some(given.clone()),
                        "{line:?} does not tell its command what it was given"
                    );
                }
            }
        }
    }
}

#[test]
fn a_line_named_for_a_workspace_names_it_once_and_is_otherwise_read_as_it_was() {
    let root = "/home/you/work";
    for task in every(GIVEN) {
        for line in task.commands() {
            let named = naming(&line, root);
            assert_eq!(&named[1..3], ["-W", root], "{named:?}");
            // The same command, from the same directory: only the workspace
            // it names differs, which a project's `-W .` named before.
            let workspace = match named[0].as_str() {
                "mind" => {
                    let (cli, plain) = (
                        Cli::try_parse_from(&named).unwrap(),
                        Cli::try_parse_from(&line).unwrap(),
                    );
                    assert_eq!(cli.directory, plain.directory, "{named:?}");
                    assert_eq!(
                        format!("{:?}", cli.command),
                        format!("{:?}", plain.command),
                        "{named:?}"
                    );
                    cli.workspace
                }
                "mindweaver" => {
                    let (cli, plain) = (
                        WeaverCli::try_parse_from(&named).unwrap(),
                        WeaverCli::try_parse_from(&line).unwrap(),
                    );
                    assert_eq!(cli.directory, plain.directory, "{named:?}");
                    assert_eq!(
                        format!("{:?}", cli.command),
                        format!("{:?}", plain.command),
                        "{named:?}"
                    );
                    cli.workspace
                }
                other => panic!("{task:?} runs `{other}`"),
            };
            assert_eq!(workspace.as_deref(), Some(root), "{named:?}");
        }
    }
}

#[test]
fn a_project_s_line_naming_the_workspace_on_screen_is_named_for_the_one_given_instead() {
    let edit = Task::Validate {
        holder: Holder::Elsewhere(String::from("alpha")),
    };
    let lines = edit.commands();
    assert_eq!(lines, [line("mind -W . -C alpha validate")]);

    assert_eq!(
        naming(&lines[0], "/home/you/work"),
        line("mind -W /home/you/work -C alpha validate")
    );
}

#[test]
fn a_root_starting_with_a_dash_is_named_as_the_value_of_w() {
    let named = naming(&line("mindweaver link beta"), "-W");

    assert_eq!(named, line("mindweaver -W=-W link beta"));
    let cli = WeaverCli::try_parse_from(&named).unwrap();
    assert_eq!(cli.workspace.as_deref(), Some("-W"));
}

#[test]
fn a_line_shown_runs_as_one_command_of_the_words_it_is() {
    for value in hostile().iter().map(String::as_str).chain(VALUES) {
        for task in every(value) {
            for line in task.commands() {
                let shown = shell(&line);

                assert_eq!(words_of(&shown), line, "{shown}");
            }
        }
    }
}

/// `line`, a line [`shell`] showed, split into its words as a POSIX shell
/// would split it, refusing anything that shell would read as more than
/// words: a character outside a quote other than those `quote` leaves bare,
/// the spaces between words, and the `\'` it escapes a quote with.
fn words_of(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut quoted = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                quoted = !quoted;
                word.get_or_insert_with(String::new);
            }
            c if quoted => word.get_or_insert_with(String::new).push(c),
            ' ' => words.extend(word.take()),
            '\\' => {
                let escaped = chars.next();
                assert_eq!(escaped, Some('\''), "{line}: a bare backslash");
                word.get_or_insert_with(String::new).push('\'');
            }
            c if c.is_ascii_alphanumeric() || "-_./:@%+=,".contains(c) => {
                word.get_or_insert_with(String::new).push(c);
            }
            c => panic!("{line}: {c:?} outside a quote"),
        }
    }
    assert!(!quoted, "{line}: a quote left open");
    words.extend(word);
    words
}

/// `line`, split at its spaces, as the words a front end runs.
fn line(line: &str) -> Vec<String> {
    line.split(' ').map(str::to_owned).collect()
}

#[test]
fn a_front_end_with_no_terminal_refuses_what_draws_edits_activates_or_shows() {
    for (said, refusal) in [
        ("mind", Refusal::Screen),
        ("mind tui", Refusal::Screen),
        ("mind -C alpha tui", Refusal::Screen),
        ("mind weaver", Refusal::Screen),
        ("mind weaver tui", Refusal::Screen),
        ("mindweaver", Refusal::Screen),
        ("mindweaver --home= tui", Refusal::Screen),
        ("mindweaver minds", Refusal::Screen),
        ("mindweaver minds work", Refusal::Screen),
        ("mindweaver install", Refusal::Screen),
        ("mindweaver install --from workspace", Refusal::Screen),
        ("mind weaver install", Refusal::Screen),
        ("mind edit deploy", Refusal::Editor),
        ("mind edit --editor true deploy", Refusal::Editor),
        ("mindweaver edit -p alpha deploy", Refusal::Editor),
        ("mind weaver edit deploy", Refusal::Editor),
        ("mind edit --contents - deploy", Refusal::Input),
        (
            "mindweaver edit -p alpha --contents - --if-unchanged 0a deploy",
            Refusal::Input,
        ),
        ("mindweaver activate", Refusal::Shell),
        ("mind weaver activate --shell fish work", Refusal::Shell),
        ("mind show deploy", Refusal::Unheld),
        ("mind -C alpha show --json rule:style", Refusal::Unheld),
        ("mindweaver show -p alpha deploy", Refusal::Unheld),
        ("mind weaver show deploy", Refusal::Unheld),
    ] {
        assert_eq!(refused(&line(said)), Some(refusal), "{said}");
    }
}

/// From either binary, with or without its options: a line that serves
/// the panel never ends, as the line of a front end must.
#[cfg(feature = "ui")]
#[test]
fn a_front_end_that_runs_lines_refuses_the_panel() {
    for said in [
        "mindweaver ui",
        "mindweaver ui --no-open --port 0",
        "mindweaver ui --no-open --token-file api.json",
        "mind ui",
        "mind -C alpha ui --dev",
        "mind weaver ui",
    ] {
        assert_eq!(refused(&line(said)), Some(Refusal::Panel), "{said}");
    }
}

/// So a build without the panel says `ui` is no command, as for any word
/// it does not know.
#[cfg(not(feature = "ui"))]
#[test]
fn without_the_ui_feature_there_is_no_ui_command() {
    for said in ["mindweaver ui", "mind ui", "mind weaver ui"] {
        let words = line(said);
        let parsed = match words[0].as_str() {
            "mind" => Cli::try_parse_from(&words).map(drop),
            _ => WeaverCli::try_parse_from(&words).map(drop),
        };
        let error = parsed.expect_err(said);
        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::InvalidSubcommand,
            "{said}"
        );
    }
}

#[test]
fn a_front_end_with_no_terminal_runs_everything_else() {
    for said in [
        "mindweaver gather list",
        // The network, each finder asked in turn, and stopped between two
        // of them; with `--take`, a gather besides (#108).
        "mindweaver gather search pdf",
        "mindweaver gather search --json --finder skills.sh pdf",
        "mindweaver gather search --take 2 pdf",
        // The network, followed as it goes and stopped when asked, and
        // never a prompt (#112).
        "mindweaver gather git https://example.com/skills.git",
        "mind weaver gather git --path tools --ref v2 /srv/git/skills",
        // The network too, followed and stopped until it pushes, and
        // never a prompt from git: refused until it reported its steps.
        "mindweaver backup https://example.com/library.git",
        "mind weaver backup ../library.git --kind skills --token-env TOKEN",
        "mindweaver export --kind rules --yes ../backup",
        "mindweaver install -p alpha deploy",
        "mindweaver install --all-projects deploy",
        // Half an install, which opens no screen: the command refuses it
        // with a reason of its own.
        "mindweaver install -p alpha",
        "mindweaver install --all-projects",
        "mind install deploy",
        "mind weaver link alpha",
        "mindweaver which",
        "mind status --json",
        // An edit that takes its contents from a file, held to core's
        // rules as any edit's are, opens nothing.
        "mind edit --contents draft.md deploy",
        // Not a line either parser takes, which running it says, in the
        // parser's words.
        "mindweaver frobnicate",
        "ls",
    ] {
        assert_eq!(refused(&line(said)), None, "{said}");
    }
}

#[test]
fn a_task_is_refused_for_the_first_of_its_lines_that_is() {
    // Not the empty value, whose lines the parser refuses before this list
    // is asked: what a line that cannot run would do is no question.
    let values = VALUES.into_iter().filter(|value| !value.is_empty());
    for task in values.flat_map(every) {
        let expected = match &task {
            Task::OpenInstall => Some(Refusal::Screen),
            Task::Edit { .. } => Some(Refusal::Editor),
            Task::Show { .. } => Some(Refusal::Unheld),
            _ => None,
        };

        assert_eq!(task.refusal(), expected, "{task:?}");
    }
}

/// A gather, a backup and a search are followed as they go and stopped
/// when asked, whichever binary spells them and whatever they are told; a
/// search that takes a result is a gather; nothing else is, a gather's list
/// and an export among it.
#[test]
fn a_front_end_follows_a_gather_a_backup_and_a_search_and_nothing_else() {
    for said in [
        "mindweaver gather git https://example.com/skills.git",
        "mindweaver -W /home/you/work gather git --path tools --ref v2 /srv/git/skills",
        "mind weaver gather git git@example.com:acme/skills.git",
        "mind -C alpha weaver gather git file:///srv/git/skills",
        "mindweaver gather update acme-skills other-rules",
        "mind weaver gather update --all --gh",
        "mindweaver gather search --take 1 pdf",
        "mind weaver gather search --gh --take 3 -- -odd",
    ] {
        assert_eq!(followed(&line(said)), Some(Followed::Gather), "{said}");
    }
    for said in [
        "mindweaver backup https://example.com/library.git",
        "mindweaver -W /home/you/work backup --branch library --kind skills /srv/git/library.git",
        "mind weaver backup --token-env GITHUB_TOKEN -m saved git@example.com:you/library.git",
    ] {
        assert_eq!(followed(&line(said)), Some(Followed::Backup), "{said}");
    }
    for said in [
        "mindweaver gather search pdf",
        "mindweaver gather search --json --finder skills.sh --finder github pdf",
        "mind weaver gather search --token-env GITHUB_TOKEN pdf",
    ] {
        assert_eq!(followed(&line(said)), Some(Followed::Search), "{said}");
    }
    for said in [
        "mindweaver gather list",
        "mindweaver gather sources",
        "mindweaver gather rename acme-skills team",
        "mindweaver gather remove --yes acme-skills",
        "mindweaver export ../backup",
        "mindweaver link alpha",
        "mind install deploy",
        "mindweaver",
        "mindweaver frobnicate",
        "ls",
    ] {
        assert_eq!(followed(&line(said)), None, "{said}");
    }
}

/// What runs apart, in the TUI, is what a front end follows: asked of the
/// lines, as a refusal is, so every task building a gather's line runs
/// apart, and no other does.
#[test]
fn a_task_runs_apart_when_one_of_its_lines_is_followed() {
    let values = VALUES.into_iter().filter(|value| !value.is_empty());
    for task in values.flat_map(every) {
        let expected = matches!(
            task,
            Task::Gather { .. }
                | Task::TakeSkill { .. }
                | Task::UpdateSources { .. }
                | Task::Backup { .. }
                | Task::SearchSkills { .. }
        );

        assert_eq!(task.runs_apart(), expected, "{task:?}");
    }
}

/// A registry is asked a page at a time, which a search narrow enough to
/// choose from reads in one request, and core reports no step of it: no
/// line of `gather mcp` is followed, so none runs apart, and none is
/// refused a front end without a terminal.
#[test]
fn no_line_of_gather_mcp_is_followed_or_refused() {
    for said in [
        "mindweaver gather mcp search airtable",
        "mindweaver gather mcp search --json --registry http://127.0.0.1:9 airtable",
        "mindweaver gather mcp add ai.clawfetch/mcp",
        "mindweaver gather mcp add --version 0.4.0 --name claw --way 2 --yes ai.clawfetch/mcp",
        "mind weaver gather mcp add --yes ai.clawfetch/mcp",
    ] {
        assert_eq!(followed(&line(said)), None, "{said}");
        assert_eq!(refused(&line(said)), None, "{said}");
        assert!(cautions(&line(said)).is_empty(), "{said}");
    }
}

/// `edit` opens an editor, or reads standard input, each of which only a
/// terminal gives, unless it is given a file to take the contents from:
/// that line runs, wherever it is spelled and however it names the file,
/// at either level.
#[test]
fn edit_runs_without_a_terminal_only_when_its_contents_are_a_files() {
    for said in [
        "mind edit --contents draft.md deploy",
        "mind -C alpha edit --contents=/tmp/draft.md --if-unchanged 0a rule:style",
        "mind edit deploy --contents ./-",
        "mindweaver edit --contents draft.md deploy",
        "mindweaver edit -p alpha --contents draft.md --if-unchanged 0a deploy",
        "mind weaver edit --contents draft.md deploy",
    ] {
        assert_eq!(refused(&line(said)), None, "{said}");
    }
    for (said, refusal) in [
        ("mind edit deploy", Refusal::Editor),
        ("mindweaver edit --editor true deploy", Refusal::Editor),
        ("mind edit --contents - deploy", Refusal::Input),
        ("mind weaver edit --contents=- deploy", Refusal::Input),
        (
            "mindweaver edit -p alpha deploy --contents -",
            Refusal::Input,
        ),
    ] {
        assert_eq!(refused(&line(said)), Some(refusal), "{said}");
    }
}

/// So a front end can say which command it will not run, not only why.
#[test]
fn a_refused_task_names_the_line_it_is_refused_for() {
    let batch = Task::ApplyInstall {
        changes: vec![
            Task::Uninstall {
                projects: owned(&["alpha"]),
                kind: Kind::Skill,
                name: String::from("deploy"),
                force: false,
            },
            Task::OpenInstall,
        ],
    };

    assert_eq!(
        batch.refused_line(),
        Some((line("mindweaver install"), Refusal::Screen))
    );
    assert_eq!(Task::Shelf.refused_line(), None);
}

#[test]
fn a_refusal_says_why_by_a_code_of_its_own() {
    let codes: Vec<&str> = [
        Refusal::Screen,
        Refusal::Editor,
        Refusal::Input,
        Refusal::Shell,
        Refusal::Unheld,
        Refusal::Panel,
    ]
    .iter()
    .map(Refusal::code)
    .collect();

    assert_eq!(codes, Refusal::CODES);
    assert_eq!(
        Refusal::Screen.to_string(),
        "opens a screen of its own, which needs a terminal to draw on"
    );
}

#[test]
fn a_line_that_takes_something_away_is_asked_about_for_what_it_takes() {
    use Caution::{Forces, Removes, Uninstalls, Unlinks, Unloads};
    for (said, expected) in [
        ("mind remove --yes deploy", vec![Removes]),
        ("mind -C alpha remove -y rule:style", vec![Removes]),
        ("mindweaver remove --yes skill:deploy", vec![Removes]),
        ("mindweaver remove -p alpha --yes deploy", vec![Removes]),
        ("mind weaver remove --yes deploy", vec![Removes]),
        (
            "mindweaver remove --everywhere --yes skill:deploy",
            vec![Removes, Uninstalls],
        ),
        ("mind uninstall deploy", vec![Uninstalls]),
        ("mind uninstall --orphaned", vec![Uninstalls]),
        ("mind uninstall --force deploy", vec![Uninstalls, Forces]),
        (
            "mindweaver uninstall -p alpha -p beta skill:deploy",
            vec![Uninstalls],
        ),
        (
            "mindweaver uninstall --all-projects --orphaned",
            vec![Uninstalls],
        ),
        (
            "mindweaver uninstall -p alpha --force deploy",
            vec![Uninstalls, Forces],
        ),
        ("mind install --force deploy", vec![Forces]),
        ("mindweaver install -p alpha --force deploy", vec![Forces]),
        ("mind update --yes --force", vec![Forces]),
        ("mindweaver update --all --yes --force deploy", vec![Forces]),
        ("mind unlink", vec![Unlinks]),
        ("mindweaver unlink alpha beta", vec![Unlinks]),
        ("mind weaver unlink alpha", vec![Unlinks]),
        ("mind unload ../team", vec![Unloads]),
        ("mindweaver unload team", vec![Unloads]),
        ("mindweaver export ../backup --yes", vec![Removes]),
        (
            "mind weaver export --kind rules -y ../backup",
            vec![Removes],
        ),
    ] {
        assert_eq!(cautions(&line(said)), expected, "{said}");
    }
}

/// A dry run takes nothing away, and nor does a line that only adds or
/// carries what is installed to where its agents now read: each is shown
/// before it runs, and runs once asked.
#[test]
fn a_line_that_takes_nothing_away_is_asked_about_for_nothing() {
    for said in [
        "mind remove deploy",
        "mindweaver remove --everywhere skill:deploy",
        "mind update --force",
        "mindweaver update --all --force",
        "mind update --yes",
        "mindweaver install -p alpha deploy",
        "mindweaver link alpha",
        "mind add skill deploy Ship",
        "mind rename deploy ship",
        "mindweaver relink alpha beta",
        "mindweaver load ../team",
        "mindweaver adopt -p alpha --yes legacy",
        "mindweaver export ../backup",
        "mindweaver backup https://example.com/library.git",
        "mindweaver agents -p alpha --yes claude",
        "mind share --yes local",
        "mind targets set --yes skills .skills",
        "mindweaver migrate --library --yes",
        "mindweaver validate",
        "mindweaver init",
        // A line no parser takes would not run, and takes nothing.
        "mindweaver unlink",
        "mindweaver frobnicate",
        "ls",
    ] {
        assert_eq!(cautions(&line(said)), Vec::new(), "{said}");
    }
}

#[test]
fn a_task_is_asked_about_for_each_of_its_lines_that_takes_something_away() {
    let values = VALUES.into_iter().filter(|value| !value.is_empty());
    for task in values.flat_map(every) {
        let expected = match &task {
            Task::Remove {
                everywhere: false,
                yes: true,
                ..
            } => vec![(0, Caution::Removes)],
            Task::Remove {
                everywhere: true,
                yes: true,
                ..
            } => vec![(0, Caution::Removes), (0, Caution::Uninstalls)],
            Task::Uninstall { force: true, .. } => {
                vec![(0, Caution::Uninstalls), (0, Caution::Forces)]
            }
            Task::Uninstall { force: false, .. } | Task::UninstallOrphaned { .. } => {
                vec![(0, Caution::Uninstalls)]
            }
            Task::Update { force: true, .. } => vec![(0, Caution::Forces)],
            // What an earlier export wrote and the library no longer holds
            // is deleted from the folder.
            Task::Export { yes: true, .. } => vec![(0, Caution::Removes)],
            Task::Unlink { .. } => vec![(0, Caution::Unlinks)],
            Task::Unload { .. } | Task::UnloadHere { .. } => vec![(0, Caution::Unloads)],
            // Its dry run takes nothing.
            Task::RemoveSource { yes: true, .. } => vec![(0, Caution::Forgets)],
            // `mind -C <path> init`, `mindweaver link` and `mindweaver unlink`.
            Task::Minds { .. } | Task::Marks { .. } => vec![(2, Caution::Unlinks)],
            // Its installs first, then its uninstalls, as `every` builds it.
            Task::ApplyInstall { .. } => vec![(1, Caution::Uninstalls)],
            _ => Vec::new(),
        };

        assert_eq!(task.cautions(), expected, "{task:?}");
    }
}

/// Each by a word of its own in JSON, as the panel compares it, and its
/// words for whoever is asked.
#[test]
fn a_caution_says_what_it_takes_in_a_word_and_in_words() {
    let words: Vec<serde_json::Value> = [
        Caution::Removes,
        Caution::Uninstalls,
        Caution::Unlinks,
        Caution::Unloads,
        Caution::Forces,
        Caution::Forgets,
    ]
    .iter()
    .map(|caution| serde_json::to_value(caution).unwrap())
    .collect();

    assert_eq!(
        words,
        [
            "removes",
            "uninstalls",
            "unlinks",
            "unloads",
            "forces",
            "forgets"
        ]
    );
    assert_eq!(
        Caution::Unlinks.to_string(),
        "makes the workspace forget a project, though nothing on disk is deleted"
    );
}

#[test]
fn a_change_core_plans_is_the_install_or_uninstall_task_that_runs_it() {
    let install = Change::Install {
        projects: owned(&["alpha", "./beta"]),
        kind: Kind::Rule,
        name: "git/commits".into(),
        from: "load:../team".into(),
    };
    let uninstall = Change::Uninstall {
        projects: owned(&["alpha"]),
        kind: Kind::Skill,
        name: "deploy".into(),
    };

    assert_eq!(
        Task::from(install).commands(),
        [line(
            "mindweaver install -p alpha -p ./beta --from load:../team rule:git/commits"
        )]
    );
    assert_eq!(
        Task::from(uninstall).commands(),
        [line("mindweaver uninstall -p alpha skill:deploy")]
    );
}

/// The text, never a path: what a save reads is the workspace's scratch file
/// named by what the text holds, from the workspace's root, at either level,
/// held to the hash it was read with.
#[test]
fn a_save_is_edit_given_its_text_in_the_workspaces_scratch_file_and_the_hash_it_was_read_with() {
    let text = "---\nname: deploy\ndescription: Ship it\n---\n";
    let hash = mindweaver_core::hash::bytes(b"before");
    let route = scratch_route(text);
    let save = |holder| Task::Save {
        holder,
        kind: Kind::Skill,
        name: String::from("deploy"),
        text: String::from(text),
        hash: hash.to_string(),
    };

    let own = save(Holder::Workspace);
    let alpha = save(Holder::Project(String::from("alpha")));

    let edit = [
        "--contents",
        &route,
        "--if-unchanged",
        &hash,
        "skill:deploy",
    ];
    assert_eq!(
        own.commands(),
        [[&["mindweaver", "edit"][..], &edit].concat()]
    );
    assert_eq!(
        alpha.commands(),
        [[&["mindweaver", "edit", "-p", "alpha"][..], &edit].concat()]
    );
    let named = &mindweaver_core::hash::bytes(text.as_bytes())[..16];
    assert_eq!(scratch_name(text), format!("edit-{named}.md"));
    assert_eq!(route, format!(".mindweaver/tmp/{}", scratch_name(text)));
    assert_ne!(scratch_route("another text"), route);
    // In a workspace a Mindflayer build made, its own scratch folder.
    let (_dir, old) = mindweaver_core::mother::MindflayerMother::empty();
    assert_eq!(
        own.commands_in(&old),
        [[
            &["mindweaver", "edit", "--contents"][..],
            &[format!(".mindflayer/tmp/{}", scratch_name(text)).as_str()],
            &edit[2..]
        ]
        .concat()]
    );
    for task in [own, alpha] {
        assert!(task.writes(), "{task:?}");
        assert_eq!(task.refusal(), None, "{task:?}");
        assert_eq!(task.cautions(), [], "{task:?}");
    }
}

// ---------------------------------------------------------------------------
// Values nobody chose
// ---------------------------------------------------------------------------

/// `count` values drawn anew each run from core's mothers, where [`hostile`]
/// draws the same ones from a fixed seed: text of any script, spaces, quotes
/// and an `=` among it, led by the shape of an option as often as not, so a
/// run tries what no seed reached.
///
/// None is empty, `.` or the default folder of a gather, each of which a
/// line spells otherwise on purpose (the project at the root, a folder left
/// unsaid); and none holds `://`, which with an `@` after it is a URL's
/// credential, taken out of a line where it is shown.
fn drawn(count: usize) -> Vec<String> {
    use mindweaver_core::mother::random;

    const LEADS: [&str; 10] = [
        "", "", "", "-", "--", "-p", "--yes", "--from=", "-C ", "--=",
    ];
    let mut values = Vec::new();
    while values.len() < count {
        let value = format!(
            "{}{}",
            random::pick(&LEADS),
            random::text(random::between(1, 20))
        );
        let special = value.is_empty() || value == "." || value == DEFAULT_SUBDIRECTORY;
        // A URL's credentials are left out of the line shown, so a value
        // reading as one is no value this test can hold to its words: one
        // holding `://`, and one starting `//`, which a name's `skill:`
        // makes `skill://`, a URL whose user is whatever comes before an
        // `@` (it was drawn once, as `//XЖ@0@...`).
        if !special && !value.contains("://") && !value.starts_with("//") {
            values.push(value);
        }
    }
    values
}

#[test]
fn values_drawn_at_random_reach_their_command_as_given() {
    let mut parsers = Parsers::new();
    let plain: Vec<Vec<Read>> = every(GIVEN)
        .iter()
        .map(|task| {
            task.commands()
                .iter()
                .map(|line| parsers.read(line))
                .collect()
        })
        .collect();
    for value in drawn(60) {
        for (task, expected) in every(&value).iter().zip(&plain) {
            let lines = task.commands();
            assert_eq!(lines.len(), expected.len(), "{task:?}");
            for (line, read) in lines.iter().zip(expected) {
                assert_eq!(
                    parsers.read(line),
                    holding(read, &value),
                    "{line:?} does not hold {value:?} where a plain value was"
                );
            }
        }
    }
}

#[test]
fn a_line_holding_values_drawn_at_random_is_shown_as_the_words_it_runs() {
    for value in drawn(60) {
        for task in every(&value) {
            for line in task.commands() {
                let shown = shell(&line);

                assert_eq!(words_of(&shown), line, "{value:?}: {shown}");
            }
        }
    }
}

/// The tasks that carry a URL to a line: a gather's address, a backup's
/// repository, and an install's origin, which names a source by its
/// address.
fn carrying(url: &str) -> Vec<Task> {
    vec![
        Task::Backup {
            url: url.to_owned(),
            branch: Some(String::from("library")),
            kinds: vec![Kind::Skill],
            message: None,
            token_env: Some(String::from("GITHUB_TOKEN")),
        },
        Task::Gather {
            url: url.to_owned(),
            folder: String::from(DEFAULT_SUBDIRECTORY),
            reference: None,
            kind: Kind::Skill,
        },
        Task::Gather {
            url: url.to_owned(),
            folder: String::from("rules"),
            reference: Some(String::from("main")),
            kind: Kind::Rule,
        },
        Task::Install {
            projects: vec![String::from("alpha")],
            kind: Kind::Skill,
            name: String::from("deploy"),
            from: Some(url.to_owned()),
            yes: false,
        },
    ]
}

/// The words run as typed, so a URL with a credential in it reaches the
/// command, which refuses it, rather than being fetched without one; what
/// is shown of them is the line without it, and pasted into a shell runs
/// that line, for every way a credential is typed and words quoting one.
#[test]
fn a_line_carrying_a_credential_runs_it_as_typed_and_is_shown_without_it() {
    use mindweaver_core::mother::UrlMother;

    for url in UrlMother::every_credentialed() {
        for url in [UrlMother::quoted(&url), url] {
            for task in carrying(&url.typed) {
                let lines = task.commands();
                let line = &lines[0];
                assert!(line.contains(&url.typed), "{line:?} lost {:?}", url.typed);

                let shown = shell(line);
                let described = task.describe();

                for said in [&shown, &described] {
                    assert!(
                        !said.contains(&url.secret),
                        "{:?} is shown with its secret: {said}",
                        url.typed
                    );
                }
                let without: Vec<String> = line
                    .iter()
                    .map(|word| word.replace(&url.typed, &url.clean))
                    .collect();
                assert_eq!(words_of(&shown), without, "{:?}: {shown}", url.typed);
                assert_eq!(described, shown, "{task:?}");
            }
        }
    }
}

/// A yes to an install that waits for one is a front end's that has shown
/// what each agent would run: in JSON for the panel's confirmation, and
/// left out, by a script or a front end from before it could be sent, it
/// is none.
#[test]
fn an_install_says_yes_on_its_line_and_in_its_json() {
    let task = Task::Install {
        projects: vec![String::from("alpha")],
        kind: Kind::Mcp,
        name: String::from("github"),
        from: Some(String::from("load:../team")),
        yes: true,
    };

    assert_eq!(
        task.describe(),
        "mindweaver install -p alpha --from load:../team --yes mcp:github"
    );
    let json = serde_json::to_value(&task).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "type": "install",
            "projects": ["alpha"],
            "kind": "mcp",
            "name": "github",
            "from": "load:../team",
            "yes": true,
        })
    );
    assert_eq!(serde_json::from_value::<Task>(json).unwrap(), task);
    let unsaid = serde_json::json!({
        "type": "install",
        "projects": ["alpha"],
        "kind": "mcp",
        "name": "github",
        "from": "load:../team",
    });
    let read: Task = serde_json::from_value(unsaid).unwrap();
    assert!(matches!(read, Task::Install { yes: false, .. }), "{read:?}");
}

#[test]
fn a_change_confirmed_says_yes_only_of_an_install_that_waits_for_one() {
    use mindweaver_core::install::Change;

    let install = |kind: Kind, from: &str| Change::Install {
        projects: vec![String::from("alpha")],
        kind,
        name: "github".into(),
        from: from.into(),
    };
    let yes = |task: Task| matches!(task, Task::Install { yes: true, .. });

    assert!(yes(Task::confirmed(install(Kind::Mcp, "load:../team"))));
    assert!(yes(Task::confirmed(install(
        Kind::Mcp,
        "https://example.com/acme/servers"
    ))));
    assert!(!yes(Task::confirmed(install(Kind::Mcp, "workspace"))));
    assert!(!yes(Task::confirmed(install(Kind::Skill, "load:../team"))));
    // Built without the confirmation, as a plan's task and the palette's
    // are, it carries none, in JSON as on its line.
    let plain = Task::from(install(Kind::Mcp, "load:../team"));
    assert!(!yes(plain.clone()));
    assert_eq!(serde_json::to_value(&plain).unwrap()["yes"], false);
    assert!(!plain.commands()[0].contains(&String::from("--yes")));
}

#[test]
fn a_servers_add_and_edit_are_its_options_words_each_secret_a_variables_name() {
    let program = mcp::Given::Program(mcp::Program {
        command: String::from("npx"),
        args: vec![String::from("-y"), String::from("@acme/server")],
        env: vec![(String::from("LOG_LEVEL"), String::from("info"))],
        secret_env: vec!["GITHUB_TOKEN".into()],
        arg_env: vec!["WORKSPACE".into()],
    });
    let address = mcp::Given::Address(mcp::Address {
        url: String::from("https://{tenant}.example.com/mcp"),
        events: true,
        headers: vec![(String::from("X-Client"), String::from("me"))],
        secret_headers: vec![(String::from("X-Api-Key"), "API_KEY".into())],
        bearer: Some("TOKEN".into()),
        url_vars: vec![("tenant".into(), "TENANT".into())],
    });

    let added = Task::Add {
        holder: Holder::Workspace,
        kind: Kind::Mcp,
        name: String::from("github"),
        description: String::from("GitHub"),
        template: None,
        server: Some(Box::new(program)),
    };
    let edited = Task::EditServer {
        holder: Holder::Project(String::from("api")),
        name: String::from("github"),
        server: Box::new(address),
    };

    assert_eq!(
        added.commands(),
        [owned(&[
            "mindweaver",
            "add",
            "--command",
            "npx",
            "--arg=-y",
            "--arg",
            "@acme/server",
            "--env",
            "LOG_LEVEL=info",
            "--secret-env",
            "GITHUB_TOKEN",
            "--arg-env",
            "WORKSPACE",
            "mcp",
            "github",
            "GitHub",
        ])]
    );
    assert_eq!(
        edited.commands(),
        [owned(&[
            "mind",
            "-C",
            "api",
            "edit",
            "--url",
            "https://{tenant}.example.com/mcp",
            "--sse",
            "--header",
            "X-Client=me",
            "--secret-header",
            "X-Api-Key=API_KEY",
            "--bearer-env",
            "TOKEN",
            "--url-var",
            "tenant=TENANT",
            "mcp:github",
        ])]
    );
    assert_eq!(added.refusal(), None);
    assert_eq!(edited.refusal(), None);
}

/// An export names each kind asked for by its folder's word, and none for
/// every kind, and says `--yes` only when told; the folder is a value
/// wherever it starts.
#[test]
fn an_export_is_the_line_the_cli_takes_and_told_yes_only_when_asked() {
    for (task, said) in [
        (
            Task::Export {
                folder: String::from("/home/you/backup"),
                kinds: Vec::new(),
                yes: false,
            },
            "mindweaver export /home/you/backup",
        ),
        (
            Task::Export {
                folder: String::from("/home/you/backup"),
                kinds: vec![Kind::Skill, Kind::Mcp],
                yes: true,
            },
            "mindweaver export --kind skills --kind mcp --yes /home/you/backup",
        ),
        (
            Task::Export {
                folder: String::from("-odd"),
                kinds: vec![Kind::Rule],
                yes: false,
            },
            "mindweaver export --kind rules -- -odd",
        ),
    ] {
        assert_eq!(task.commands(), [words_of(said)], "{task:?}");
        assert!(task.writes(), "{task:?}");
    }
}

/// A backup says each option only when it has something to say, the
/// variable a token is read from by its name, and the repository last.
#[test]
fn a_backup_is_the_line_the_cli_takes_naming_a_token_by_its_variable() {
    for (task, said) in [
        (
            Task::Backup {
                url: String::from("https://github.com/you/library.git"),
                branch: None,
                kinds: Vec::new(),
                message: None,
                token_env: None,
            },
            "mindweaver backup https://github.com/you/library.git",
        ),
        (
            Task::Backup {
                url: String::from("git@github.com:you/library.git"),
                branch: Some(String::from("library")),
                kinds: vec![Kind::Skill, Kind::Rule],
                message: Some(String::from("- weekly")),
                token_env: Some(String::from("GITHUB_TOKEN")),
            },
            "mindweaver backup --branch library --kind skills --kind rules \
             '--message=- weekly' --token-env GITHUB_TOKEN git@github.com:you/library.git",
        ),
    ] {
        assert_eq!(task.commands(), [words_of(said)], "{task:?}");
        assert!(task.writes() && task.runs_apart(), "{task:?}");
        assert!(task.cautions().is_empty(), "{task:?}");
    }
}

/// The TUI's search of the registry reads what the line prints back as
/// JSON, and names no registry: the official one is the only one a front
/// end asks.
#[test]
fn a_search_of_the_registry_is_its_json_line_naming_no_registry() {
    let task = Task::SearchServers {
        query: String::from("air table"),
    };

    let mut expected = line("mindweaver gather mcp search --json");
    expected.push(String::from("air table"));
    assert_eq!(task.commands(), vec![expected]);
    assert_eq!(
        task.describe(),
        "mindweaver gather mcp search --json 'air table'"
    );
    assert!(!task.writes() && !task.runs_apart());
    assert_eq!(task.refusal(), None);
    assert!(task.cautions().is_empty());
}

/// An add is its dry run until it is told yes, which is the one that
/// writes, and neither starts anything nor takes anything away.
#[test]
fn an_import_from_the_registry_is_its_dry_run_until_it_is_told_yes() {
    let dry = Task::ImportServer {
        server: String::from("io.github.buildkite/buildkite-mcp-server"),
        version: Some(String::from("0.7.0")),
        yes: false,
    };
    let yes = Task::ImportServer {
        server: String::from("ai.clawfetch/mcp"),
        version: None,
        yes: true,
    };

    assert_eq!(
        dry.commands(),
        [line(
            "mindweaver gather mcp add --version 0.7.0 io.github.buildkite/buildkite-mcp-server"
        )]
    );
    assert_eq!(
        yes.commands(),
        [line("mindweaver gather mcp add --yes ai.clawfetch/mcp")]
    );
    assert!(!dry.writes() && yes.writes());
    for task in [dry, yes] {
        assert!(!task.runs_apart(), "{task:?}");
        assert_eq!(task.refusal(), None, "{task:?}");
        assert!(task.cautions().is_empty(), "{task:?}");
    }
}

/// The TUI's search reads what the line prints back as JSON, and hands no
/// token over, nor the name of a variable holding one (D-16).
#[test]
fn a_search_of_the_skill_directories_is_its_json_line_with_no_token() {
    let task = Task::SearchSkills {
        query: String::from("pdf forms"),
    };

    let mut expected = line("mindweaver gather search --json");
    expected.push(String::from("pdf forms"));
    assert_eq!(task.commands(), vec![expected]);
    assert_eq!(
        task.describe(),
        "mindweaver gather search --json 'pdf forms'"
    );
    assert!(!task.writes());
    assert!(task.runs_apart());
    assert_eq!(task.refusal(), None);
    assert!(task.cautions().is_empty());
}

/// A result the panel sends back to take is gathered by the line the
/// TUI's take and `gather search --take` gather it by: from its repository
/// and its folder there at the branch a finder named, or, naming neither,
/// the whole repository at its default branch, the folder left unsaid as
/// a gather of skills leaves it.
#[test]
fn a_result_taken_is_the_gather_of_its_repository_folder_and_branch() {
    let mut found = hit("https://github.com/acme/skills", true);
    found.path = Some(String::from("skills/pdf"));
    found.reference = Some(String::from("main"));
    let at_root = hit("/home/you/src/house-rules", false);

    let placed = Task::TakeSkill { hit: found.clone() };
    let whole = Task::TakeSkill {
        hit: at_root.clone(),
    };

    assert_eq!(
        placed.commands(),
        [line(
            "mindweaver gather git --path skills/pdf --ref main https://github.com/acme/skills"
        )]
    );
    assert_eq!(
        whole.commands(),
        [line("mindweaver gather git /home/you/src/house-rules")]
    );
    assert_eq!(placed.commands(), Task::taking(&found).commands());
    assert_eq!(whole.commands(), Task::taking(&at_root).commands());
    for task in [placed, whole] {
        assert!(task.writes(), "{task:?}");
        assert!(task.runs_apart(), "{task:?}");
        assert_eq!(task.refusal(), None, "{task:?}");
        assert!(task.cautions().is_empty(), "{task:?}");
    }
}

/// What the panel sends to take a result is the result as the search's
/// JSON gave it, beside its tag, and nothing a browser could aim elsewhere:
/// no address to ask, no token, no folder of its own.
#[test]
fn a_result_taken_goes_by_its_tag_with_the_result_as_the_search_gave_it() {
    let task = Task::TakeSkill {
        hit: hit("https://github.com/acme/skills", false),
    };

    assert_eq!(
        serde_json::to_value(&task).unwrap(),
        serde_json::json!({
            "type": "take_skill",
            "hit": {
                "name": "plain",
                "repository": "https://github.com/acme/skills",
                "path": null,
                "reference": null,
                "installs": 56,
                "stars": null,
                "finders": ["skills.sh"],
            },
        })
    );
}

/// The catalogs a front end was started with are named by each line that
/// asks one, `--registry` by a search of the registry and an add from it,
/// `--finder-url` by a search of the skill directories; every other line
/// is the one it is without them, a take's gather among them, which asks
/// the repository.
#[test]
fn the_catalogs_a_front_end_was_started_with_are_named_by_each_line_asking_one() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = WeaverWorkspace::unmade(dir.path()).unwrap();
    let catalogs = Catalogs {
        registry: Some(String::from("http://127.0.0.1:4100")),
        finders: vec![
            String::from("skills.sh=http://127.0.0.1:4200"),
            String::from("claude-plugins.dev=https://mirror.example"),
        ],
    };
    let finders = "--finder-url skills.sh=http://127.0.0.1:4200 \
                   --finder-url claude-plugins.dev=https://mirror.example";

    let asked = |task: &Task| task.commands_asking(&workspace, &catalogs);
    assert_eq!(
        asked(&Task::SearchSkills {
            query: String::from("pdf")
        }),
        [line(&format!(
            "mindweaver gather search {finders} --json pdf"
        ))]
    );
    assert_eq!(
        asked(&Task::SearchServers {
            query: String::from("clawfetch")
        }),
        [line(
            "mindweaver gather mcp search --registry http://127.0.0.1:4100 --json clawfetch"
        )]
    );
    assert_eq!(
        asked(&Task::ImportServer {
            server: String::from("ai.clawfetch/mcp"),
            version: Some(String::from("0.4.0")),
            yes: true,
        }),
        [line(
            "mindweaver gather mcp add --registry http://127.0.0.1:4100 --version 0.4.0 --yes \
             ai.clawfetch/mcp"
        )]
    );
    for value in VALUES {
        for task in every(value) {
            if matches!(
                task,
                Task::SearchSkills { .. } | Task::SearchServers { .. } | Task::ImportServer { .. }
            ) {
                continue;
            }
            assert_eq!(asked(&task), task.commands_in(&workspace), "{task:?}");
        }
    }
}

/// A catalog's address starting with a dash still reaches its option as
/// its value, joined to it by `=`, as every value does: `--yes` as a
/// registry's address says no yes.
#[test]
fn a_catalog_s_address_starting_with_a_dash_is_still_its_option_s_value() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = WeaverWorkspace::unmade(dir.path()).unwrap();
    let catalogs = Catalogs {
        registry: Some(String::from("--yes")),
        finders: vec![String::from("--json")],
    };
    let mut parsers = Parsers::new();
    let given = |read: &Read, id: &str| -> Option<Vec<String>> {
        read.iter()
            .flat_map(|(_, given)| given)
            .find(|given| given.id == id)
            .map(|given| given.values.clone())
    };

    let search = Task::SearchSkills {
        query: String::from("pdf"),
    };
    let read = parsers.read(&search.commands_asking(&workspace, &catalogs)[0]);
    assert_eq!(given(&read, "at"), Some(vec![String::from("--json")]));
    let servers = Task::SearchServers {
        query: String::from("pdf"),
    };
    let read = parsers.read(&servers.commands_asking(&workspace, &catalogs)[0]);
    assert_eq!(given(&read, "registry"), Some(vec![String::from("--yes")]));
    let dry = Task::ImportServer {
        server: String::from("ai.clawfetch/mcp"),
        version: None,
        yes: false,
    };
    let read = parsers.read(&dry.commands_asking(&workspace, &catalogs)[0]);
    assert_eq!(given(&read, "registry"), Some(vec![String::from("--yes")]));
    assert_eq!(given(&read, "yes"), None, "{read:?}");
}
