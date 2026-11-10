//! The commands that change artifacts: `add`, `edit`, `rename`, `remove` and
//! `templates`, at either level.
//!
//! Each takes the holder it acts on (a project, or the workspace's own), so
//! `mind add` and `mindweaver add -p alpha` are one function given one project,
//! and `mindweaver add` is the same function given the workspace. Each also takes
//! the workspace the command resolved and hands it to core: its templates are
//! among those `add` starts from, and core's lifecycle sees to its record of
//! what it installed: kept through an edit, carried through a rename, and
//! dropped when the copy goes. Here that is only said, along with what
//! opening the ledger to do it scrubbed out of it.

use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read as _};
use std::path::Path;

use mindweaver_core::create::Start;
use mindweaver_core::everywhere::{self, EverywhereError, Plan, Step};
use mindweaver_core::install::{InstallError, Removed};
use mindweaver_core::lifecycle::{self, Edited, Record};
use mindweaver_core::mcp;
use mindweaver_core::template::{self, Origin, Template};
use mindweaver_core::{
    paths, AgentHomes, Kind, MindProject, Reference, Standing, Target, WeaverWorkspace,
    WorkspaceError,
};

use super::typed;
use crate::commands::gather::scrubbed;
use crate::commands::install::{listed, named, render_said, shown, still};
use crate::commands::update::listed as words_listed;
use crate::error::deleting;
use crate::render::{printable, table, verdict};
use crate::{CliError, Editing, Failure, Given, Outcome, Warning};

/// What a dry run that would delete something ends with.
const UNREMOVED: &str = "nothing removed: add --yes to go ahead\n";

/// `add`, at either level: `kind` `name`, described as `description`, from
/// the template `template` names, or for an MCP server from how `server`
/// says an agent starts or reaches it, or else from scratch; core judges
/// each before a byte is written ([`lifecycle::add`]).
pub(crate) fn add_to(
    workspace: Option<&WeaverWorkspace>,
    holder: &MindProject,
    kind: Kind,
    (name, description): (&str, &str),
    template: Option<&str>,
    server: Option<&mcp::Given>,
) -> Result<Outcome, Failure> {
    let template = template
        .map(|template| template::find(workspace, holder, kind, template))
        .transpose()
        .map_err(CliError::from)?;
    // One or the other, never both: the grammar refuses a transport given
    // with a template, which says how a server is started as surely.
    let start = match (&template, server) {
        (Some(template), _) => Start::Template(template),
        (None, Some(given)) => Start::Server(given),
        (None, None) => Start::Scratch,
    };
    let added = lifecycle::add(workspace, holder, kind, name, description, start)
        .map_err(CliError::from)?;
    let mut outcome = Outcome::plain(format!(
        "created {} {} at {}\n",
        added.artifact.kind(),
        added.artifact.name(),
        added.artifact.file().display()
    ));
    recorded(&mut outcome, added.stale, stale);
    Ok(scrubbed(&added.scrub).before(outcome))
}

/// `edit`, at either level: in an editor, or, with `--contents`, by
/// replacing what the artifact holds with what a file or standard input
/// does, which core checks before it writes a byte
/// ([`lifecycle::replace_contents`]), or, for an MCP server told how it is
/// started or reached, by writing that again ([`lifecycle::retransport`]),
/// held to the same rules. Either way the same is said after:
/// whether it changed, what `validate` makes of it now, and that an
/// installed copy stays installed, edited.
///
/// A file `--contents` names is typed from `directory`, the one the command
/// works in, as every path a command takes is, so a line a front end runs
/// from the workspace's root reads what it names there.
pub(crate) fn edit_in(
    workspace: Option<&WeaverWorkspace>,
    holder: &MindProject,
    reference: &str,
    editing: &Editing,
    directory: &Path,
) -> Result<Outcome, Failure> {
    let target = lifecycle::find_to_change(workspace, holder, &Reference::parse(reference))
        .map_err(CliError::from)?;
    let finished = match (editing.server.given(), editing.given()) {
        // A server's options, which the grammar refuses beside an editor
        // and contents: what its transport is written again from.
        (Some(server), _) => {
            lifecycle::retransport(workspace, holder, &target, &server).map_err(CliError::from)?
        }
        (None, None) => open(workspace, holder, &target, editing.editor.as_deref())?,
        (None, Some(given)) => {
            let contents = read_given(given, directory)?;
            lifecycle::replace_contents(
                workspace,
                holder,
                &target,
                &contents,
                editing.if_unchanged.as_deref(),
            )
            .map_err(CliError::from)?
        }
    };

    let mut outcome = edited(holder, &target, finished.changed);
    recorded(&mut outcome, finished.copy, |record| {
        (record == Record::Kept).then(|| {
            String::from(
                "it was installed by `mindweaver install`, and stays so; edited, it is left alone by \
                 `mindweaver install` and `mindweaver uninstall` unless they are forced",
            )
        })
    });
    Ok(outcome)
}

/// Open `target` in `editor`, or the one the environment names, and say
/// what the edit came to once it is closed.
fn open(
    workspace: Option<&WeaverWorkspace>,
    holder: &MindProject,
    target: &Target,
    editor: Option<&str>,
) -> Result<Edited, Failure> {
    let edit = lifecycle::start_edit(target);
    let editor = editor.map_or_else(default_editor, str::to_owned);
    let opened = open_in_editor(&editor, edit.file());
    // Finished whatever the editor came to. One that writes the file and then
    // exits non-zero, as vim does on `:cq`, has changed the copy all the same.
    let finished = lifecycle::finish_edit(workspace, holder, edit);
    if let Err(error) = opened {
        let mut failure = Failure::from(error);
        if let Err(error) = finished.copy {
            failure
                .warnings
                .push(Warning::coded(error.code(), format!("warning: {error}")));
        }
        return Err(failure);
    }
    Ok(finished)
}

/// What `--contents` names, read whole: standard input, or a file typed
/// from `directory`.
///
/// Read here rather than in core, which never reads standard input, and
/// hands the bytes over as they are: whether they are text, and what they
/// may say, is core's to decide.
fn read_given(given: Given<'_>, directory: &Path) -> Result<Vec<u8>, CliError> {
    match given {
        Given::Input => {
            let mut contents = Vec::new();
            io::stdin()
                .lock()
                .read_to_end(&mut contents)
                .map_err(|source| CliError::ContentsUnread {
                    from: String::from("standard input"),
                    source,
                })?;
            Ok(contents)
        }
        Given::File(path) => {
            let path = typed(directory, path)?;
            fs::read(&path).map_err(|source| CliError::ContentsUnread {
                from: path.display().to_string(),
                source,
            })
        }
    }
}

pub(crate) fn rename_in(
    workspace: Option<&WeaverWorkspace>,
    holder: &MindProject,
    reference: &str,
    to: &str,
) -> Result<Outcome, Failure> {
    let target = lifecycle::find_to_change(workspace, holder, &Reference::parse(reference))
        .map_err(CliError::from)?;
    let renamed = lifecycle::rename(workspace, holder, &target, to).map_err(CliError::from)?;
    let mut outcome = Outcome::plain(format!(
        "renamed {} {} to {} at {}\n",
        target.kind,
        target.name,
        renamed.target.name,
        renamed.target.path.display()
    ));
    let to = renamed.target.name.clone();
    recorded(&mut outcome, renamed.copy, |record| {
        match record {
        Record::Moved { edited: true } => Some(String::from(
            "it was installed by `mindweaver install`, and stays so under its new name; renamed, it \
             is left alone by `mindweaver install` and `mindweaver uninstall` unless they are forced",
        )),
        // A rule's rename moves its file as it is, so nothing in it reads
        // as an edit, and the uninstall that put it there takes it out.
        Record::Moved { edited: false } => Some(String::from(
            "it was installed by `mindweaver install`, and stays so under its new name; renamed, it \
             still holds what was installed, and `mindweaver uninstall` takes it out",
        )),
        Record::Dropped => Some(format!(
            "it had been installed, but `{to}` is installed already; renamed, it is yours, and \
             `mindweaver install` leaves it alone"
        )),
        Record::Untouched | Record::Kept => None,
    }
    });
    recorded(&mut outcome, renamed.stale, stale);
    Ok(scrubbed(&renamed.scrub).before(outcome))
}

pub(crate) fn remove_from(
    workspace: Option<&WeaverWorkspace>,
    holder: &MindProject,
    reference: &str,
    yes: bool,
) -> Result<Outcome, Failure> {
    let target = lifecycle::find_to_change(workspace, holder, &Reference::parse(reference))
        .map_err(CliError::from)?;
    // Deleting is the one thing here that cannot be taken back, so it is
    // asked for twice: once by naming it, once by saying yes.
    if !yes {
        return Err(CliError::Unconfirmed {
            kind: target.kind,
            path: target.path.clone(),
            files: lifecycle::files(&target),
        }
        .into());
    }
    let removed = lifecycle::remove(workspace, holder, &target).map_err(CliError::from)?;
    let mut outcome = Outcome::plain(format!(
        "removed {} {} from {}\n",
        target.kind,
        target.name,
        target.path.display()
    ));
    recorded(&mut outcome, removed.copy, |record| {
        (record == Record::Dropped).then(|| {
            String::from(
                "it had been installed, and is uninstalled with it; `mindweaver install` can put it back",
            )
        })
    });
    Ok(scrubbed(&removed.scrub).before(outcome))
}

/// `mindweaver remove` of one of the workspace's own, without `--everywhere`:
/// removed as [`remove_from`] removes it, or said what would be, and beside
/// it each project that has a copy installed from it, which nothing offers
/// once it is gone, as core plans it ([`everywhere::plan`]): orphaned there,
/// left as it is, never updated again, and taken out only when somebody
/// says so. Said in the dry run with what `--everywhere` would do about it,
/// and after the removal with what takes them out now.
///
/// Which projects is read before the removal, while the library copy still
/// says what was installed from it, and a ledger that cannot be read is a
/// warning beside the removal rather than a refusal of it: the removal is
/// the workspace's, and the projects only to be told of. They are told once
/// the removal is done, or in the dry run, which refuses without `--yes`.
/// A removal that fails says nothing of them: it may have left the library
/// copy whole, offering it still, or a skill's folder half deleted, offering
/// nothing, and either way its error is what there is to say.
pub(crate) fn remove_own(
    workspace: &WeaverWorkspace,
    reference: &str,
    yes: bool,
) -> Result<Outcome, Failure> {
    let orphaning = match everywhere::plan(workspace, &Reference::parse(reference)) {
        Ok(plan) => orphaning(&plan, yes),
        // The removal finds the same thing, and refuses it the same way.
        Err(EverywhereError::Lifecycle(_)) => Vec::new(),
        Err(error) => vec![Warning::coded(
            error.code(),
            format!("warning: whether it is installed in any project cannot be told: {error}"),
        )],
    };
    match remove_from(Some(workspace), &workspace.own(), reference, yes) {
        Ok(mut outcome) => {
            outcome.ok &= orphaning.is_empty();
            outcome.stderr.extend(orphaning);
            Ok(outcome)
        }
        Err(mut failure) => {
            if !yes {
                failure.warnings.extend(orphaning);
            }
            Err(failure)
        }
    }
}

/// What [`remove_own`] says of the projects `plan` finds a copy installed
/// in: one warning naming them all, as a dry run or, once `removed`, as
/// what was done. None when there are none.
fn orphaning(plan: &Plan, removed: bool) -> Vec<Warning> {
    let projects: Vec<&str> = plan
        .orphaned()
        .iter()
        .map(|copy| copy.project.name())
        .collect();
    if projects.is_empty() {
        return Vec::new();
    }
    let what = named(plan.target.kind, &plan.target.name);
    let projects = words_listed(&projects);
    let said = if removed {
        format!(
            "warning: {what} is still installed in {projects}, orphaned now that nothing offers \
             it; `mindweaver uninstall --orphaned --all-projects` takes out what nothing offers"
        )
    } else {
        format!(
            "warning: {what} is installed in {projects}, which removing it alone would leave \
             orphaned there; --everywhere takes it out of them too"
        )
    };
    vec![said.into()]
}

/// `mindweaver remove --everywhere` without `--yes`: what removing `reference`,
/// one of the workspace's own, from every project it links and then from
/// the workspace would do, as core plans it ([`everywhere::plan`]), nothing
/// written. A line for each project's copy, saying how it stands, then one
/// for the library copy, in the order they would go.
pub(crate) fn would_remove_everywhere(
    workspace: &WeaverWorkspace,
    reference: &str,
) -> Result<Outcome, Failure> {
    let plan = everywhere::plan(workspace, &Reference::parse(reference)).map_err(CliError::from)?;
    let target = &plan.target;
    let mut outcome = Outcome::plain(String::new());
    for copy in &plan.copies {
        let what = named(target.kind, &target.name);
        let into = copy.project.name();
        let standing = copy.standing.slug();
        let _ = match &copy.step {
            Step::Uninstall => writeln!(
                outcome.stdout,
                "would remove {what} from {into}, where it is {standing}"
            ),
            Step::Leave(paths) => writeln!(
                outcome.stdout,
                "would leave {what} in {into}, where it is {standing}, at {}",
                listed(&copy.project, paths)
            ),
            Step::Somebodys(path) => writeln!(
                outcome.stdout,
                "would leave {what} in {into}, where it is {standing}, at {}",
                shown(&copy.project, path)
            ),
        };
    }
    let _ = writeln!(
        outcome.stdout,
        "would remove {} {} from {}",
        target.kind,
        target.name,
        deleting(&target.path, &target.kind, &lifecycle::files(target))
    );
    outcome.stdout.push_str(UNREMOVED);
    unread(&mut outcome, &plan.unread);
    Ok(outcome)
}

/// `mindweaver remove --everywhere --yes`: `reference`, one of the workspace's
/// own, uninstalled from every project it links and then removed from the
/// workspace, as core does it ([`everywhere::apply`]), handing it the
/// agents' own folders, `homes`, as an uninstall does.
///
/// A line for each project's copy, as the dry run has it but for what was
/// done, then the library copy's. A copy left because it was changed since
/// it was installed is offered by nothing from then on, which a note says
/// beside it with the command that takes it out once somebody has looked.
/// A copy whose uninstall failed is an error of its project, as among
/// several in `mindweaver uninstall`, and the rest stand.
pub(crate) fn remove_everywhere(
    workspace: &WeaverWorkspace,
    reference: &str,
    homes: &AgentHomes,
) -> Result<Outcome, Failure> {
    let everywhere::Applied {
        plan,
        removed,
        scrub,
    } = everywhere::apply(workspace, &Reference::parse(reference), homes)
        .map_err(CliError::from)?;
    let target = &plan.target;
    let what = named(target.kind, &target.name);
    let among = plan.copies.len() > 1;
    let mut outcome = Outcome::plain(String::new());
    let mut orphaned: Vec<&str> = Vec::new();
    for copy in &plan.copies {
        let project = &copy.project;
        let from = project.name();
        match (&copy.step, &copy.done) {
            (Step::Leave(paths), _) | (_, Some(Ok(Removed::Edited { paths, .. }))) => {
                let _ = writeln!(
                    outcome.stdout,
                    "left {what} in {from}, where it is {}, at {}",
                    Standing::Edited.slug(),
                    listed(project, paths)
                );
                orphaned.push(from);
            }
            (Step::Somebodys(path), _) | (_, Some(Ok(Removed::Foreign { path, .. }))) => {
                let _ = writeln!(
                    outcome.stdout,
                    "left {what} in {from}, where it is {}, at {}",
                    Standing::Foreign.slug(),
                    shown(project, path)
                );
            }
            (
                _,
                Some(Ok(Removed::Removed {
                    still_excluded,
                    rendered,
                    ..
                })),
            ) => {
                let _ = writeln!(outcome.stdout, "removed {what} from {from}");
                // What the project's rules come to without it.
                if let Some(rendered) = rendered {
                    let (warned, notes) =
                        render_said(rendered, &target.name, among.then_some(from));
                    outcome.ok &= warned.is_empty();
                    outcome.stderr.extend(warned);
                    outcome.stderr.extend(notes);
                }
                if let Some(reason) = still_excluded {
                    outcome.stderr.push(still(&what, from, reason));
                    outcome.ok = false;
                }
            }
            (_, Some(Err(error))) => {
                outcome.stderr.push(Warning::coded(
                    error.code(),
                    format!("error: {from}: {error}"),
                ));
                outcome.ok = false;
            }
            // Taken out by something else since it was planned.
            (_, Some(Ok(Removed::Absent { .. })) | None) => {}
        }
    }
    let mut removed_scrub = None;
    match removed {
        Ok(removed) => {
            let _ = writeln!(
                outcome.stdout,
                "removed {} {} from {}",
                target.kind,
                target.name,
                target.path.display()
            );
            // The library copy is no copy installed anywhere, so nothing
            // is said of a record but one that could not be changed.
            recorded(&mut outcome, removed.copy, |_| None);
            // Named as `uninstall` takes it: a route is a rule's alone, but
            // a rule of one word could be a skill's name too.
            let typed = match target.kind {
                Kind::Skill => target.name.to_string(),
                Kind::Rule | Kind::Mcp => target.qualified_name(),
            };
            for from in orphaned {
                outcome.stderr.push(
                    format!(
                        "note: nothing offers {what} in {from} any more; `mindweaver uninstall -p \
                         {from} {typed} --force` takes it out"
                    )
                    .into(),
                );
            }
            removed_scrub = Some(removed.scrub);
        }
        Err(error) => {
            outcome
                .stderr
                .push(Warning::coded(error.code(), format!("error: {error}")));
            outcome.ok = false;
        }
    }
    unread(&mut outcome, &plan.unread);
    let outcome = match removed_scrub {
        Some(later) => scrubbed(&later).before(outcome),
        None => outcome,
    };
    Ok(scrubbed(&scrub).before(outcome))
}

/// A warning for each project the workspace links that could not be
/// opened, beside what is said of the rest: whether it holds a copy cannot
/// be told.
fn unread(outcome: &mut Outcome, unread: &[WorkspaceError]) {
    for failure in unread {
        outcome.stderr.push(Warning::coded(
            failure.code(),
            format!("warning: {failure}"),
        ));
        outcome.ok = false;
    }
}

/// What `add --template` can start from in `holder`, with `workspace`'s:
/// its name, its kind and where it is, a route from `holder` or `built in`;
/// with the templates kept in `found`, for `--json` to print in place of
/// the text.
pub(crate) fn templates_of(
    workspace: Option<&WeaverWorkspace>,
    holder: &MindProject,
    found: &mut Option<Vec<Template>>,
) -> Result<Outcome, Failure> {
    let templates = template::templates(workspace, holder).map_err(CliError::from)?;
    let rows: Vec<Vec<String>> = templates
        .iter()
        .map(|template| {
            let origin = match &template.origin {
                Origin::BuiltIn => String::from("built in"),
                Origin::Path(path) => paths::relative_to(path, holder.root())
                    .as_deref()
                    .and_then(paths::to_config_string)
                    .unwrap_or_else(|| path.display().to_string()),
            };
            vec![
                printable(&template.name),
                template.kind.slug().to_owned(),
                printable(&origin),
            ]
        })
        .collect();
    *found = Some(templates);
    Ok(Outcome::plain(table(&rows)))
}

// ---------------------------------------------------------------------------
// edit, rename, remove
// ---------------------------------------------------------------------------

/// The editor to use when none was named: `$VISUAL`, then `$EDITOR`, then the
/// one every system has: the order git looks in.
fn default_editor() -> String {
    ["VISUAL", "EDITOR"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.trim().is_empty())
        .unwrap_or_else(|| String::from(if cfg!(windows) { "notepad" } else { "vi" }))
}

/// Open `file` in `editor` and wait for it to be closed.
///
/// On Unix the editor runs through `sh`, as git runs it, so `code --wait` and
/// an editor whose path has a space in it both work. The file is passed as an
/// argument rather than spliced into the command, so its name is never read
/// by the shell.
fn open_in_editor(editor: &str, file: &Path) -> Result<(), CliError> {
    #[cfg(unix)]
    let mut command = {
        let mut command = std::process::Command::new("sh");
        command
            .arg("-c")
            .arg(format!("{editor} \"$1\""))
            .arg(editor)
            .arg(file);
        command
    };
    #[cfg(not(unix))]
    let mut command = {
        let mut words = editor.split_whitespace();
        let mut command = std::process::Command::new(words.next().unwrap_or("notepad"));
        command.args(words).arg(file);
        command
    };

    let status = command.status().map_err(|source| CliError::Editor {
        editor: editor.to_owned(),
        source,
    })?;
    if !status.success() {
        return Err(CliError::EditorFailed {
            editor: editor.to_owned(),
            status: status.to_string(),
        });
    }
    Ok(())
}

/// What editing came to: whether anything changed, and whether what is there
/// now still passes `validate`, the question worth answering the moment the
/// editor closes, not at the next CI run.
fn edited(project: &MindProject, target: &Target, changed: bool) -> Outcome {
    let what = format!("{} {}", target.kind, target.name);
    let head = if changed {
        format!("edited {what}")
    } else {
        format!("left {what} unchanged")
    };
    let artifact = match lifecycle::load(project, target.kind, &target.path, &target.name) {
        Ok(artifact) => artifact,
        Err(error) => {
            return Outcome {
                stdout: format!("{head}, and it does not read\n"),
                stderr: vec![Warning::coded(error.code(), format!("warning: {error}"))],
                ok: false,
            }
        }
    };

    let mut stdout = String::new();
    let invalid = verdict(&mut stdout, &head, &artifact.validate());
    Outcome {
        stdout,
        stderr: Vec::new(),
        ok: !invalid,
    }
}

/// Say what a change did to a record that `mindweaver install` put something
/// into the project, when `why` has something to say of it.
///
/// A record that could not be changed is a warning rather than an error, as
/// core reports it: the change it was about has already been made, and
/// reporting it as not made would be false.
pub(crate) fn recorded(
    outcome: &mut Outcome,
    record: Result<Record, InstallError>,
    why: impl Fn(Record) -> Option<String>,
) {
    match record {
        Ok(record) => {
            if let Some(why) = why(record) {
                let _ = writeln!(outcome.stdout, "  {why}");
            }
        }
        Err(error) => {
            outcome
                .stderr
                .push(Warning::coded(error.code(), format!("warning: {error}")));
            outcome.ok = false;
        }
    }
}

/// What to say of a record of an install under the name a change wrote,
/// whose copy had gone, and which the change dropped.
pub(crate) fn stale(record: Record) -> Option<String> {
    (record == Record::Dropped).then(|| {
        String::from(
            "an old record said `mindweaver install` had put one of that name here; it is gone",
        )
    })
}
