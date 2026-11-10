//! What each command does: `run` and `run_weaver_cli` dispatch a parsed
//! command line, and each family of commands has a module of its own.
//!
//! Split by family so that a change to one command touches one file, and two
//! pieces of work on different commands do not collide in the same one.

pub(crate) mod activate;
pub(crate) mod adopt;
pub(crate) mod artifact;
pub(crate) mod backup;
pub(crate) mod doctor;
pub(crate) mod export;
pub(crate) mod gather;
pub(crate) mod install;
pub(crate) mod known;
pub(crate) mod migrate;
pub(crate) mod project;
pub(crate) mod read;
pub(crate) mod settings;
pub(crate) mod status;
#[cfg(feature = "ui")]
pub(crate) mod ui;
pub(crate) mod update;
pub(crate) mod workspace;

use std::path::{Path, PathBuf};

use mindweaver_core::gather::Watch;
use mindweaver_core::resolve::{self, resolve, Intent};
use mindweaver_core::url;
use mindweaver_core::view::Listing;
use mindweaver_core::{
    paths, scan, Catalog, Change, MindProject, Names, Reference, Share, WeaverWorkspace,
};

use clap::Parser as _;

use crate::homes::Lookup;
use crate::render::json_or_text;
use crate::task::Refusal;
use crate::{
    tui, Cli, CliError, Command, Failure, Outcome, Warning, WeaverCli, WeaverCommand,
    WorkspacesCommand,
};
use activate::{activate_command, which_command};
use adopt::adopt_from;
use artifact::{
    add_to, edit_in, remove_everywhere, remove_from, remove_own, rename_in, templates_of,
    would_remove_everywhere,
};
use backup::{backup_to, Backing};
use doctor::{doctor_here, doctor_of_project, doctor_of_workspace};
use export::export_into;
use gather::run_gather;
use install::{install_named, uninstall_named, uninstall_orphans};
use known::{forget_command, workspaces_command};
use migrate::{migrate_all, migrate_here, migrate_library, migrate_names, migrate_one, Asked};
use project::{
    init_project, init_workspace, link_here, link_projects, relink_project, renew_here,
    scan_workspace, unlink_here, unlink_projects,
};
use read::{
    everywhere, everywhere_and_the_shelf, list, list_workspace, listed_here, one_project, show,
    validate, wanted, Level, Selector,
};
use settings::{agents_change, change_here, change_of, targets_change};
use status::{status_here, status_of_project, status_of_workspace};
use update::update_projects;
use workspace::{load_command, unload_command};

/// Run `mind`.
pub fn run(cli: &Cli) -> Result<Outcome, Failure> {
    run_watched(cli, &Watch::new())
}

/// Run `mind`, telling `watch` how a gather goes and stopping one when it is
/// cancelled: how a front end that shows progress, or can stop what it
/// started, runs the same line `run` does.
///
/// A parameter beside the parsed line rather than a field of it, because it
/// is not something a person types: the line stays exactly what was typed.
pub fn run_watched(cli: &Cli, watch: &Watch) -> Result<Outcome, Failure> {
    let lookup = Lookup::given(
        cli.home.as_deref(),
        cli.workspace.as_deref(),
        &cli.agent_homes,
    );
    let moving = match &cli.command {
        Some(Command::Migrate { names, .. }) => *names,
        Some(Command::Weaver {
            command: Some(command),
        }) => moves_names(command),
        _ => false,
    };
    let result = run_with(cli, &lookup, watch);
    looked_up(result, &lookup, cli.directory.as_deref(), moving)
}

/// Whether `command` is the move to Mindweaver's names, which says what it
/// moves itself.
fn moves_names(command: &WeaverCommand) -> bool {
    matches!(command, WeaverCommand::Migrate { names: true, .. })
}

/// `result`, what a line typed in a shell came to, with what deciding where
/// to look had to say: first each variable taken under the name a
/// Mindflayer build read ([`Lookup::older`]), and last, of the workspace
/// the line used, read again once it has run, how to move it to
/// Mindweaver's names when a Mindflayer build made it, and the old folder
/// left beside a new one, which an older build reads instead (D-26).
///
/// Said once a line, by the two entry points a shell reaches alone, since
/// that is where the environment was read and where somebody who can move
/// the workspace reads what is said: a line the TUI or the panel runs
/// reads none, so its own launch says it once, as it closes. Said whatever
/// the line did, a read as well as a write, being about the workspace
/// rather than the line; never by the move itself (`moving`), which says
/// what it moves. Notes and a hint, which reach no envelope and fail
/// nothing: what was read is as it was written.
fn looked_up(
    mut result: Result<Outcome, Failure>,
    lookup: &Lookup,
    directory: Option<&Path>,
    moving: bool,
) -> Result<Outcome, Failure> {
    let said: Vec<Warning> = lookup
        .older
        .iter()
        .map(|older| Warning::from(older.note()))
        .collect();
    // Read again rather than handed up from the command, which resolved it
    // wherever it needed to, and as it is now: a line that moved it, or made
    // a new one, is said nothing of the old.
    let used = working_directory(directory)
        .ok()
        .and_then(|directory| resolve(&lookup.inputs(&directory, Intent::Read)).ok())
        .flatten()
        .map(|resolved| resolved.workspace);
    let mut after: Vec<Warning> = Vec::new();
    if let Some(workspace) = used.filter(|_| !moving) {
        if let Some(leftover) = workspace.leftover() {
            after.push(Warning::from(format!(
                "note: {}: left over beside the {} the workspace is read from, which an older \
                 build reads instead; remove it, or move it aside, once nothing needs it",
                leftover.display(),
                workspace.weaver_dir().display()
            )));
        }
        if workspace.names() == Names::Mindflayer {
            after.push(Warning::from(format!(
                "hint: the workspace at {} was made by a Mindflayer build, and is kept under \
                 its names; `mindweaver migrate --names` moves it, and what it wrote in its \
                 projects, to Mindweaver's",
                workspace.root().display()
            )));
        }
    }
    match &mut result {
        Ok(outcome) => {
            outcome.stderr.splice(0..0, said);
            outcome.stderr.extend(after);
        }
        Err(failure) => {
            failure.warnings.splice(0..0, said);
            failure.warnings.extend(after);
        }
    }
    result
}

/// Run `mind` with where to look for a workspace already decided: what
/// [`run_watched`] does once it has read that from the command line and the
/// environment, and what the TUI does with its own, so the lines it runs look
/// where its screen did. What `cli` says about it is not read again.
pub(crate) fn run_with(cli: &Cli, lookup: &Lookup, watch: &Watch) -> Result<Outcome, Failure> {
    let directory = working_directory(cli.directory.as_deref())?;
    match cli.command.as_ref().unwrap_or(&Command::Tui) {
        Command::Weaver { command } => run_weaver(
            command.as_ref().unwrap_or(&WeaverCommand::Tui),
            &directory,
            lookup,
            watch,
        ),
        Command::Init { new_id: true, .. } => renew_here(&directory, lookup),
        Command::Init {
            agents,
            shared,
            skills,
            rules,
            new_id: false,
        } => init_project(
            &directory,
            agents,
            if *shared { Share::Shared } else { Share::Local },
            skills.as_deref(),
            rules.as_deref(),
        ),
        Command::Add {
            kind,
            name,
            description,
            template,
            server,
        } => {
            let project = project_here(&directory)?;
            let given = server.given();
            match template {
                // The workspace lends the template, so one that cannot be
                // read refuses the add: passed over, it would let a farther
                // template of the same name, a built-in one say, stand in
                // for its own and write the artifact from that.
                Some(template) => {
                    let workspace = consulted(&project, lookup)?;
                    add_to(
                        workspace.as_ref(),
                        &project,
                        *kind,
                        (name, description),
                        Some(template),
                        given.as_ref(),
                    )
                }
                None => consulting(&project, lookup, |workspace| {
                    add_to(
                        workspace,
                        &project,
                        *kind,
                        (name, description),
                        None,
                        given.as_ref(),
                    )
                }),
            }
        }
        Command::Edit { reference, editing } => {
            let project = project_here(&directory)?;
            consulting(&project, lookup, |workspace| {
                edit_in(workspace, &project, reference, editing, &directory)
            })
        }
        Command::Rename { reference, to } => {
            let project = project_here(&directory)?;
            consulting(&project, lookup, |workspace| {
                rename_in(workspace, &project, reference, to)
            })
        }
        Command::Remove { reference, yes } => {
            let project = project_here(&directory)?;
            consulting(&project, lookup, |workspace| {
                remove_from(workspace, &project, reference, *yes)
            })
        }
        // Laid out as JSON once the workspace consulted has said whether it
        // could be read, which reaches the envelope as it reaches stderr.
        Command::Templates { json } => {
            let project = project_here(&directory)?;
            let mut found = None;
            let text = consulting(&project, lookup, |workspace| {
                templates_of(workspace, &project, &mut found)
            })?;
            Ok(json_or_text(found.filter(|_| *json), text))
        }
        Command::List { kind, json } => {
            let project = project_here(&directory)?;
            if *json {
                return listed_here(project, wanted(*kind), lookup);
            }
            one_project(project, wanted(*kind), |catalog, projects, level| {
                Ok(list(catalog, projects, level))
            })
        }
        Command::Show { reference, json } => {
            let project = project_here(&directory)?;
            let reference = Reference::parse(reference);
            let mut found = None;
            let text = one_project(project, wanted(reference.kind()), |catalog, _, level| {
                show(catalog, &reference, level, &mut found)
            })?;
            Ok(json_or_text(found.filter(|_| *json), text))
        }
        Command::Validate { target, json } => {
            let project = project_here(&directory)?;
            let selector = Selector::parse(target.as_deref());
            let mut found = None;
            let text = one_project(project, selector.kinds(), |catalog, _, level| {
                validate(catalog, &selector, level, &mut found)
            })?;
            Ok(json_or_text(found.filter(|_| *json), text))
        }
        Command::Status { json } => status_here(project_here(&directory)?, lookup, *json),
        Command::Doctor { json } => doctor_here(project_here(&directory)?, lookup, *json),
        Command::Link => link_here(&directory, lookup),
        Command::Unlink => unlink_here(&directory, lookup),
        Command::Load { workspaces } => {
            let project = project_here(&directory)?;
            load_command(&directory, project.root(), workspaces, lookup)
        }
        Command::Unload { workspaces } => {
            let project = project_here(&directory)?;
            unload_command(&directory, project.root(), workspaces, lookup)
        }
        Command::Install {
            name,
            from,
            force,
            yes,
        } => {
            let project = project_here(&directory)?;
            let workspace = managing(&project, lookup)?;
            install_named(
                &workspace,
                &[project],
                name,
                from.as_deref(),
                &directory,
                &lookup.agents,
                *force,
                *yes,
            )
        }
        // The parser holds a name and --orphaned apart, and asks for one.
        Command::Uninstall { name, force, .. } => {
            let project = project_here(&directory)?;
            let workspace = managing(&project, lookup)?;
            match name {
                Some(name) => uninstall_named(&workspace, &[project], name, &lookup.agents, *force),
                None => uninstall_orphans(&workspace, &[project], &lookup.agents, *force),
            }
        }
        Command::Update { names, force, yes } => {
            let project = project_here(&directory)?;
            let workspace = managing_unmade(&project, lookup)?;
            update_projects(
                &workspace,
                &[project],
                names,
                *force,
                *yes,
                &lookup.agents,
                Vec::new(),
            )
        }
        Command::Agents { change, named } => {
            let (change, yes) = agents_change(change.as_ref(), named);
            change_here(&project_here(&directory)?, &change, yes, lookup)
        }
        Command::Share { mode } => change_here(
            &project_here(&directory)?,
            &Change::Share {
                share: mode.share,
                edit_tracked: mode.edit_tracked,
            },
            mode.yes,
            lookup,
        ),
        Command::Targets { change } => {
            let (change, yes) = targets_change(change);
            change_here(&project_here(&directory)?, &change, yes, lookup)
        }
        Command::Migrate {
            names: true, yes, ..
        } => {
            let project = project_here(&directory)?;
            match consulted(&project, lookup)? {
                Some(workspace) => migrate_names(&workspace, *yes),
                None => Err(CliError::NoWorkspaceAbove(project.root().to_path_buf()).into()),
            }
        }
        Command::Migrate {
            settings,
            yes,
            names: false,
        } => migrate_here(
            &project_here(&directory)?,
            &Asked {
                agents: &settings.agents,
                share: settings.share(),
                yes: *yes,
            },
            lookup,
        ),
        Command::Tui => tui::run_project(&directory, lookup),
        #[cfg(feature = "ui")]
        Command::Ui(options) => ui::ui_command(&directory, options, lookup),
    }
}

/// Run `mindweaver`, which is the same as running `mind weaver`.
pub fn run_weaver_cli(cli: &WeaverCli) -> Result<Outcome, Failure> {
    run_weaver_cli_watched(cli, &Watch::new())
}

/// Run `mindweaver` with a gather's progress and cancel going through `watch`,
/// as [`run_watched`] does for `mind`.
pub fn run_weaver_cli_watched(cli: &WeaverCli, watch: &Watch) -> Result<Outcome, Failure> {
    let lookup = Lookup::given(
        cli.home.as_deref(),
        cli.workspace.as_deref(),
        &cli.agent_homes,
    );
    let moving = cli.command.as_ref().is_some_and(moves_names);
    let result = run_weaver_with(cli, &lookup, watch);
    looked_up(result, &lookup, cli.directory.as_deref(), moving)
}

/// [`run_with`], for `mindweaver`.
pub(crate) fn run_weaver_with(
    cli: &WeaverCli,
    lookup: &Lookup,
    watch: &Watch,
) -> Result<Outcome, Failure> {
    let directory = working_directory(cli.directory.as_deref())?;
    run_weaver(
        cli.command.as_ref().unwrap_or(&WeaverCommand::Tui),
        &directory,
        lookup,
        watch,
    )
}

/// What came of one command line a front end handed over to run.
#[derive(Debug)]
pub(crate) enum Ran {
    /// A command of the list every front end shares ([`Refusal`]) that the
    /// one running it does not run: nothing of it ran.
    Refused(Refusal),
    /// It ran, or could not, as a terminal would say: a line neither parser
    /// takes among the second, in the parser's words.
    Done(Result<Outcome, Failure>),
}

/// Run one command line, `words`, through the real parsers, as typed from
/// `base`: what every front end that runs lines rather than reading them
/// from a terminal, the TUI and the panel's backend, runs each one with.
///
/// A `-C` or a `-W` among the words is resolved against `base` rather than
/// against wherever this process happened to start, as somebody reading
/// the line takes it. Where to look for a workspace is otherwise `lookup`,
/// the front end's own, passed along rather than looked up again, so no line
/// reads the environment the front end was opened in. A gather among them
/// reports to `watch`, and stops when it is cancelled. A command `refuses`
/// says the front end does not run is refused before anything of it runs.
///
/// One function, so the two front ends cannot come to resolve one line two
/// ways; each lays out what came of it as it shows lines.
pub(crate) fn run_line(
    words: &[String],
    base: &Path,
    lookup: &Lookup,
    watch: &Watch,
    refuses: impl Fn(Refusal) -> bool,
) -> Ran {
    let from = |directory: Option<PathBuf>| {
        Some(directory.map_or_else(|| base.to_path_buf(), |directory| base.join(directory)))
    };
    // A line's own `-W` is the whole answer, as on a command line; empty
    // stays empty rather than becoming the root, which joining would make it.
    let looking = |named: Option<String>| match named {
        Some(named) => Lookup {
            flag: Some(if named.is_empty() {
                PathBuf::new()
            } else {
                base.join(named)
            }),
            activated: None,
            ..lookup.clone()
        },
        None => lookup.clone(),
    };
    let unparsed = |error: String| Ran::Done(Err(CliError::Unparsed(error).into()));

    match words.first().map(String::as_str) {
        Some("mind") => match Cli::try_parse_from(words) {
            Err(error) => unparsed(error.to_string()),
            Ok(cli) => match Refusal::of_mind(cli.command.as_ref()).filter(|r| refuses(*r)) {
                Some(refusal) => Ran::Refused(refusal),
                None => {
                    let mut cli = cli;
                    cli.directory = from(cli.directory.take());
                    let lookup = looking(cli.workspace.take());
                    Ran::Done(run_with(&cli, &lookup, watch))
                }
            },
        },
        Some("mindweaver") => match WeaverCli::try_parse_from(words) {
            Err(error) => unparsed(error.to_string()),
            Ok(cli) => match Refusal::of_weaver(cli.command.as_ref()).filter(|r| refuses(*r)) {
                Some(refusal) => Ran::Refused(refusal),
                None => {
                    let mut cli = cli;
                    cli.directory = from(cli.directory.take());
                    let lookup = looking(cli.workspace.take());
                    Ran::Done(run_weaver_with(&cli, &lookup, watch))
                }
            },
        },
        _ => unparsed(String::from("not a `mind` or `mindweaver` command")),
    }
}

/// The workspace half, shared by `mind weaver <cmd>` and `mindweaver <cmd>`.
///
/// Every command that changes an artifact acts on the workspace's own unless
/// `-p` names one of its projects, and is the same function `mind` runs for
/// that project: the two levels differ in what they hold, never in the rules.
fn run_weaver(
    command: &WeaverCommand,
    directory: &Path,
    lookup: &Lookup,
    watch: &Watch,
) -> Result<Outcome, Failure> {
    match command {
        // Typed from the directory the command works in, as `link`'s paths
        // are, rather than from where it was run.
        WeaverCommand::Init {
            path,
            skills,
            rules,
        } => {
            let root = match path {
                Some(path) => typed(directory, path)?,
                None => directory.to_path_buf(),
            };
            init_workspace(&root, skills.as_deref(), rules.as_deref(), lookup)
        }
        WeaverCommand::Activate { path, shell } => {
            activate_command(directory, path.as_deref(), *shell, lookup)
        }
        WeaverCommand::Which => which_command(directory, lookup),
        WeaverCommand::Workspaces { command, json } => match command {
            None => workspaces_command(directory, lookup, *json),
            Some(WorkspacesCommand::Forget { path }) => forget_command(directory, path, lookup),
        },

        WeaverCommand::Add {
            project,
            kind,
            name,
            description,
            template,
            server,
        } => {
            let workspace = workspace_here(directory, lookup)?;
            add_to(
                Some(&workspace),
                &workspace.holder(project.as_deref())?,
                *kind,
                (name, description),
                template.as_deref(),
                server.given().as_ref(),
            )
        }
        WeaverCommand::Edit {
            reference,
            project,
            editing,
        } => {
            let workspace = workspace_here(directory, lookup)?;
            edit_in(
                Some(&workspace),
                &workspace.holder(project.as_deref())?,
                reference,
                editing,
                directory,
            )
        }
        WeaverCommand::Rename {
            reference,
            to,
            project,
        } => {
            let workspace = workspace_here(directory, lookup)?;
            rename_in(
                Some(&workspace),
                &workspace.holder(project.as_deref())?,
                reference,
                to,
            )
        }
        // Without --yes it deletes nothing, so it is resolved as a read and
        // makes no workspace, the default one in home included: a dry run
        // that wrote a marker would not be one.
        WeaverCommand::Remove {
            reference,
            project,
            everywhere,
            yes,
        } => match (*everywhere, *yes) {
            (true, true) => remove_everywhere(
                &workspace_here(directory, lookup)?,
                reference,
                &lookup.agents,
            ),
            (true, false) => reading_workspace(directory, lookup, |workspace| {
                would_remove_everywhere(workspace, reference)
            }),
            (false, true) => {
                let workspace = workspace_here(directory, lookup)?;
                removed_from(&workspace, project.as_deref(), reference, true)
            }
            (false, false) => reading_workspace(directory, lookup, |workspace| {
                removed_from(workspace, project.as_deref(), reference, false)
            }),
        },
        WeaverCommand::Templates { project, json } => {
            let mut found = None;
            let text = reading_workspace(directory, lookup, |workspace| {
                templates_of(
                    Some(workspace),
                    &workspace.holder(project.as_deref())?,
                    &mut found,
                )
            })?;
            Ok(json_or_text(found.filter(|_| *json), text))
        }

        WeaverCommand::Link { projects } => link_projects(directory, projects, lookup),
        WeaverCommand::Unlink { projects } => unlink_projects(directory, projects, lookup),
        WeaverCommand::Relink { old, new } => relink_project(directory, old, new, lookup),

        WeaverCommand::Load { workspaces } => {
            load_command(directory, directory, workspaces, lookup)
        }
        WeaverCommand::Unload { workspaces } => {
            unload_command(directory, directory, workspaces, lookup)
        }

        // A screen, so it opens as one: on the default workspace before it
        // is made too, which its first change makes. Its folders are taken
        // from here as `scan` takes them.
        WeaverCommand::Minds { roots } => {
            tui::run_minds(directory, &targets(directory, roots)?, lookup)
        }

        // Each folder taken from here as `link` takes a path, and each path
        // found spelled as `mindweaver link` run here would take it, so a line of
        // the report is a line to type.
        WeaverCommand::Scan {
            roots,
            depth,
            suggest,
            json,
        } => {
            let request = scan::Request {
                roots: targets(directory, roots)?,
                depth: *depth,
                from: Some(typed(directory, Path::new(""))?),
                order: if *suggest {
                    scan::Order::Suggested
                } else {
                    scan::Order::Route
                },
            };
            reading_workspace(directory, lookup, |workspace| {
                Ok(scan_workspace(workspace, &request, *json))
            })
        }

        WeaverCommand::Gather(command) => run_gather(command, directory, lookup, watch),

        WeaverCommand::Install {
            projects,
            all_projects,
            name,
            from,
            force,
            yes,
        } => match (name, projects.is_empty() && !*all_projects) {
            // The screen reads until something on it is applied, so it is
            // opened the way a read is: on the default workspace before it
            // is made too, which manages nothing to install into.
            (None, true) => reading_workspace(directory, lookup, |workspace| {
                crate::install::run(workspace, lookup)
            }),
            (Some(name), false) => assigning(directory, lookup, *all_projects, |workspace| {
                let (asked, unread) = named_projects(workspace, projects, *all_projects)?;
                if asked.is_empty() {
                    return Ok(none_linked("installed", unread));
                }
                beside_all(
                    install_named(
                        workspace,
                        &asked,
                        name,
                        from.as_deref(),
                        directory,
                        &lookup.agents,
                        *force,
                        *yes,
                    ),
                    unread,
                )
            }),
            // Half of one: a skill with no project to put it in, or a
            // project with nothing named to put there.
            (Some(_), true) => Err(CliError::InstallWhere.into()),
            (None, false) => Err(CliError::InstallWhat.into()),
        },
        WeaverCommand::Uninstall {
            projects,
            all_projects,
            name,
            force,
            ..
        } => {
            if projects.is_empty() && !*all_projects {
                return Err(CliError::UninstallWhere.into());
            }
            assigning(directory, lookup, *all_projects, |workspace| {
                let (asked, unread) = named_projects(workspace, projects, *all_projects)?;
                if asked.is_empty() {
                    return Ok(none_linked("removed", unread));
                }
                let done = match name {
                    Some(name) => uninstall_named(workspace, &asked, name, &lookup.agents, *force),
                    None => uninstall_orphans(workspace, &asked, &lookup.agents, *force),
                };
                beside_all(done, unread)
            })
        }

        // Resolved as a read, on the default workspace before it is made
        // too, as `migrate` is: one made for the occasion links no project
        // to update. Each project named, or every one linked, each that
        // would not open said beside the rest, as `status` says one.
        WeaverCommand::Update {
            projects,
            all,
            names,
            force,
            yes,
        } => {
            if projects.is_empty() && !*all {
                return Err(CliError::UpdateWhere.into());
            }
            reading_workspace(directory, lookup, |workspace| {
                let (asked, warnings) = named_projects(workspace, projects, *all)?;
                update_projects(
                    workspace,
                    &asked,
                    names,
                    *force,
                    *yes,
                    &lookup.agents,
                    warnings,
                )
            })
        }

        // Resolved as a read, with --yes too, as `update` is: a default
        // workspace made for the occasion links no project to adopt from,
        // and `-p` would refuse it all the same.
        WeaverCommand::Adopt {
            project,
            reference,
            yes,
        } => reading_workspace(directory, lookup, |workspace| {
            adopt_from(
                workspace,
                &workspace.pick(project)?,
                reference,
                *yes,
                &lookup.agents,
            )
        }),

        // Resolved as a read, as an export is: it writes only a check-out
        // in the scratch folder, which it takes away, and a workspace
        // nothing has made has no library to send.
        WeaverCommand::Backup {
            url,
            branch,
            kinds,
            message,
            token_env,
        } => reading_workspace(directory, lookup, |workspace| {
            // A path typed is the folder's it was typed in, where `git`
            // would read it from wherever it runs.
            let url = if url::is_path(url) {
                typed(directory, Path::new(url))?
                    .to_string_lossy()
                    .into_owned()
            } else {
                url.clone()
            };
            backup_to(
                workspace,
                &Backing {
                    url: &url,
                    branch: branch.as_deref(),
                    kinds,
                    message: message.as_deref(),
                    token_env: token_env.as_deref(),
                },
            )
        }),

        // Resolved as a read: an export reads the workspace and writes only
        // the folder it is given, so a default workspace nothing has made
        // exports the empty library it would have, and is not made for it.
        WeaverCommand::Export { into, kinds, yes } => {
            reading_workspace(directory, lookup, |workspace| {
                export_into(workspace, &typed(directory, into)?, kinds, *yes)
            })
        }

        // Resolved as a read, on the default workspace before it is made
        // too: a workspace made for the occasion keeps its own in its
        // library from the start, and links no project to move.
        WeaverCommand::Migrate {
            library,
            names,
            project,
            all_projects,
            settings,
            yes,
        } => {
            let asked = Asked {
                agents: &settings.agents,
                share: settings.share(),
                yes: *yes,
            };
            if *names {
                return reading_workspace(directory, lookup, |workspace| {
                    migrate_names(workspace, *yes)
                });
            }
            match (library, project, all_projects) {
                (true, _, _) => reading_workspace(directory, lookup, |workspace| {
                    migrate_library(workspace, *yes)
                }),
                (_, Some(project), _) => reading_workspace(directory, lookup, |workspace| {
                    migrate_one(workspace, &workspace.pick(project)?, &asked, &lookup.agents)
                }),
                (_, _, true) => reading_workspace(directory, lookup, |workspace| {
                    migrate_all(workspace, &asked, &lookup.agents)
                }),
                (false, None, false) => Err(CliError::MigrateWhat.into()),
            }
        }

        // `-p` is the project's whichever side of `add` or `remove` it is
        // typed, which clap can take only as an option it does not require.
        WeaverCommand::Agents {
            project,
            change,
            named,
        } => {
            let project = project.as_deref().ok_or(CliError::AgentsWhere)?;
            let workspace = workspace_here(directory, lookup)?;
            let (change, yes) = agents_change(change.as_ref(), named);
            change_of(
                &workspace,
                &workspace.pick(project)?,
                &change,
                yes,
                &lookup.agents,
            )
        }
        WeaverCommand::Share { project, mode } => {
            let workspace = workspace_here(directory, lookup)?;
            change_of(
                &workspace,
                &workspace.pick(project)?,
                &Change::Share {
                    share: mode.share,
                    edit_tracked: mode.edit_tracked,
                },
                mode.yes,
                &lookup.agents,
            )
        }
        // `-p` is the project's on either side of `set` or `reset`, as for
        // `agents`.
        WeaverCommand::Targets { project, change } => {
            let project = project.as_deref().ok_or(CliError::TargetsWhere)?;
            let workspace = workspace_here(directory, lookup)?;
            let (change, yes) = targets_change(change);
            change_of(
                &workspace,
                &workspace.pick(project)?,
                &change,
                yes,
                &lookup.agents,
            )
        }
        WeaverCommand::Tui => tui::run(directory, lookup),
        #[cfg(feature = "ui")]
        WeaverCommand::Ui(options) => ui::ui_command(directory, options, lookup),

        // Under --json, what the snapshot holds of each holder listed, with
        // the warnings and the exit status of the listing.
        WeaverCommand::List {
            kind,
            project,
            json,
        } => {
            let kinds = wanted(*kind);
            reading_workspace(directory, lookup, |workspace| match project {
                Some(project) => {
                    let picked = workspace.pick(project)?;
                    let listing =
                        json.then(|| Listing::of_project(&picked, Some(workspace), &kinds));
                    let text = one_project(picked, kinds, |catalog, projects, level| {
                        Ok(list(catalog, projects, level))
                    })?;
                    Ok(json_or_text(listing, text))
                }
                None => {
                    let listing = json.then(|| Listing::of_workspace(workspace, &kinds));
                    let (projects, warnings) = managed(workspace);
                    let text = everywhere(
                        workspace,
                        warnings,
                        &projects,
                        kinds,
                        |catalog, projects, level| {
                            Ok(list_workspace(catalog, projects, workspace, level))
                        },
                    )?;
                    Ok(json_or_text(listing, text))
                }
            })
        }
        WeaverCommand::Show {
            reference,
            project,
            json,
        } => {
            let reference = Reference::parse(reference);
            let kinds = wanted(reference.kind());
            let mut found = None;
            let read = |catalog: &Catalog, _: &[MindProject], level: Level| {
                show(catalog, &reference, level, &mut found)
            };
            let text = reading_workspace(directory, lookup, |workspace| match project {
                Some(project) => one_project(workspace.pick(project)?, kinds, read),
                None => {
                    let (projects, warnings) = managed(workspace);
                    everywhere(workspace, warnings, &projects, kinds, read)
                }
            })?;
            Ok(json_or_text(found.filter(|_| *json), text))
        }
        // Read as `list` reads, on the default workspace before it is made
        // too, and from a ledger read without being made: a workspace that
        // has installed nothing has none, and a report is no reason to
        // create one.
        WeaverCommand::Status { project, json } => {
            reading_workspace(directory, lookup, |workspace| match project {
                Some(project) => Ok(status_of_project(
                    workspace,
                    workspace.pick(project)?,
                    *json,
                )),
                None => Ok(status_of_workspace(workspace, *json)),
            })
        }
        // Read as `status` reads, and told the agent homes and the names of
        // the variables set by the lookup, as core never looks them up.
        WeaverCommand::Doctor { project, json } => {
            reading_workspace(directory, lookup, |workspace| match project {
                Some(project) => Ok(doctor_of_project(
                    workspace,
                    &workspace.pick(project)?,
                    lookup,
                    *json,
                )),
                None => Ok(doctor_of_workspace(workspace, lookup, *json)),
            })
        }
        // Laid out as JSON once the read is done, the hint about a workspace
        // not made yet among what it said, which stays on stderr.
        WeaverCommand::Validate {
            target,
            project,
            json,
        } => {
            let selector = Selector::parse(target.as_deref());
            let kinds = selector.kinds();
            let mut found = None;
            let read = |catalog: &Catalog, _: &[MindProject], level: Level| {
                validate(catalog, &selector, level, &mut found)
            };
            let text = reading_workspace(directory, lookup, |workspace| match project {
                Some(project) => one_project(workspace.pick(project)?, kinds, read),
                None => {
                    let (projects, warnings) = managed(workspace);
                    everywhere_and_the_shelf(workspace, warnings, &projects, kinds, read)
                }
            })?;
            Ok(json_or_text(found.filter(|_| *json), text))
        }
    }
}

// ---------------------------------------------------------------------------
// Locating what a command acts on
// ---------------------------------------------------------------------------

/// The directory this invocation works in.
fn working_directory(path: Option<&Path>) -> Result<PathBuf, CliError> {
    match path {
        Some(dir) => Ok(dir.to_path_buf()),
        None => std::env::current_dir().map_err(CliError::CurrentDirectory),
    }
}

/// A path a user typed, made absolute against the directory the command
/// works in.
fn typed(directory: &Path, path: &Path) -> Result<PathBuf, CliError> {
    let joined = directory.join(path);
    let absolute = std::path::absolute(&joined).map_err(|source| CliError::Resolve {
        path: joined.clone(),
        source,
    })?;
    Ok(paths::normalize(&absolute))
}

/// The mind project the caller is standing in.
fn project_here(directory: &Path) -> Result<MindProject, CliError> {
    MindProject::locate(directory)?.ok_or_else(|| CliError::NotInProject(directory.to_path_buf()))
}

/// The weaver workspace a command that writes acts on: the one `lookup`
/// resolves from `directory`, which is the default one in home, made there
/// the first time a command needs it, when nothing chooses another. A
/// command that only reads asks [`workspace_to_read`] instead.
fn workspace_here(directory: &Path, lookup: &Lookup) -> Result<WeaverWorkspace, CliError> {
    resolve(&lookup.inputs(directory, Intent::Write))?
        .map(|resolved| resolved.workspace)
        .ok_or_else(|| CliError::NotInWorkspace(directory.to_path_buf()))
}

/// The weaver workspace a command that only reads looks at: the one `lookup`
/// resolves from `directory` (the default one in home made, or as it will
/// be once something makes it).
///
/// Never made for the occasion. A read is no reason to write a marker into
/// somebody's home, and the workspace that would be made there answers every
/// read as the made one would a moment later.
fn workspace_to_read(directory: &Path, lookup: &Lookup) -> Result<WeaverWorkspace, CliError> {
    resolve(&lookup.inputs(directory, Intent::Read))?
        .map(|resolved| resolved.workspace)
        .ok_or_else(|| CliError::NotInWorkspace(directory.to_path_buf()))
}

/// Run a read on the workspace [`workspace_to_read`] finds, with one line
/// more when that has not been made: where it will be.
///
/// A hint rather than a warning, and so no reason to fail: nothing is wrong
/// with a workspace nobody has needed yet, and the read says what it would
/// say of an empty one.
fn reading_workspace(
    directory: &Path,
    lookup: &Lookup,
    read: impl FnOnce(&WeaverWorkspace) -> Result<Outcome, Failure>,
) -> Result<Outcome, Failure> {
    reading(&workspace_to_read(directory, lookup)?, read)
}

/// Run a read on a workspace found some other way, with the same one line
/// more when it has not been made.
fn reading(
    workspace: &WeaverWorkspace,
    read: impl FnOnce(&WeaverWorkspace) -> Result<Outcome, Failure>,
) -> Result<Outcome, Failure> {
    let mut result = read(workspace);
    if !workspace.is_made() {
        let hint = Warning::from(unmade_hint(workspace));
        match &mut result {
            Ok(outcome) => outcome.stderr.push(hint),
            Err(failure) => failure.warnings.push(hint),
        }
    }
    result
}

/// What a read says about a default workspace nothing has made yet: that
/// it is not there, and where the first command that writes will put it.
fn unmade_hint(workspace: &WeaverWorkspace) -> String {
    format!(
        "hint: no workspace here or above, and the default one is not made yet; \
         the first command that writes to it, such as `mindweaver link <path>`, makes it in {}",
        workspace.weaver_dir().display()
    )
}

/// Every path a command was given, resolved and each named once: the same
/// project typed twice is one project.
fn targets(directory: &Path, paths: &[PathBuf]) -> Result<Vec<PathBuf>, CliError> {
    let mut resolved: Vec<PathBuf> = Vec::new();
    for path in paths {
        let target = typed(directory, path)?;
        if !resolved.contains(&target) {
            resolved.push(target);
        }
    }
    Ok(resolved)
}

/// A command given several things refuses them all when any is wrong, and
/// says why about each one rather than only the first.
fn all_or_nothing(mut refused: Vec<CliError>) -> Option<Failure> {
    if refused.is_empty() {
        return None;
    }
    let first = refused.remove(0);
    Some(Failure {
        error: Box::new(first),
        warnings: refused
            .iter()
            .map(|error| Warning::coded(error.code(), format!("error: {error}")))
            .collect(),
    })
}

/// The projects a workspace manages, and a warning for each entry that
/// would not open.
///
/// Only the registered ones. A workspace manages what it was told to manage,
/// so a project that happens to sit inside it is not in scope until it is
/// linked; otherwise `mindweaver list` would answer a different question
/// depending on which directory it was run from.
fn managed(workspace: &WeaverWorkspace) -> (Vec<MindProject>, Vec<Warning>) {
    let (projects, failures) = workspace.projects();
    let warnings = failures
        .iter()
        .map(|failure| Warning::coded(failure.code(), format!("warning: {failure}")))
        .collect();
    (projects, warnings)
}

/// The workspace a project's own commands register it with: the one
/// `lookup` resolves from the project, whether or not it manages it yet;
/// it is where `mind link` adds it, and the default one in home when nothing
/// chooses another and there is none up the tree.
fn workspace_for(project: &MindProject, lookup: &Lookup) -> Result<WeaverWorkspace, CliError> {
    resolve(&lookup.inputs(project.root(), Intent::Write))?
        .map(|resolved| resolved.workspace)
        .ok_or_else(|| CliError::NoWorkspaceAbove(project.root().to_path_buf()))
}

/// `mindweaver remove` without `--everywhere`: of one of the workspace's own,
/// with what it leaves orphaned said beside it, or of `project`'s, a
/// project the workspace links, whose own nothing installed anywhere.
fn removed_from(
    workspace: &WeaverWorkspace,
    project: Option<&str>,
    reference: &str,
    yes: bool,
) -> Result<Outcome, Failure> {
    match project {
        None => remove_own(workspace, reference, yes),
        Some(_) => remove_from(Some(workspace), &workspace.holder(project)?, reference, yes),
    }
}

/// The projects a command names: each one `-p` names, by name or path, or,
/// with `all`, every one the workspace links, with a warning for each entry
/// that would not open ([`managed`]). One `-p` names that the workspace does
/// not link refuses the command, before anything is done for the others: a
/// typo in one name is not a reason to change the rest without it.
fn named_projects(
    workspace: &WeaverWorkspace,
    projects: &[String],
    all: bool,
) -> Result<(Vec<MindProject>, Vec<Warning>), CliError> {
    if all {
        return Ok(managed(workspace));
    }
    let asked = projects
        .iter()
        .map(|project| workspace.pick(project))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((asked, Vec::new()))
}

/// Run an install or uninstall of one artifact, `assign`, on the workspace
/// it is for: as a read when it is given every project (`all`), as `mindweaver
/// update --all` is, and as a write when it names them.
///
/// Every project of a default workspace nothing has made yet is none, so
/// making it for the occasion would leave a marker in somebody's home for a
/// command that then does nothing; read, it says so with where the workspace
/// will be. A project named by `-p` is still looked for in the workspace a
/// write makes, as it was for one project alone before several could be
/// named, which leaves that one refusing after making it (an open question
/// in the architecture doc).
fn assigning(
    directory: &Path,
    lookup: &Lookup,
    all: bool,
    assign: impl FnOnce(&WeaverWorkspace) -> Result<Outcome, Failure>,
) -> Result<Outcome, Failure> {
    if all {
        reading_workspace(directory, lookup, assign)
    } else {
        assign(&workspace_here(directory, lookup)?)
    }
}

/// What a command given every project the workspace links says when it
/// links none it could open: that nothing was `done`, and why, with each
/// entry that would not open, `unread`, beside it, as `mindweaver update --all`
/// says it. Not a failure while every entry opens, since nothing asked of
/// it went wrong.
fn none_linked(done: &str, unread: Vec<Warning>) -> Outcome {
    let why = if unread.is_empty() {
        "no project is linked"
    } else {
        "no project linked could be read"
    };
    Outcome {
        stdout: format!("nothing {done}: {why}\n"),
        ok: unread.is_empty(),
        stderr: unread,
    }
}

/// `result`, with `unread`, the entries a command given every project could
/// not open, said after what it says: beside an outcome, failing it, and
/// before the error of a failure, among its warnings.
fn beside_all(result: Result<Outcome, Failure>, unread: Vec<Warning>) -> Result<Outcome, Failure> {
    match result {
        Ok(mut outcome) => {
            outcome.ok &= unread.is_empty();
            outcome.stderr.extend(unread);
            Ok(outcome)
        }
        Err(mut failure) => {
            failure.warnings.extend(unread);
            Err(failure)
        }
    }
}

/// The workspace that manages a project, as core's
/// [`managing`](resolve::managing) resolves it: the one whose ledger records
/// what is installed there.
fn managing(project: &MindProject, lookup: &Lookup) -> Result<WeaverWorkspace, CliError> {
    resolve::managing(&lookup.inputs(project.root(), Intent::Write), project)?
        .ok_or_else(|| CliError::NoWorkspaceAbove(project.root().to_path_buf()))
}

/// [`managing`], resolved as a read: the default workspace in home is never
/// made for the occasion. For `mind update`, which changes only what the
/// workspace records, so one made now would link no project and refuse it
/// all the same, having written a marker into somebody's home on the way,
/// even for a dry run.
fn managing_unmade(project: &MindProject, lookup: &Lookup) -> Result<WeaverWorkspace, CliError> {
    resolve::managing(&lookup.inputs(project.root(), Intent::Read), project)?
        .ok_or_else(|| CliError::NoWorkspaceAbove(project.root().to_path_buf()))
}

/// The workspace a `mind` command consults about the project it changes, as
/// core's [`consulted`](resolve::consulted) resolves it: the one `mind link`
/// and `mind install` find, if it has been made.
///
/// The TUI asks it too, for the templates its add form offers a project, so
/// they are what the `mind` lines it runs will find.
pub(crate) fn consulted(
    project: &MindProject,
    lookup: &Lookup,
) -> Result<Option<WeaverWorkspace>, CliError> {
    Ok(
        resolve::consulted(&lookup.inputs(project.root(), Intent::Read), project)?
            .map(|resolved| resolved.workspace),
    )
}

/// Make a change to `project` with the workspace [`consulted`] about it.
///
/// One found that cannot be read is a warning after the change rather than a
/// refusal of it: the change is the project's, and the workspace was only to
/// be told. Not so for `mind add --template`, where the workspace lends what
/// the change is made from, which is why that asks [`consulted`] itself. Nor
/// for one `-W` or the activation names that is no workspace, which refuses
/// the change as it refuses every other command: whoever chose it meant that
/// one to be told, and a change made without it would leave its record
/// behind once the name is put right.
fn consulting(
    project: &MindProject,
    lookup: &Lookup,
    change: impl FnOnce(Option<&WeaverWorkspace>) -> Result<Outcome, Failure>,
) -> Result<Outcome, Failure> {
    let (workspace, unread) = match consulted(project, lookup) {
        Ok(workspace) => (workspace, None),
        Err(error @ CliError::Chosen { .. }) => return Err(error.into()),
        Err(error) => (
            None,
            Some(Warning::coded(error.code(), format!("warning: {error}"))),
        ),
    };
    let mut result = change(workspace.as_ref());
    if let Some(warning) = unread {
        match &mut result {
            Ok(outcome) => {
                outcome.stderr.push(warning);
                outcome.ok = false;
            }
            Err(failure) => failure.warnings.push(warning),
        }
    }
    result
}
