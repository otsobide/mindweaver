//! `mindweaver install`: the screen for putting skills and rules into projects.
//!
//! The screen only marks boxes. Everything marked is carried out when you
//! say so, as the `mindweaver install` and `mindweaver uninstall` lines it amounts
//! to ([`Screen::task`]), each parsed and run as typed, the way the TUI runs
//! every line; and what they said is printed afterwards as the TUI's
//! transcript prints it, each line and then what it said. So the report
//! shows exactly what ran, and can be run again without the screen.

pub mod state;
pub mod ui;

use std::io;
use std::time::Duration;

use crate::progress::Watch;
use mindweaver_core::install::{self, Edits};
use mindweaver_core::ledger::Ledger;
use mindweaver_core::{AgentHomes, Kind, MindProject, WeaverWorkspace};
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::commands::install::{entries_said, named};
use crate::commands::workspace::load_warnings;
use crate::tui::state::{transcript, Done};
use crate::{tui, CliError, Failure, Lookup, Outcome, Warning};
use state::{Screen, Step, Target};

/// Build the screen for a workspace, run it, and carry out what it asked for.
///
/// Built from the ledger as it is ([`open`]), without making one or scrubbing
/// it: opening the screen and leaving it changes nothing, so it is a read until
/// something is applied. Applying runs lines ([`apply`]), the first of which
/// opens the ledger to write, which makes it and scrubs it, as that command
/// does from the shell; each with `lookup`, so they install nothing into the
/// agents' own folders the command line was given, and use the workspace it
/// chose.
pub fn run(workspace: &WeaverWorkspace, lookup: &Lookup) -> Result<Outcome, Failure> {
    let (projects, warnings) = registered(workspace);
    let screen = open(workspace, projects, &lookup.agents)?;

    if screen.targets.is_empty() {
        let mut stderr = warnings;
        stderr.extend(load_warnings(workspace));
        return Ok(Outcome {
            stdout: format!(
                "{} manages no projects yet\n  link one with `mindweaver link <path>`\n",
                workspace.name()
            ),
            stderr,
            ok: true,
        });
    }

    // `try_init` rather than `init`, which panics: this command is the one
    // thing here that needs a terminal, and being run without one (in CI,
    // with the output redirected) deserves a sentence rather than a panic.
    let mut terminal = ratatui::try_init().map_err(|source| CliError::NoTerminal {
        command: "mindweaver install",
        source,
    })?;
    let outcome = drive(&mut terminal, screen);
    ratatui::restore();

    let (screen, step) = outcome.map_err(|source| CliError::Screen { source })?;
    Ok(finish(workspace, &screen, step, lookup, warnings))
}

/// What `mindweaver install` says once its screen is left with `step`: what
/// applying ran ([`apply`]), or that nothing was, after `warnings`, the
/// projects that would not open, and what the loaded workspaces could not
/// offer, on stderr, failing it.
///
/// What the loaded workspaces could not offer is said by every install line
/// itself, as `mindweaver install -p` says it typed by hand, so it is the
/// screen's to say only when no install line ran: said here as well, it
/// would be said once more than the lines that said it. An uninstall line
/// does not say it, since what it takes out is read off the ledger rather
/// than offered. What the lines said stays under each, on stdout, as the
/// TUI prints a session, warnings and errors included: on stderr, a warning
/// would be cut off from the line it is about.
pub fn finish(
    workspace: &WeaverWorkspace,
    screen: &Screen,
    step: Step,
    lookup: &Lookup,
    mut warnings: Vec<Warning>,
) -> Outcome {
    let applied = step == Step::Apply;
    let mut outcome = if applied {
        apply(workspace, screen, lookup)
    } else {
        Outcome {
            stdout: String::from("nothing applied\n"),
            stderr: Vec::new(),
            ok: true,
        }
    };
    let (installs, _) = screen.counts();
    if !applied || installs == 0 {
        warnings.extend(load_warnings(workspace));
    }
    outcome.ok = outcome.ok && warnings.is_empty();
    outcome.stderr.splice(0..0, warnings);
    outcome
}

/// Build the screen from the ledger as it is, as a read: without making the
/// ledger, or scrubbing it of what an older build stored with a credential.
///
/// The screen `mindweaver install` opens and the one inside the TUI both open here
/// and apply as the lines [`Screen::task`] gives, so they cannot differ about
/// either.
///
/// `homes` are the agents' own folders the command line decided, which what
/// an install of an MCP server would give each agent is read against, as
/// the line that installs it reads it.
pub fn open(
    workspace: &WeaverWorkspace,
    projects: Vec<MindProject>,
    homes: &AgentHomes,
) -> Result<Screen, CliError> {
    let read = workspace.ledger_for_reading()?;
    Ok(build(workspace, &read, projects, homes)?)
}

/// Carry out what `screen` marked: each line it amounts to ([`Screen::task`])
/// parsed and run from the workspace's root with `lookup`, as the TUI runs
/// one ([`tui::execute`]), and what each said, as the TUI's transcript says
/// it; failing when any line did not go through.
///
/// Every line is run, whatever came of the ones before, as the TUI runs the
/// same lines ([`crate::task::Task::chained`]): each is boxes marked
/// one by one. Its own ledger is opened to write by each line, scrubbed
/// first, so a source a scrub makes one with another is offered from where
/// it was by the time the line asks for it, by the origin the screen showed,
/// which a scrub leaves as it was; and what only the source the scrub took
/// out had gathered is refused as not offered, said where its line is.
pub fn apply(workspace: &WeaverWorkspace, screen: &Screen, lookup: &Lookup) -> Outcome {
    let dones: Vec<Done> = screen
        .task()
        .commands()
        .iter()
        .map(|words| tui::execute(words, workspace.root(), lookup, &Watch::new()))
        .collect();
    Outcome {
        stdout: transcript(&dones),
        stderr: Vec::new(),
        ok: dones.iter().all(|done| done.ok),
    }
}

/// The projects a workspace manages, and what could not be opened.
pub fn registered(workspace: &WeaverWorkspace) -> (Vec<MindProject>, Vec<Warning>) {
    let (projects, failures) = workspace.projects();
    let warnings = failures
        .iter()
        .map(|failure| Warning::coded(failure.code(), format!("warning: {failure}")))
        .collect();
    (projects, warnings)
}

/// Ask core how everything of every kind that can be installed stands with
/// each project, and, for each offer whose install waits for a yes, what it
/// would give each agent there ([`install::mcp::preview`]), laid out as the
/// line installing it says it ([`previewed`]).
///
/// Public because it and [`apply`] are the two ends of this feature that do
/// not need a terminal: a test builds a screen, presses the keys a person
/// would, and applies the result.
pub fn build(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    projects: Vec<MindProject>,
    homes: &AgentHomes,
) -> Result<Screen, install::InstallError> {
    let mut targets = Vec::new();
    for project in projects {
        let mut candidates = Vec::new();
        for kind in Kind::ALL {
            candidates.extend(install::survey(workspace, ledger, &project, kind)?);
        }
        // Which offer of each name the project holds, as core says: one box
        // a name, however many places offer it.
        let held = install::holding(workspace, ledger, &project, &candidates)?;
        let picked_by = workspace.naming(&project);
        let mut target = Target::new(project, picked_by, candidates);
        for (row, held) in target.rows.iter_mut().zip(held) {
            row.was = held;
            row.checked = held;
            if row.candidate.asks_first() {
                row.preview = Some(foreseen(
                    workspace,
                    ledger,
                    &target.project,
                    &row.candidate,
                    homes,
                ));
            }
        }
        targets.push(target);
    }
    Ok(Screen::new(targets))
}

/// What installing `candidate`, which waits for a yes, into `project` would
/// give each agent there, as a confirmation shows it before the yes: core's
/// preview of it ([`install::mcp::preview`]), laid out as the line
/// installing it says it ([`previewed`]). One for the install screen and
/// the panel's backend, so the two confirm a server in the same words.
pub(crate) fn foreseen(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidate: &install::Candidate,
    homes: &AgentHomes,
) -> Vec<String> {
    previewed(install::mcp::preview(
        workspace,
        ledger,
        project,
        candidate,
        homes,
        Edits::Kept,
    ))
}

/// The line a confirmation names an install by, above what it would give
/// each agent ([`foreseen`]): `mcp github into api, from load:../team:`.
pub(crate) fn heading(kind: Kind, name: &str, project: &str, origin: &str) -> String {
    format!("{} into {project}, from {origin}:", named(kind, name))
}

/// What a preview of an install, `done`, says, a line each, as the line
/// installing it says it under its own ([`entries_said`]), each warning
/// after what each agent would run, or why it would be refused.
fn previewed(done: Result<install::Installation, install::InstallError>) -> Vec<String> {
    match done {
        Ok(done) => {
            let Some(entries) = &done.entries else {
                // Refused before any agent was asked: somebody's server of
                // the name, or an entry changed since it was written.
                return vec![format!("  refused: {}", refused(&done.installed))];
            };
            let (lines, warned) = entries_said(entries, None, false);
            let mut said: Vec<String> = lines.lines().map(str::to_owned).collect();
            said.extend(
                warned
                    .into_iter()
                    .map(|warning| format!("  {}", warning.text)),
            );
            said
        }
        Err(error) => vec![format!("  error: {error}")],
    }
}

/// Why an install wrote nothing, by its code, as the line says it.
fn refused(installed: &install::Installed) -> String {
    installed
        .refused()
        .map_or_else(String::new, |refused| refused.code().to_owned())
}

/// Draw, wait for a key, repeat.
///
/// The only part of this feature a test cannot drive, which is why it holds
/// nothing but the loop: what a key means is [`Screen::press`]'s answer and
/// what the screen looks like is [`ui::draw`]'s.
fn drive(terminal: &mut DefaultTerminal, mut screen: Screen) -> Result<(Screen, Step), io::Error> {
    loop {
        terminal.draw(|frame| ui::draw(&screen, frame))?;

        // A poll rather than a blocking read, so a resize repaints instead of
        // waiting for somebody to press something first.
        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        match event::read()? {
            // Windows reports both press and release; acting on each would
            // toggle every box twice.
            Event::Key(key) if key.kind == KeyEventKind::Press => match screen.press(key.code) {
                Step::Stay => {}
                step => return Ok((screen, step)),
            },
            _ => {}
        }
    }
}
