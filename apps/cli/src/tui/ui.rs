//! Drawing the TUI.
//!
//! Two columns (the projects a workspace manages, and what the one under the
//! cursor holds), with the keys underneath and, over them, whatever the mode
//! asks for: a form, a question, a page of output. Every form shows the
//! command it is about to run, as it is typed. The install screen, when it is
//! open, draws itself. Nothing here decides anything; [`super::state`] does.

use mindweaver_core::create;
use mindweaver_core::{Agent, Kind, MarkerFile, Reason, Share, State};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use super::minds::{self, Minds};
use super::state::{
    AddField, AddForm, App, BackupForm, Confirm, ExportForm, Focus, GatherForm, Input, Item,
    LinkForm, LoadForm, Member, Mode, Reader, RenameForm, Running, Scope, SettingsForm,
    TargetsForm, Task, Tone, View, Workspace, BACKUP_FIELDS, GATHER_FIELDS,
};
use crate::commands::project::worth_linking;
use crate::install::ui::{centred, pane, selected};
use crate::progress::said;
use crate::render::{plural, printable};
use crate::task::{followed, Followed, MindsChange};

/// How wide a form's labels are, so the values line up.
const LABEL: u16 = 13;

/// Draw the whole screen.
pub fn draw(app: &App, frame: &mut Frame) {
    let area = frame.area();
    if let Mode::Installing(screen) = &app.mode {
        crate::install::ui::draw(screen, frame);
    } else {
        match &app.view {
            View::Nowhere => nowhere(app, frame, area),
            View::Workspace(workspace) => home(app, workspace, frame, area),
        }
        match &app.mode {
            Mode::Adding(form) => adding(app, form, frame, area),
            Mode::Renaming(form) => renaming(app, form, frame, area),
            Mode::Linking(form) => linking(app, form, frame, area),
            Mode::Loading(form) => loading(app, form, frame, area),
            Mode::Gathering(form) => gathering(form, frame, area),
            Mode::Exporting(form) => exporting(form, frame, area),
            Mode::Backing(form) => backing(form, frame, area),
            Mode::Confirming(confirm) => confirming(confirm, frame, area),
            Mode::Reading(reader) => reading(reader, frame, area),
            Mode::Minds(screen) => minds_screen(app, screen, frame, area),
            Mode::Settings(form) => setting(app, form, frame, area),
            Mode::Targets(form) => targeting(app, form, frame, area),
            Mode::Browse | Mode::Installing(_) => {}
        }
    }
    if let Some(running) = &app.running {
        watching(running, frame, area);
    } else if let Some(busy) = &app.busy {
        working(busy, frame, area);
    }
}

// ---------------------------------------------------------------------------
// The two screens underneath everything
// ---------------------------------------------------------------------------

/// No workspace yet: say so, and what one keypress would do about it.
fn nowhere(app: &App, frame: &mut Frame, area: Rect) {
    const WIDTH: u16 = 78;

    let directory = printable(&app.directory.display().to_string());
    let (missing, what, command) = match app.opened {
        Scope::Workspace => (
            "No weaver workspace here",
            "A workspace keeps track of the mind projects you manage together.",
            "mindweaver init",
        ),
        Scope::Project => (
            "No mind project here",
            "A project keeps the skills and rules its agents read, beside its code.",
            "mind init",
        ),
    };
    // The directory is the one line of unknown length, so it gets a line of
    // its own and the box grows by however many rows it wraps onto.
    let room = usize::from(WIDTH.min(area.width).saturating_sub(2)).max(1);
    let wrapped = (directory.chars().count() + 2).div_ceil(room);
    let text = vec![
        Line::from(Span::styled(missing, bold())),
        Line::from(""),
        Line::from("Nothing at or above this directory is one:"),
        Line::from(Span::styled(format!("  {directory}"), dim())),
        Line::from(what),
        Line::from(""),
        Line::from(vec![
            Span::styled("enter  ", bold()),
            Span::raw("create one here"),
            Span::styled(format!("  runs `{command}`"), dim()),
        ]),
        Line::from(vec![Span::styled("q      ", bold()), Span::raw("quit")]),
    ];
    let height = u16::try_from(text.len() + wrapped + 1).unwrap_or(u16::MAX);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" mindweaver ");
    let box_ = centred(area, WIDTH, height);
    frame.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: false }),
        box_,
    );

    // Whatever the last attempt said, where the eye goes after the box.
    if let Some(message) = &app.message {
        let line = Rect {
            y: area.bottom().saturating_sub(1),
            height: 1,
            ..area
        };
        frame.render_widget(
            Paragraph::new(Span::styled(printable(&message.text), tone(message.tone))),
            line,
        );
    }
}

/// The workspace: a header, the two columns, and the keys.
fn home(app: &App, workspace: &Workspace, frame: &mut Frame, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(rows[1]);

    header(workspace, frame, rows[0]);
    projects(app, workspace, frame, columns[0]);
    holdings(app, workspace, frame, columns[1]);
    footer(app, frame, rows[2]);
}

fn header(workspace: &Workspace, frame: &mut Frame, area: Rect) {
    let shelf = match (workspace.scope, workspace.shelf) {
        (Scope::Project, _) => String::from("a project no workspace manages"),
        (Scope::Workspace, 0) => String::from("nothing on the shelf"),
        (Scope::Workspace, count) => format!("{count} on the shelf"),
    };
    let shelf = match workspace.loaded.len() {
        0 => shelf,
        count => format!("{shelf} · {count} loaded"),
    };
    // Why this workspace, said because where you are does not say it: one
    // named or activated wins over the one above you, and with neither and
    // nothing at or above here it is the one in your home, which until the
    // first change makes it is not there yet.
    let why = match (workspace.reason, workspace.made) {
        (Some(Reason::Explicit), _) => Some("named with -W"),
        (Some(Reason::Activated), _) => Some("activated"),
        (Some(Reason::Above), _) => Some("found above"),
        (Some(Reason::Home), false) => Some("default workspace, not made yet"),
        (Some(Reason::Home), true) => Some("default workspace"),
        (None, _) => None,
    };
    let shelf = match why {
        Some(why) => format!("{why} · {shelf}"),
        None => shelf,
    };
    let mut spans = vec![
        Span::styled(
            " mindweaver ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(printable(&workspace.name), bold()),
    ];
    if let Some(older) = older(workspace) {
        spans.push(Span::styled("  · ", dim()));
        spans.push(Span::styled(older, Style::default().fg(Color::Yellow)));
    }
    spans.extend([
        Span::styled(format!("  · {shelf}  · "), dim()),
        // Last, because it is the one thing that can be long, and what a
        // narrow terminal cuts off the end of should be the least needed.
        Span::styled(printable(&workspace.root.display().to_string()), dim()),
    ]);
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// What the header says of projects an older Mindweaver made, marked by a
/// `mind.toml` or a `settings.json`, with the command that moves them to a
/// `settings.toml`: until then a `mind.toml`'s installs reach only the
/// folder it names, neither one's settings can be changed, and nothing else
/// on screen tells them apart before a toggle refuses one. Each file named
/// once, in the order they came, oldest first. Early in the line, so a
/// narrow terminal keeps it. None when there are none.
fn older(workspace: &Workspace) -> Option<String> {
    let older: Vec<MarkerFile> = workspace
        .members
        .iter()
        .filter(|member| !member.own)
        .filter_map(|member| member.holdings.as_ref().ok()?.older)
        .collect();
    let files: Vec<&str> = [MarkerFile::MindToml, MarkerFile::SettingsJson]
        .into_iter()
        .filter(|file| older.contains(file))
        .map(MarkerFile::name)
        .collect();
    let files = files.join(" or ");
    match (workspace.scope, older.len()) {
        (_, 0) => None,
        (Scope::Project, _) => Some(format!("marked by {files}: `mind migrate`")),
        (Scope::Workspace, count) => Some(format!(
            "{} marked by {files}: `mindweaver migrate --all-projects`",
            plural(count, "project")
        )),
    }
}

/// The left column: every registered entry, with how many artifacts it holds
/// and how many of those want attention.
fn projects(app: &App, workspace: &Workspace, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Projects;
    let title = match workspace.scope {
        Scope::Workspace => format!("Projects ({})", workspace.projects()),
        Scope::Project => String::from("Project"),
    };

    if workspace.members.is_empty() {
        let mut text = vec![
            Line::from("none linked yet"),
            Line::from(""),
            Line::from(Span::styled("l links one", dim())),
        ];
        if !workspace.unlinked.is_empty() {
            text.push(Line::from(Span::styled(
                format!(
                    "{} found under {}",
                    workspace.unlinked.len(),
                    looked_under(workspace)
                ),
                dim(),
            )));
        }
        frame.render_widget(
            Paragraph::new(text)
                .block(pane(&title, focused))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = workspace.members.iter().map(project_row).collect();
    let mut state = ListState::default();
    state.select(Some(app.cursor));
    let list = List::new(items)
        .block(pane(&title, focused))
        .highlight_style(selected(focused))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, area, &mut state);
}

fn project_row(member: &Member) -> ListItem<'_> {
    let name = printable(&member.name);
    // The workspace's own row reads as something else than a project: it
    // holds what is shared with all of them.
    let name = if member.own {
        format!("◆ {name}")
    } else {
        name
    };
    let spans = match &member.holdings {
        // A stale entry is listed, not hidden: it is exactly the one somebody
        // needs to find in order to unlink it.
        Err(_) => vec![Span::styled(
            format!("! {name}"),
            Style::default().fg(Color::Red),
        )],
        Ok(holdings) => {
            let styled = if member.own {
                Span::styled(name, Style::default().fg(Color::Cyan))
            } else {
                Span::raw(name)
            };
            let mut spans = vec![
                styled,
                Span::styled(format!("  {}", holdings.items.len()), dim()),
            ];
            let problems = member.problems();
            if problems > 0 {
                spans.push(Span::styled(
                    format!("  ✗ {problems}"),
                    Style::default().fg(Color::Red),
                ));
            }
            // Installs some of its agents do not see: not a problem with an
            // artifact, so a mark of its own, which the strip beside says.
            if !holdings.unreached.is_empty() {
                spans.push(Span::styled("  ⚠", warn()));
            }
            // What doctor finds, counted, which a strip beside says too.
            if !holdings.found.is_empty() {
                spans.push(Span::styled(
                    format!("  ✚ {}", holdings.found.len()),
                    warn(),
                ));
            }
            spans
        }
    };
    ListItem::new(Line::from(spans))
}

/// The right column: what the project under the cursor holds.
fn holdings(app: &App, workspace: &Workspace, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Items;
    let Some(member) = app.member() else {
        let text = if workspace.unlinked.is_empty() {
            String::from("nothing linked; l links a project by its path")
        } else {
            format!(
                "nothing linked; l picks from what is under {}",
                looked_under(workspace)
            )
        };
        frame.render_widget(
            Paragraph::new(Span::styled(text, dim()))
                .block(pane("Artifacts", false))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    };

    let title = if member.own {
        format!("{} · the workspace's own", printable(&member.name))
    } else if member.name == member.entry || member.entry == "." {
        printable(&member.name)
    } else {
        format!("{} ({})", printable(&member.name), printable(&member.entry))
    };

    let holdings = match &member.holdings {
        Ok(holdings) => holdings,
        Err(error) => {
            let mut text = vec![
                Line::from(Span::styled(
                    "this project cannot be opened",
                    bold().fg(Color::Red),
                )),
                Line::from(""),
                Line::from(printable(error)),
                Line::from(""),
                Line::from(Span::styled(
                    "U unlinks it; nothing on disk is touched",
                    dim(),
                )),
            ];
            // Said where the scan found its project, by the id it holds: the
            // one thing better to do with a stale entry than drop it.
            if let Some(moved) = workspace.moved_to(member) {
                text.push(Line::from(Span::styled(
                    format!(
                        "it moved to {}: M relinks it there, keeping what was installed",
                        printable(&moved.route)
                    ),
                    dim(),
                )));
            }
            frame.render_widget(
                Paragraph::new(text)
                    .block(pane(&title, focused))
                    .wrap(Wrap { trim: true }),
                area,
            );
            return;
        }
    };

    // Unreadable files get a strip of their own, so they cannot be scrolled
    // out of sight by the artifacts that did read, and so do installs some
    // of the project's agents do not see.
    let strips: Vec<(&str, &[String])> = [
        (" Could not be read ", holdings.warnings.as_slice()),
        (" Not seen by every agent ", holdings.unreached.as_slice()),
        (" Found by doctor ", holdings.found.as_slice()),
    ]
    .into_iter()
    .filter(|(_, lines)| !lines.is_empty())
    .collect();
    // Wrapped here, and the strip as tall as what that gives, so the end of
    // a sentence, which says what to do, is not cut off.
    let room = usize::from(area.width.saturating_sub(2));
    let strips: Vec<(&str, Vec<String>)> = strips
        .into_iter()
        .map(|(title, said)| {
            let wrapped = said
                .iter()
                .flat_map(|line| wrap(&printable(line), room))
                .collect();
            (title, wrapped)
        })
        .collect();
    // At most half the pane, and at least a line between its borders while
    // the pane has room for one: a pane shorter than that gets what it has,
    // since a lower bound above the upper one panics.
    let most = area.height / 2;
    let mut constraints = vec![Constraint::Min(3)];
    for (_, lines) in &strips {
        let height = u16::try_from(lines.len() + 2)
            .unwrap_or(u16::MAX)
            .clamp(3.min(most), most);
        constraints.push(Constraint::Length(height));
    }
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    let list_area = parts[0];

    if holdings.items.is_empty() {
        let mut text = vec![Line::from("nothing here yet"), Line::from("")];
        for (kind, directory) in &holdings.directories {
            text.push(Line::from(Span::styled(
                format!("{} go in {}/", kind.plural(), printable(directory)),
                dim(),
            )));
        }
        text.push(Line::from(""));
        text.push(Line::from(Span::styled("a adds a skill or a rule", dim())));
        if member.own {
            text.push(Line::from(Span::styled(
                "what is here is shared: i installs a skill into any project",
                dim(),
            )));
        }
        frame.render_widget(Paragraph::new(text).block(pane(&title, focused)), list_area);
    } else {
        let width = holdings
            .items
            .iter()
            .map(|item| printable(&item.name).chars().count())
            .max()
            .unwrap_or(0)
            .min(40);
        // Inside the borders and the highlight symbol, less the indent the
        // problems are written at.
        let room =
            usize::from(list_area.width.saturating_sub(4)).saturating_sub(ISSUE_INDENT.len());
        let rows: Vec<ListItem> = holdings
            .items
            .iter()
            .map(|item| item_row(item, &holdings.agents, width, room))
            .collect();
        let mut state = ListState::default();
        state.select(Some(app.item));
        let list = List::new(rows)
            .block(pane(&title, focused))
            .highlight_style(selected(focused))
            .highlight_symbol("> ");
        frame.render_stateful_widget(list, list_area, &mut state);
    }

    for ((title, said), strip) in strips.iter().zip(parts.iter().skip(1)) {
        let lines: Vec<Line> = said
            .iter()
            .map(|line| Line::from(Span::styled(line.clone(), warn())))
            .collect();
        let block = Block::bordered().title(*title).border_style(warn());
        frame.render_widget(Paragraph::new(lines).block(block), *strip);
    }
}

/// Where the problems under an artifact start, so they read as belonging to
/// it rather than as rows of their own.
const ISSUE_INDENT: &str = "        ";

/// One artifact: its kind, its name, how it stands in its project and which
/// of `agents`, the project's, read it, what it is for, and, underneath, what
/// `validate` would say about it, right beside the thing it is about, wrapped
/// to `room` rather than cut off, because the end of a reason is usually the
/// part that says what to do.
fn item_row<'a>(item: &'a Item, agents: &[Agent], width: usize, room: usize) -> ListItem<'a> {
    let name = printable(&item.name);
    let padding = width.saturating_sub(name.chars().count());
    let red = Style::default().fg(Color::Red);
    // `!` for a file that does not read at all, `✗` for one that reads and
    // is wrong: two different things to go and fix.
    let (mark, summary) = match &item.broken {
        Some(_) => (
            Span::styled("! ", red),
            Span::styled("cannot be read; e opens it", red),
        ),
        None if !item.issues.is_empty() => (
            Span::styled("✗ ", red),
            Span::styled(printable(&item.summary), Style::default().fg(Color::Gray)),
        ),
        None => (
            Span::raw("  "),
            Span::styled(printable(&item.summary), Style::default().fg(Color::Gray)),
        ),
    };
    let mut spans = vec![
        mark,
        Span::styled(format!("{:<5} ", item.kind.slug()), dim()),
        Span::styled(format!("{name}{}", " ".repeat(padding)), bold()),
        Span::raw("  "),
    ];
    // What `status` says of it, before what it is for: a row of the
    // workspace's own, which is no project's, says nothing of the kind.
    if let Some(state) = item.state {
        let widest = State::ALL
            .iter()
            .map(|state| state.slug().len())
            .max()
            .unwrap_or(0);
        spans.push(Span::styled(
            format!("{:<widest$}", state.slug()),
            state_style(state),
        ));
        spans.push(Span::raw(" "));
        if !agents.is_empty() {
            spans.extend(badges(agents, &item.agents));
            spans.push(Span::raw("  "));
        }
    }
    spans.push(summary);
    let mut lines = vec![Line::from(spans)];
    for issue in item.broken.iter().chain(&item.issues) {
        for part in wrap(&printable(issue), room) {
            lines.push(Line::from(Span::styled(
                format!("{ISSUE_INDENT}{part}"),
                Style::default().fg(Color::Red),
            )));
        }
    }
    ListItem::new(lines)
}

/// What a state looks like: green for what Mindweaver installed whole and
/// current, yellow for one an update brings up to date or puts back, cyan
/// for one somebody edited, which nothing writes over unless forced, red for
/// one nothing offers any more, and magenta for somebody's, which no key of
/// Mindweaver's changes.
fn state_style(state: State) -> Style {
    match state {
        State::Current => Style::default().fg(Color::Green),
        State::Outdated | State::Missing => warn(),
        State::Edited => Style::default().fg(Color::Cyan),
        State::Orphaned => Style::default().fg(Color::Red),
        State::Foreign => Style::default().fg(Color::Magenta),
        State::Authored => Style::default(),
    }
}

/// A badge for each of `slots`, the agents a project names, in its place
/// whether or not the artifact reaches it: shown where it is among `reached`,
/// blank where it is not, so a column of rows reads down by agent.
fn badges(slots: &[Agent], reached: &[Agent]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (index, agent) in slots.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        let label = badge(*agent);
        spans.push(if reached.contains(agent) {
            Span::styled(label, bold().fg(Color::Cyan))
        } else {
            Span::raw(" ".repeat(label.chars().count()))
        });
    }
    spans
}

/// The two letters a row shows for an agent: its own initials rather than
/// its first, which Claude Code and Codex share.
pub(crate) const fn badge(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "CC",
        Agent::Codex => "CX",
        Agent::OpenCode => "OC",
    }
}

/// Break text into lines at most `room` columns wide, at spaces where there
/// are any.
///
/// Measured in the columns a terminal draws, not in characters: a line as
/// many characters long as the room, holding some drawn two columns wide,
/// would otherwise run past the border and be cut there.
fn wrap(text: &str, room: usize) -> Vec<String> {
    if room == 0 {
        return vec![text.to_owned()];
    }
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        let taken = columns(&line);
        if taken > 0 && taken + 1 + columns(word) > room {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
        // A word wider than the room is cut where the room ends.
        while columns(&line) > room {
            let rest = line.split_off(fitting(&line, room));
            lines.push(std::mem::replace(&mut line, rest));
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// How many columns a terminal draws `text` in.
fn columns(text: &str) -> usize {
    Span::raw(text).width()
}

/// Where `line` is cut for what comes before to fit in `room` columns: at
/// least one character in, so one wider than the room still moves on.
fn fitting(line: &str, room: usize) -> usize {
    let mut taken = 0;
    for (at, character) in line.char_indices() {
        taken += columns(character.encode_utf8(&mut [0; 4]));
        if taken > room {
            return if at == 0 { character.len_utf8() } else { at };
        }
    }
    line.len()
}

/// The keys, most needed first so a narrow terminal cuts the least useful,
/// and the last thing that happened.
fn footer(app: &App, frame: &mut Frame, area: Rect) {
    // Only the keys that do something here: a project on its own has no
    // registry, shelf or installs to offer keys for.
    let keys = match (app.focus, app.scope()) {
        (Focus::Items, Scope::Workspace) => {
            "q quit  ? keys  enter show  e edit  r rename  d delete  u update  A adopt  a add  ← back  v validate"
        }
        (Focus::Items, Scope::Project) => {
            "q quit  ? keys  enter show  e edit  r rename  d delete  a add  ← back  v validate"
        }
        (Focus::Projects, Scope::Workspace) => {
            "q quit  ? keys  → open  a add  u update  m minds  l link  U unlink  v validate  g gather  L load  s shelf  i install  t agents  T targets  w workspaces  x export  b backup"
        }
        (Focus::Projects, Scope::Project) => {
            "q quit  ? keys  → open  a add  v validate  l link it to the workspace above  L load  t agents  T targets  w workspaces"
        }
    };
    let second = match &app.message {
        Some(message) => Span::styled(printable(&message.text), tone(message.tone)),
        None => Span::styled(
            "every change is a command, printed when you quit; ? lists them",
            dim(),
        ),
    };
    let text = vec![Line::from(Span::styled(keys, dim())), Line::from(second)];
    frame.render_widget(Paragraph::new(text), area);
}

// ---------------------------------------------------------------------------
// What goes over them
// ---------------------------------------------------------------------------

fn adding(app: &App, form: &AddForm, frame: &mut Frame, area: Rect) {
    let project = app
        .workspace()
        .and_then(|workspace| workspace.members.get(form.member))
        .map_or_else(String::new, |member| printable(&member.name));
    let inner = popup(frame, area, &format!("Add to {project}"), 76, 10);
    let rows = lines(inner, 8);

    // The kinds the form steps through, as core says `add` writes them.
    let kinds: Vec<(String, bool)> = Kind::ALL
        .iter()
        .filter(|kind| create::scaffolds(**kind))
        .map(|kind| (kind.slug().to_owned(), *kind == form.kind))
        .collect();
    choices(frame, rows[0], "kind", &kinds, form.field == AddField::Kind);

    // No template is a choice like the others, and the first one.
    let offered = app.templates_for(form.member, form.kind);
    let mut templates = vec![(String::from("none"), form.template.is_none())];
    templates.extend(
        offered
            .iter()
            .enumerate()
            .map(|(at, choice)| (printable(&choice.name), form.template == Some(at))),
    );
    choices(
        frame,
        rows[1],
        "template",
        &templates,
        form.field == AddField::Template,
    );

    input(
        frame,
        rows[2],
        "name",
        &form.name,
        form.field == AddField::Name,
    );
    input(
        frame,
        rows[3],
        "description",
        &form.description,
        form.field == AddField::Description,
    );
    runs(frame, rows[5], app.add_task(form).as_ref());
    error(frame, rows[6], form.error.as_deref());
    keys(
        frame,
        rows[7],
        "enter create   tab next   ←→ choose kind and template   esc cancel",
    );
}

fn renaming(app: &App, form: &RenameForm, frame: &mut Frame, area: Rect) {
    let inner = popup(
        frame,
        area,
        &format!("Rename {} {}", form.kind, printable(&form.from)),
        76,
        7,
    );
    let rows = lines(inner, 5);

    input(frame, rows[0], "new name", &form.name, true);
    let what = if form.kind == Kind::Skill {
        "the folder and the `name` it declares move together"
    } else {
        "the file is filed again by its new route"
    };
    let hint = Line::from(vec![
        Span::raw(" ".repeat(usize::from(LABEL))),
        Span::styled(what, dim()),
    ]);
    frame.render_widget(Paragraph::new(hint), rows[1]);
    runs(frame, rows[2], app.rename_task(form).as_ref());
    error(frame, rows[3], form.error.as_deref());
    keys(frame, rows[4], "enter rename   esc cancel");
}

/// Where one project installs skills, typed, and what applying it would run.
fn targeting(app: &App, form: &TargetsForm, frame: &mut Frame, area: Rect) {
    let project = app
        .workspace()
        .and_then(|workspace| workspace.members.get(form.member))
        .map_or_else(String::new, |member| printable(&member.name));
    let inner = popup(
        frame,
        area,
        &format!("Where {project} installs {}s", form.kind),
        90,
        8,
    );
    let rows = lines(inner, 6);

    input(
        frame,
        rows[0],
        &format!("{}s go in", form.kind),
        &form.folders,
        true,
    );
    for (row, hint) in [
        "folders in the project, a space between each; none for where its agents read",
        "an agent sees a folder of the project's own only once it is pointed there",
    ]
    .into_iter()
    .enumerate()
    {
        let line = Line::from(vec![
            Span::raw(" ".repeat(usize::from(LABEL))),
            Span::styled(hint, dim()),
        ]);
        frame.render_widget(Paragraph::new(line), rows[row + 1]);
    }
    runs(frame, rows[3], app.targets_task(form).as_ref());
    error(frame, rows[4], form.error.as_deref());
    keys(frame, rows[5], "enter apply   esc cancel");
}

/// The agents and share toggles of one project, and what applying them
/// would run.
fn setting(app: &App, form: &SettingsForm, frame: &mut Frame, area: Rect) {
    let project = app
        .workspace()
        .and_then(|workspace| workspace.members.get(form.member))
        .map_or_else(String::new, |member| printable(&member.name));
    let fields = SettingsForm::FIELDS;
    let height = u16::try_from(fields).unwrap_or(5) + 6;
    let inner = popup(
        frame,
        area,
        &format!("Agents and sharing for {project}"),
        90,
        height,
    );
    let rows = lines(inner, fields + 4);

    for (at, (agent, ticked)) in form.ticked.iter().enumerate() {
        let line = Line::from(vec![
            label(agent.name(), form.field == at),
            tick(*ticked),
            Span::styled(format!("  {}", badge(*agent)), dim()),
        ]);
        frame.render_widget(Paragraph::new(line), rows[at]);
    }
    let share = SettingsForm::share_field();
    let modes: Vec<(String, bool)> = Share::ALL
        .iter()
        .map(|mode| (mode.name().to_owned(), *mode == form.share))
        .collect();
    choices(frame, rows[share], "share", &modes, form.field == share);
    let line = Line::from(vec![
        label("edit tracked", form.field == share + 1),
        tick(form.edit_tracked),
        Span::styled(
            "  a file git tracks may be edited to reach an agent, when local",
            dim(),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), rows[share + 1]);

    runs(frame, rows[fields + 1], app.settings_task(form).as_ref());
    error(frame, rows[fields + 2], form.error.as_deref());
    keys(
        frame,
        rows[fields + 3],
        "enter apply   ↑↓ move   space tick   ←→ share   esc cancel",
    );
}

/// A box, ticked or not.
fn tick(ticked: bool) -> Span<'static> {
    if ticked {
        Span::styled("[x]", bold().fg(Color::Cyan))
    } else {
        Span::styled("[ ]", dim())
    }
}

/// A labelled line of choices, the chosen one bracketed.
fn choices(frame: &mut Frame, area: Rect, name: &str, offered: &[(String, bool)], focused: bool) {
    let mut line = vec![label(name, focused)];
    for (text, chosen) in offered {
        line.push(if *chosen {
            Span::styled(format!("[{text}] "), bold().fg(Color::Cyan))
        } else {
            Span::styled(format!(" {text}  "), dim())
        });
    }
    frame.render_widget(Paragraph::new(Line::from(line)), area);
}

fn linking(app: &App, form: &LinkForm, frame: &mut Frame, area: Rect) {
    let Some(workspace) = app.workspace() else {
        return;
    };
    let shown = workspace.unlinked.len().clamp(1, 10);
    let height = u16::try_from(shown).unwrap_or(10) + 8;
    let inner = popup(
        frame,
        area,
        &format!("Link a project to {}", printable(&workspace.name)),
        78,
        height,
    );
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(u16::try_from(shown).unwrap_or(10)),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    input(frame, rows[0], "path", &form.path, true);

    if workspace.unlinked.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!(
                    "nothing unlinked found under {}; type a path",
                    looked_under(workspace)
                ),
                dim(),
            )),
            rows[2],
        );
    } else {
        // The pick only counts while the path is empty, so it dims as soon as
        // something is typed.
        let picking = form.path.text().trim().is_empty();
        let width = workspace
            .unlinked
            .iter()
            .map(|unlinked| printable(&unlinked.route).chars().count())
            .max()
            .unwrap_or(0);
        let items: Vec<ListItem> = workspace
            .unlinked
            .iter()
            .map(|unlinked| {
                let route = printable(&unlinked.route);
                let padding = " ".repeat(width.saturating_sub(route.chars().count()));
                // Why, as the minds screen and `mindweaver scan --suggest` say
                // it, since the list is in their order.
                let why = worth_linking(
                    &unlinked.route,
                    unlinked.moved.as_deref(),
                    unlinked.reason,
                    &unlinked.agent_files,
                );
                let what = if unlinked.mind {
                    Span::styled(format!("  {why}"), dim())
                } else {
                    Span::styled(format!("  {}", found_repository(&why)), warn())
                };
                ListItem::new(Line::from(vec![
                    Span::raw(format!("{route}{padding}")),
                    what,
                ]))
            })
            .collect();
        let mut state = ListState::default();
        state.select(Some(form.pick));
        let list = List::new(items)
            .highlight_style(selected(picking))
            .highlight_symbol("> ");
        frame.render_stateful_widget(list, rows[2], &mut state);
    }

    runs(frame, rows[4], app.link_task(form).as_ref());
    error(frame, rows[5], form.error.as_deref());
    keys(
        frame,
        rows[6],
        "enter link   ↑↓ pick   type a path relative to the workspace   esc cancel",
    );
}

/// The load form: a path to type, over the workspaces already loaded, each
/// with what it offers, or why it cannot be read, and picked to unload.
fn loading(app: &App, form: &LoadForm, frame: &mut Frame, area: Rect) {
    let Some(workspace) = app.workspace() else {
        return;
    };
    let on_its_own = workspace.scope == Scope::Project;
    let shown = workspace.loaded.len().clamp(1, 10);
    let height = u16::try_from(shown).unwrap_or(10) + 8;
    let title = if on_its_own {
        String::from("Load a workspace into the one above this project")
    } else {
        format!("Load a workspace into {}", printable(&workspace.name))
    };
    let inner = popup(frame, area, &title, 78, height);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(u16::try_from(shown).unwrap_or(10)),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    input(frame, rows[0], "path", &form.path, true);

    if workspace.loaded.is_empty() {
        let hint = if on_its_own {
            "type a workspace's path, relative to this project (nothing loaded yet)"
        } else {
            "type a workspace's path, relative to this one (nothing loaded yet)"
        };
        frame.render_widget(Paragraph::new(Span::styled(hint, dim())), rows[2]);
    } else {
        // As in the link form, the pick only counts while nothing is typed.
        let picking = form.path.text().trim().is_empty();
        let width = workspace
            .loaded
            .iter()
            .map(|source| printable(&source.entry).chars().count())
            .max()
            .unwrap_or(0);
        let items: Vec<ListItem> = workspace
            .loaded
            .iter()
            .map(|source| {
                let entry = printable(&source.entry);
                let padding = " ".repeat(width.saturating_sub(entry.chars().count()));
                let what = match &source.offers {
                    Ok(0) => Span::styled(
                        format!("  {} · nothing to install yet", printable(&source.name)),
                        dim(),
                    ),
                    Ok(count) => Span::styled(
                        format!("  {} · {count} to install", printable(&source.name)),
                        dim(),
                    ),
                    Err(_) => Span::styled(
                        "  cannot be opened; enter unloads it",
                        Style::default().fg(Color::Red),
                    ),
                };
                ListItem::new(Line::from(vec![
                    Span::raw(format!("{entry}{padding}")),
                    what,
                ]))
            })
            .collect();
        let mut state = ListState::default();
        state.select(Some(form.pick));
        let list = List::new(items)
            .highlight_style(selected(picking))
            .highlight_symbol("> ");
        frame.render_stateful_widget(list, rows[2], &mut state);
    }

    runs(frame, rows[4], app.load_task(form).as_ref());
    error(frame, rows[5], form.error.as_deref());
    keys(
        frame,
        rows[6],
        "enter load, or unload the one picked   ↑↓ pick   esc cancel",
    );
}

fn gathering(form: &GatherForm, frame: &mut Frame, area: Rect) {
    let inner = popup(frame, area, "Gather skills from a git repository", 78, 9);
    let rows = lines(inner, 7);

    for (index, name) in GATHER_FIELDS.iter().enumerate() {
        input(
            frame,
            rows[index],
            name,
            form.input(index),
            form.field == index,
        );
    }
    // What an empty `folder` and `ref` mean, under them, rather than in a
    // key line too long to fit.
    let hint = Line::from(vec![
        Span::raw(" ".repeat(usize::from(LABEL))),
        Span::styled(
            "empty folder: all of it; empty ref: its default branch",
            dim(),
        ),
    ]);
    frame.render_widget(Paragraph::new(hint), rows[3]);
    runs(frame, rows[4], form.task().as_ref());
    error(frame, rows[5], form.error.as_deref());
    keys(frame, rows[6], "enter gather   tab next   esc cancel");
}

/// The export form: the folder, a tick for each kind, the yes that lets it
/// change or remove what an earlier export wrote, and what it would run.
fn exporting(form: &ExportForm, frame: &mut Frame, area: Rect) {
    let count = ExportForm::lines();
    let height = u16::try_from(count).unwrap_or(5) + 6;
    let inner = popup(frame, area, "Export the library into a folder", 84, height);
    let rows = lines(inner, count + 4);

    input(frame, rows[0], "folder", &form.folder, form.field == 0);
    for (at, (kind, ticked)) in form.kinds.iter().enumerate() {
        kind_line(frame, rows[at + 1], *kind, *ticked, form.field == at + 1);
    }
    let yes = ExportForm::yes_field();
    let line = Line::from(vec![
        label("replace", form.field == yes),
        tick(form.yes),
        Span::styled(
            "  change or remove what an earlier export wrote there, asked first",
            dim(),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), rows[yes]);
    runs(frame, rows[count + 1], form.task().ok().as_ref());
    error(frame, rows[count + 2], form.error.as_deref());
    keys(
        frame,
        rows[count + 3],
        "enter export   tab next   space tick   esc cancel",
    );
}

/// The backup form: the repository, a branch, a message and the variable
/// a token is read from, a tick for each kind, and what it would run.
fn backing(form: &BackupForm, frame: &mut Frame, area: Rect) {
    let count = BackupForm::lines();
    let height = u16::try_from(count).unwrap_or(7) + 7;
    let inner = popup(
        frame,
        area,
        "Back the library up to a git repository",
        84,
        height,
    );
    let rows = lines(inner, count + 5);

    for (index, name) in BACKUP_FIELDS.iter().enumerate() {
        input(
            frame,
            rows[index],
            name,
            form.input(index),
            form.field == index,
        );
    }
    let typed = BACKUP_FIELDS.len();
    for (at, (kind, ticked)) in form.kinds.iter().enumerate() {
        kind_line(
            frame,
            rows[typed + at],
            *kind,
            *ticked,
            form.field == typed + at,
        );
    }
    // What `token env` takes, under the rest, rather than in a key line too
    // long to fit: a variable's name, which is all a form is ever given.
    let hint = Line::from(vec![
        Span::raw(" ".repeat(usize::from(LABEL))),
        Span::styled(
            "token env: a variable's name, never a token; empty logs in as git does",
            dim(),
        ),
    ]);
    frame.render_widget(Paragraph::new(hint), rows[count]);
    runs(frame, rows[count + 1], form.task().ok().as_ref());
    error(frame, rows[count + 2], form.error.as_deref());
    keys(
        frame,
        rows[count + 4],
        "enter back up   tab next   space tick   esc cancel",
    );
}

/// One kind's line of a form, ticked or not: the kinds an export or a
/// backup takes.
fn kind_line(frame: &mut Frame, area: Rect, kind: Kind, ticked: bool, focused: bool) {
    let line = Line::from(vec![label(kind.folder(), focused), tick(ticked)]);
    frame.render_widget(Paragraph::new(line), area);
}

fn confirming(confirm: &Confirm, frame: &mut Frame, area: Rect) {
    const WIDTH: u16 = 76;

    let runs = format!("runs  {}", printable(&confirm.task.describe()));
    let mut said: Vec<String> = confirm
        .question
        .iter()
        .map(|line| printable(line))
        .collect();
    said.extend([String::new(), runs]);
    // What the second yes runs, under the first, so both lines a key can
    // start are read before either key is pressed.
    let mut keys = String::from("y  yes        ");
    if let Some(also) = &confirm.also {
        said.push(format!(
            "{}     {}",
            also.key,
            printable(&also.task.describe())
        ));
        keys.push_str(&format!("{}  {}        ", also.key, also.label));
    }
    keys.push_str("n  no");
    let dimmed = said.len() - confirm.question.len() - 1;
    said.push(String::new());

    // Sized to what it says once wrapped, so the answer keys at the bottom are
    // never the part that falls off.
    let room = usize::from(WIDTH.min(area.width).saturating_sub(2)).max(1);
    let rows: usize = said
        .iter()
        .map(|line| line.chars().count().div_ceil(room).max(1))
        .sum();
    let height = u16::try_from(rows + 3).unwrap_or(u16::MAX);

    // The command lines, between the blank after the question and the one
    // before the keys.
    let commands = said.len() - 1 - dimmed..said.len() - 1;
    let mut text: Vec<Line> = said
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            if commands.contains(&index) {
                Line::from(Span::styled(line, dim()))
            } else {
                Line::from(line)
            }
        })
        .collect();
    text.push(Line::from(Span::styled(keys, bold())));

    let box_ = centred(area, WIDTH, height);
    frame.render_widget(Clear, box_);
    let block = Block::bordered()
        .border_type(BorderType::Thick)
        .title(" Confirm ");
    frame.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: false }),
        box_,
    );
}

fn reading(reader: &Reader, frame: &mut Frame, area: Rect) {
    // Everything below the header: output is read in full, and the keys that
    // apply while reading are in its own border.
    let below = 1.min(area.height);
    let box_ = Rect {
        y: area.y + below,
        height: area.height - below,
        ..area
    };
    frame.render_widget(Clear, box_);
    let lines: Vec<Line> = reader
        .lines
        .iter()
        .map(|(tone_of, text)| Line::from(Span::styled(clean(text), tone(*tone_of))))
        .collect();
    let block = Block::bordered()
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(Color::Cyan))
        .title(format!(" {} ", printable(&reader.title)))
        .title_bottom(Line::from(" ↑↓ scroll   esc close ").right_aligned());
    let scroll = u16::try_from(reader.scroll).unwrap_or(u16::MAX);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        box_,
    );
}

/// Where the screen looked for what could be linked, as its texts name it:
/// the workspace, or the folders `mindweaver minds` was given.
pub(crate) fn looked_under(workspace: &Workspace) -> String {
    if workspace.roots.is_empty() {
        return String::from("this workspace");
    }
    let roots: Vec<String> = workspace
        .roots
        .iter()
        .map(|root| printable(&root.display().to_string()))
        .collect();
    roots.join(", ")
}

/// The minds screen: every directory the workspace manages or could, ticked
/// where it does, over everything below the header.
fn minds_screen(app: &App, screen: &Minds, frame: &mut Frame, area: Rect) {
    let below = 1.min(area.height);
    let box_ = Rect {
        y: area.y + below,
        height: area.height - below,
        ..area
    };
    frame.render_widget(Clear, box_);

    let name = app
        .workspace()
        .map_or_else(String::new, |workspace| printable(&workspace.name));
    // Where it looked, when that is not the workspace the title names.
    let under = app
        .workspace()
        .filter(|workspace| !workspace.roots.is_empty())
        .map_or_else(String::new, |workspace| {
            format!(", found under {}", looked_under(workspace))
        });
    let looked = app
        .workspace()
        .map_or_else(|| String::from("this workspace"), looked_under);
    let (link, init, unlink) = screen.counts();
    let waiting = match (link, unlink) {
        (0, 0) => String::from("nothing marked"),
        _ if init > 0 => {
            format!("{link} to link, {init} of them after mind init · {unlink} to unlink")
        }
        _ => format!("{link} to link · {unlink} to unlink"),
    };
    let block = Block::bordered()
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(Color::Cyan))
        .title(format!(" Minds in {name}{under} "))
        .title_bottom(
            Line::from(format!(
                " ↑↓ move   space tick   a apply   esc back · {waiting} "
            ))
            .right_aligned(),
        );
    let inner = block.inner(box_);
    frame.render_widget(block, box_);

    // What the scan missed gets a strip of its own, as a project's unreadable
    // files do, so a folder typed wrong reads as that rather than as a folder
    // with nothing in it, and no list scrolls it out of sight. Sized without
    // `clamp`, which panics on a screen too short for its floor.
    let warnings = app
        .workspace()
        .map_or(&[][..], |workspace| &workspace.scan_warnings[..]);
    let inner = if warnings.is_empty() {
        inner
    } else {
        let height = u16::try_from(warnings.len() + 2)
            .unwrap_or(u16::MAX)
            .min((inner.height / 2).max(3));
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(height)])
            .split(inner);
        let lines: Vec<Line> = warnings
            .iter()
            .map(|warning| Line::from(Span::styled(printable(warning), warn())))
            .collect();
        let strip = Block::bordered()
            .title(" What the scan missed ")
            .border_style(warn());
        frame.render_widget(
            Paragraph::new(lines).block(strip).wrap(Wrap { trim: true }),
            parts[1],
        );
        parts[0]
    };

    if screen.rows.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!(
                    "nothing linked, and no repository under {looked}; l links one by its path"
                ),
                dim(),
            ))
            .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }

    let width = screen
        .rows
        .iter()
        .map(|row| printable(&row.path).chars().count())
        .max()
        .unwrap_or(0)
        .min(50);
    // Padded to the longest standing too, so what applying would do lines
    // up in one column, and that column is the one to keep in sight: a
    // standing, which can name every agent file a repository holds, is cut
    // to what the pane leaves once the highlight, the box, the path and the
    // widest change any row could ask for have their room, so ticking one
    // never moves the column. `mindweaver scan --suggest` says it whole.
    let offered = screen
        .rows
        .iter()
        .map(|row| columns(pending(row.offers()).0))
        .max()
        .unwrap_or(0);
    let room = usize::from(inner.width).saturating_sub(2 + 4 + width + 2 + offered);
    let said = screen
        .rows
        .iter()
        .map(|row| columns(&standing(row).0))
        .max()
        .unwrap_or(0)
        .min(room);
    let items: Vec<ListItem> = screen
        .rows
        .iter()
        .map(|row| mind_row(row, width, said))
        .collect();
    let mut state = ListState::default();
    state.select(Some(screen.cursor));
    let list = List::new(items)
        .highlight_style(selected(true))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, inner, &mut state);
}

/// One directory: its box, its path, where it stands, and what applying
/// would do to it, coloured by what it will become, not by what it is.
fn mind_row(row: &minds::Row, width: usize, said: usize) -> ListItem<'_> {
    let change = row.change();
    let colour = match change {
        Some(MindsChange::Link | MindsChange::InitAndLink) => Color::Green,
        Some(MindsChange::Unlink) => Color::Red,
        None => Color::Reset,
    };
    let path = printable(&row.path);
    let padding = " ".repeat(width.saturating_sub(path.chars().count()));
    let (standing, style) = standing(row);
    let standing = cut(&standing, said);
    let after = " ".repeat(said.saturating_sub(columns(&standing)));
    let mut spans = vec![
        Span::styled(
            if row.ticked { "[x] " } else { "[ ] " },
            Style::default().fg(colour),
        ),
        Span::raw(format!("{path}{padding}  ")),
        Span::styled(standing, style),
        Span::raw(after),
    ];
    if let Some((text, colour)) = change.map(pending) {
        spans.push(Span::styled(text, Style::default().fg(colour)));
    }
    ListItem::new(Line::from(spans))
}

/// What applying a row would do to it, in words, and in what colour.
fn pending(change: MindsChange) -> (&'static str, Color) {
    match change {
        MindsChange::Link => ("  + link", Color::Green),
        MindsChange::InitAndLink => ("  + mind init, then link", Color::Green),
        MindsChange::Unlink => ("  - unlink", Color::Red),
    }
}

/// `text` in at most `room` columns, ending in `…` where it was cut, so a
/// standing cut short reads as cut rather than as all there is to it.
fn cut(text: &str, room: usize) -> String {
    if columns(text) <= room {
        return text.to_owned();
    }
    let mut kept = String::new();
    let mut taken = 0;
    for character in text.chars() {
        let drawn = columns(character.encode_utf8(&mut [0; 4]));
        // A column left over for the ellipsis.
        if taken + drawn >= room {
            break;
        }
        taken += drawn;
        kept.push(character);
    }
    kept.truncate(kept.trim_end().len());
    if room > 0 {
        kept.push('…');
    }
    kept
}

/// Where a row stands, in words, and how loudly: for one found, why it is
/// worth linking.
fn standing(row: &minds::Row) -> (String, Style) {
    match row.standing {
        minds::Standing::Linked { opens: true } => (String::from("linked"), dim()),
        minds::Standing::Linked { opens: false } => (
            String::from("cannot be opened; untick it to unlink"),
            Style::default().fg(Color::Red),
        ),
        minds::Standing::Found { mind: true } => (row.why.clone(), dim()),
        minds::Standing::Found { mind: false } => (found_repository(&row.why), warn()),
    }
}

/// A repository found, by why it is worth linking, and what linking it
/// takes first: on the minds screen and in the link form alike.
fn found_repository(why: &str) -> String {
    format!("{why} (mind init first)")
}

/// Drawn while a command runs, which for a gather means the network.
fn working(busy: &str, frame: &mut Frame, area: Rect) {
    let text = printable(busy);
    // A floor rather than a `clamp` to the screen's width, which panics on a
    // screen narrower than the floor; `centred` keeps it on the screen.
    let width = u16::try_from(text.chars().count() + 4)
        .unwrap_or(u16::MAX)
        .max(30);
    let box_ = centred(area, width, 3);
    frame.render_widget(Clear, box_);
    let block = Block::bordered()
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" working ");
    frame.render_widget(Paragraph::new(text).block(block), box_);
}

/// Drawn while a task runs apart: the command it is, the last step it
/// reported, redrawn as it reports more, and how to stop it while it still
/// can be.
fn watching(running: &Running, frame: &mut Frame, area: Rect) {
    let command = printable(&running.task.describe());
    let step = running
        .progress
        .map_or_else(|| String::from("starting"), said);
    let (late, cancelling) = stops_short_of(&running.task);
    let (stop, style) = match running.progress {
        // Past the run's last check, where `esc` changes nothing: said, so
        // that neither the key nor a cancel already asked for promises a stop
        // the run will not make. Loud only for whoever asked for one.
        Some(progress) if !progress.cancellable() => {
            (late, if running.cancelling { warn() } else { dim() })
        }
        _ if running.cancelling => (cancelling, warn()),
        _ => ("esc cancels", dim()),
    };
    let longest = [command.chars().count(), step.chars().count(), stop.len()]
        .into_iter()
        .max()
        .unwrap_or(0);
    // Floored as `working` floors its box: this one is drawn on every frame
    // of a gather, and a panic here would end the gather wherever it was.
    let width = u16::try_from(longest + 4).unwrap_or(u16::MAX).max(30);
    let box_ = centred(area, width, 5);
    frame.render_widget(Clear, box_);
    let block = Block::bordered()
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" working ");
    let text = vec![
        Line::from(command),
        Line::from(step),
        Line::from(Span::styled(stop, style)),
    ];
    frame.render_widget(Paragraph::new(text).block(block), box_);
}

/// What a run says once a cancel is too late, and while one asked in time
/// is honoured: a gather stops short of placing, a backup of pushing. By
/// what the task's lines are, as the one list says ([`followed`]).
fn stops_short_of(task: &Task) -> (&'static str, &'static str) {
    match task.commands().iter().find_map(|line| followed(line)) {
        Some(Followed::Backup) => (
            "too late to cancel: pushing",
            "cancelling, before anything is pushed",
        ),
        Some(Followed::Gather) | None => (
            "too late to cancel: placing",
            "cancelling, before anything is placed",
        ),
    }
}

// ---------------------------------------------------------------------------
// Pieces
// ---------------------------------------------------------------------------

/// A cleared, bordered box in the middle of the screen, and the room inside
/// it.
fn popup(frame: &mut Frame, area: Rect, title: &str, width: u16, height: u16) -> Rect {
    let box_ = centred(area, width, height);
    frame.render_widget(Clear, box_);
    let block = Block::bordered()
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(Color::Cyan))
        .title(format!(" {title} "));
    let inner = block.inner(box_);
    frame.render_widget(block, box_);
    inner
}

/// `count` rows of one line each, from the top of `area`.
fn lines(area: Rect, count: usize) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(1); count])
        .split(area)
        .to_vec()
}

/// A form's label, lit when the keyboard is on its line.
fn label(text: &str, focused: bool) -> Span<'static> {
    let style = if focused {
        bold().fg(Color::Cyan)
    } else {
        dim()
    };
    Span::styled(format!("{text:<width$}", width = usize::from(LABEL)), style)
}

/// One labelled line of typing, with the terminal's cursor in it when the
/// keyboard is there.
fn input(frame: &mut Frame, area: Rect, name: &str, input: &Input, focused: bool) {
    let room = usize::from(area.width.saturating_sub(LABEL));
    let (visible, cursor) = window(input, room);
    let line = Line::from(vec![label(name, focused), Span::raw(visible)]);
    frame.render_widget(Paragraph::new(line), area);
    if focused {
        let x = area.x + LABEL + u16::try_from(cursor).unwrap_or(0);
        frame.set_cursor_position((x.min(area.right().saturating_sub(1)), area.y));
    }
}

/// The part of an input that fits in `room` cells, and where the cursor sits
/// inside it. Scrolled so the cursor is always in view.
fn window(input: &Input, room: usize) -> (String, usize) {
    if room == 0 {
        return (String::new(), 0);
    }
    let start = input.cursor().saturating_sub(room - 1);
    let visible = input
        .text()
        .chars()
        .skip(start)
        .take(room)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    (visible, input.cursor() - start)
}

/// The command a form is about to run, as it would be typed.
fn runs(frame: &mut Frame, area: Rect, task: Option<&Task>) {
    let text = task.map_or_else(String::new, |task| printable(&task.describe()));
    let line = Line::from(vec![
        Span::styled(
            format!("{:<width$}", "runs", width = usize::from(LABEL)),
            dim(),
        ),
        Span::styled(text, Style::default().fg(Color::Gray)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn error(frame: &mut Frame, area: Rect, error: Option<&str>) {
    if let Some(error) = error {
        frame.render_widget(
            Paragraph::new(Span::styled(
                printable(error),
                Style::default().fg(Color::Red),
            )),
            area,
        );
    }
}

fn keys(frame: &mut Frame, area: Rect, text: &str) {
    frame.render_widget(Paragraph::new(Span::styled(text.to_owned(), dim())), area);
}

/// A line of output made safe to draw, keeping its indentation: a tab becomes
/// spaces, and any other control character a space, so a carriage return in
/// somebody's file cannot move the cursor and draw over another line.
fn clean(text: &str) -> String {
    text.chars()
        .flat_map(|c| match c {
            '\t' => vec![' '; 4],
            c if c.is_control() => vec![' '],
            c => vec![c],
        })
        .collect()
}

fn tone(tone: Tone) -> Style {
    match tone {
        Tone::Plain => Style::default(),
        Tone::Info => Style::default().fg(Color::Gray),
        Tone::Good => Style::default().fg(Color::Green),
        Tone::Warn => warn(),
        Tone::Bad => Style::default().fg(Color::Red),
    }
}

fn bold() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn warn() -> Style {
    Style::default().fg(Color::Yellow)
}
