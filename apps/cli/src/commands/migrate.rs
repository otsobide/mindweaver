//! `mindweaver migrate` and `mind migrate`: bringing what an older Mindweaver
//! made into the layout a new one has. `--library` moves a workspace's own
//! into its library; `--names` moves a workspace a Mindflayer build made to
//! Mindweaver's names, with what it wrote in its projects; `mind migrate`,
//! `-p` and `--all-projects` move a project from its `mind.toml` or its
//! `settings.json` to a `settings.toml`. Core's [`migrate`] decides
//! everything about each; here it is said.

use std::fmt::Write as _;
use std::path::Path;

use mindweaver_core::claude_rules;
use mindweaver_core::migrate::{
    self, CarriedRule, LeftWhy, MigrateError, NamesPlan, Plan, ProjectNames, ProjectPlan, Stranded,
};
use mindweaver_core::paths;
use mindweaver_core::{Agent, AgentHomes, MarkerFile, MindProject, Names, Share, WeaverWorkspace};

use super::all_or_nothing;
use super::consulted;
use super::gather::scrubbed;
use super::install::{cleared, shown};
use super::settings::{agents, moved, not_copied, outside_note};
use crate::{CliError, Failure, Lookup, Outcome, Warning};

/// `mindweaver migrate --library`: what moving the workspace's own into its
/// library would do, or, with `yes`, doing it.
///
/// The dry run is a report, and exits as one, where `remove`'s is a
/// refusal: what it gives is a line per artifact to read before saying yes,
/// not one deletion that was asked for and did not happen. Refused, both say
/// every reason at once, as a command given several things does, since each
/// has to be put right before it can go ahead.
pub(crate) fn migrate_library(workspace: &WeaverWorkspace, yes: bool) -> Result<Outcome, Failure> {
    if !yes {
        let mut plan = migrate::library_plan(workspace);
        refused(std::mem::take(&mut plan.refusals))?;
        if plan.is_empty() {
            return Ok(Outcome::plain(String::from(NOTHING)));
        }
        let mut stdout = report(workspace, &plan, false);
        stdout.push_str("nothing moved: add --yes to go ahead\n");
        return Ok(Outcome::plain(stdout));
    }

    let mut migrating = workspace.clone();
    match migrate::library(&mut migrating) {
        Ok(migrated) if migrated.plan.is_empty() => Ok(Outcome::plain(String::from(NOTHING))),
        Ok(migrated) => Ok(Outcome::plain(report(&migrating, &migrated.plan, true))),
        // Never without a reason, so what follows the refusal is not reached.
        Err(MigrateError::Refused(refusals)) => {
            refused(refusals)?;
            Ok(Outcome::plain(String::from(NOTHING)))
        }
        Err(error) => {
            // Said with the error, being what the migration left where the
            // workspace no longer looks: it could not be put back.
            let warnings = error
                .stranded()
                .iter()
                .map(|one| {
                    format!(
                        "warning: {} was moved and could not be put back; move it to {} yourself",
                        one.to.display(),
                        one.from.display()
                    )
                    .into()
                })
                .collect();
            Err(Failure {
                error: Box::new(CliError::Migrate(error)),
                warnings,
            })
        }
    }
}

/// What a migration that has nothing to do says, with or without `--yes`.
const NOTHING: &str = "nothing to move: the workspace keeps its own in its library already\n";

/// `mindweaver migrate --names`, and `mind migrate --names` on the workspace
/// a project consults: what moving a workspace a Mindflayer build made to
/// Mindweaver's names would do, or, with `yes`, doing it.
///
/// A report, as `--library`'s dry run is, refused with every reason at
/// once. Git is never run: each rule file git tracks that the move carries
/// is said, with the lines that take the move into the index.
pub(crate) fn migrate_names(workspace: &WeaverWorkspace, yes: bool) -> Result<Outcome, Failure> {
    if !yes {
        let mut plan = migrate::names_plan(workspace).map_err(CliError::Migrate)?;
        refused(std::mem::take(&mut plan.refusals))?;
        if plan.is_empty() {
            return Ok(Outcome::plain(named(&plan, false)));
        }
        let mut stdout = names_report(&plan, false);
        stdout.push_str("nothing moved: add --yes to go ahead\n");
        let mut outcome = Outcome::plain(stdout);
        outcome.stderr.extend(tracked(&plan));
        return Ok(outcome);
    }

    let mut migrating = workspace.clone();
    match migrate::names(&mut migrating) {
        Ok(moved) if moved.plan.is_empty() => Ok(Outcome::plain(named(&moved.plan, true))),
        Ok(moved) => {
            let mut outcome = Outcome::plain(names_report(&moved.plan, true));
            outcome.stderr.extend(tracked(&moved.plan));
            Ok(outcome)
        }
        // Never without a reason, so what follows the refusal is not reached.
        Err(MigrateError::Refused(refusals)) => {
            refused(refusals)?;
            Ok(Outcome::plain(String::from(NAMED)))
        }
        Err(error) => {
            let warnings = error
                .left_behind_names()
                .iter()
                .map(|path| {
                    format!(
                        "warning: {} could not be put back as it was before the move",
                        path.display()
                    )
                    .into()
                })
                .collect();
            Err(Failure {
                error: Box::new(CliError::Migrate(error)),
                warnings,
            })
        }
    }
}

/// What a move to Mindweaver's names that has nothing to do says.
const NAMED: &str = "nothing to move: the workspace is under Mindweaver's names already\n";

/// What a move with nothing to do says: what it leaves as it is under
/// Mindflayer's names, a rule file changed since or a block git tracks,
/// which is why a name is still there, then that nothing moves.
fn named(plan: &NamesPlan, done: bool) -> String {
    let mut text = names_report(plan, done);
    text.push_str(NAMED);
    text
}

/// A line for each thing moving to Mindweaver's names does, as what would
/// happen in a dry run and as what did: the workspace's folder and the
/// files in it, the marker's folders and the shelf, then each project.
fn names_report(plan: &NamesPlan, done: bool) -> String {
    let would = |now: &'static str, then: &'static str| if done { now } else { then };
    let mut text = String::new();
    if let Some(folder) = &plan.folder {
        let _ = writeln!(
            text,
            "{} {} to {}",
            would("moved", "would move"),
            folder.from.display(),
            folder.to.display()
        );
    }
    for renamed in &plan.files {
        let _ = writeln!(
            text,
            "{} {} to {}",
            would("renamed", "would rename"),
            renamed.from.display(),
            renamed.to.display()
        );
    }
    for repointed in &plan.directories {
        let _ = writeln!(
            text,
            "{} {} in the marker at {}",
            would("pointed", "would point"),
            repointed.key,
            route(&repointed.to)
        );
    }
    if plan.shelf > 0 {
        let _ = writeln!(
            text,
            "{} what the ledger records of {} gathered {} with the shelf",
            would("moved", "would move"),
            plan.shelf,
            if plan.shelf == 1 {
                "artifact"
            } else {
                "artifacts"
            }
        );
    }
    for project in &plan.projects {
        project_names(&mut text, project, done);
    }
    for file in &plan.excludes {
        let _ = writeln!(
            text,
            "{} the markers of the block in {} as Mindweaver's",
            would("spelled", "would spell"),
            file.display()
        );
    }
    text
}

/// [`names_report`]'s lines for one project.
fn project_names(text: &mut String, project: &ProjectNames, done: bool) {
    let would = |now: &'static str, then: &'static str| if done { now } else { then };
    for file in &project.blocks {
        let _ = writeln!(
            text,
            "{} the markers of the block in {} as Mindweaver's",
            would("spelled", "would spell"),
            file.display()
        );
    }
    for kept in &project.kept {
        let _ = writeln!(
            text,
            "{} the markers of the block in {} as Mindflayer's: {}",
            would("left", "would leave"),
            kept.path.display(),
            kept.why
        );
    }
    for carried in &project.rules {
        let _ = writeln!(
            text,
            "{} {} to {}",
            would("moved", "would move"),
            carried.from.display(),
            carried.to.display()
        );
    }
    for absent in &project.absent {
        let _ = writeln!(
            text,
            "{} the record of rule `{}` at {}: its file is not there, so nothing moves, and an \
             update puts it back there",
            would("pointed", "would point"),
            absent.route,
            absent.to.display()
        );
    }
    for left in &project.left {
        let why = match left.why {
            LeftWhy::Edited => "changed since Mindweaver wrote it, so it is somebody's",
            LeftWhy::Unlike => "not a file Mindweaver could have written, so it is somebody's",
        };
        let _ = writeln!(
            text,
            "{} {} where it is: {why}",
            would("left", "would leave"),
            left.path.display()
        );
    }
}

/// `path`, a route, as a marker and git spell it: `/` separated on every
/// platform, or as it displays when it is not UTF-8.
fn route(path: &Path) -> String {
    paths::to_config_string(path).unwrap_or_else(|| path.display().to_string())
}

/// What the move says on stderr: for each project with a rule file git
/// tracks that the move carries, how to take the move into the index,
/// which Mindweaver runs no git to do.
///
/// Each file by its route, never the old folder, which may hold a file the
/// move left there: a `git rm -r --cached` of the folder would take that one
/// out of the index too, and out of the team's repository with the commit.
fn tracked(plan: &NamesPlan) -> Vec<Warning> {
    plan.projects
        .iter()
        .filter_map(|project| {
            let routes = |of: fn(&CarriedRule) -> &Path| {
                project
                    .rules
                    .iter()
                    .filter(|carried| carried.tracked)
                    .filter_map(|carried| of(carried).strip_prefix(&project.root).ok())
                    .map(route)
                    .collect::<Vec<String>>()
                    .join(" ")
            };
            let from = routes(|carried| &carried.from);
            if from.is_empty() {
                return None;
            }
            Some(
                format!(
                    "hint: git tracks what moved from {}; `git rm --cached -- {from}` and \
                     `git add -- {}` in {} take the move into the index",
                    project
                        .root
                        .join(claude_rules::folder_under(Names::Mindflayer))
                        .display(),
                    routes(|carried| &carried.to),
                    project.root.display()
                )
                .into(),
            )
        })
        .collect()
}

/// Every refusal, the first as the error and the rest before it.
fn refused(refusals: Vec<migrate::Refusal>) -> Result<(), Failure> {
    match all_or_nothing(refusals.into_iter().map(CliError::Refused).collect()) {
        Some(failure) => Err(failure),
        None => Ok(()),
    }
}

/// A line for each artifact moved, each one a project keeps, and the
/// marker's rewrite: as what will happen in a dry run, and as what did.
fn report(workspace: &WeaverWorkspace, plan: &Plan, done: bool) -> String {
    let (moving, leaving, uncounted) = if done {
        ("moved", "left", "no longer counts")
    } else {
        ("would move", "would leave", "would no longer count")
    };
    let mut text = String::new();
    for one in &plan.moves {
        let _ = writeln!(
            text,
            "{moving} {} to {}",
            one.from.display(),
            one.to.display()
        );
    }
    for left in &plan.left {
        // Said on its line, since it is what changes for whoever installed
        // it: the workspace listed and offered it, and will not.
        let _ = write!(
            text,
            "{leaving} {} where it is: the project at {} holds it",
            left.path.display(),
            left.project.display()
        );
        if left.counted {
            let _ = write!(text, ", and the workspace {uncounted} it as its own");
        }
        text.push('\n');
    }
    let kinds: Vec<&str> = plan
        .kinds
        .iter()
        .map(|repoint| repoint.kind.folder())
        .collect();
    let marker = workspace.config_path();
    let _ = writeln!(
        text,
        "{} {} in {} at the library, {} in {}",
        if done { "pointed" } else { "would point" },
        listed(&kinds),
        marker.display(),
        if done {
            "kept as it was"
        } else {
            "keeping it as it is"
        },
        plan.backup.display(),
    );
    text
}

/// `skills`, `skills and rules`, `a, b and c`.
fn listed(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// What a project's settings are to say once it is moved from a
/// `mind.toml`, and whether to move it or only say what moving it would do.
/// A project a `settings.json` marks keeps the agents and share mode it
/// names, whatever these say.
pub(crate) struct Asked<'a> {
    pub agents: &'a [Agent],
    pub share: Share,
    pub yes: bool,
}

/// `mind migrate`: the project at or above here moved to a `settings.toml`,
/// carrying what the workspace `mind` consults about it records, when that
/// one links it.
///
/// A project nothing links is moved all the same, since its marker is its
/// own, with a hint saying nothing installed was carried along, as `mind
/// agents` says it. One `-W` or the activation names that is no workspace
/// refuses it, as it refuses every command.
///
/// So does a workspace found that cannot be read, in a dry run too, where
/// `mind agents` goes ahead beside a warning and can be run again. What it
/// records decides which copies move, and the move is made once: made
/// without it, nothing is left to carry them once settings mark the
/// project, and a copy recorded without a hash, which only the move holds
/// to its offer (D-04), is dropped the next time the agents change, as one
/// nothing can compare. A project its settings mark moves no copy, from a
/// `settings.json` or with nothing to move at all
/// ([`migrate::carries_installs`]), so for that one it is still a warning.
pub(crate) fn migrate_here(
    project: &MindProject,
    asked: &Asked,
    lookup: &Lookup,
) -> Result<Outcome, Failure> {
    let carries = migrate::carries_installs(project);
    let (workspace, unread) = match consulted(project, lookup) {
        Ok(workspace) => (workspace, None),
        Err(error @ CliError::Chosen { .. }) => return Err(error.into()),
        Err(error) if !carries => (None, Some(error)),
        Err(error) => return Err(CliError::MigrateUnrecorded(Box::new(error)).into()),
    };
    let result = laid_out(workspace.as_ref(), project, asked, &lookup.agents);
    let carried = if asked.yes {
        "was carried along"
    } else {
        "would be carried along"
    };
    // Only beside a move made or laid out that carries copies. Never that no
    // workspace links the project, which the walk up cannot know: one that
    // links it from beside it is never reached that way, and its records are
    // lost to the move just the same, so the hint names `-W`, which reaches
    // it.
    let hint = match (&result, &workspace) {
        _ if !carries => None,
        (Ok(Some(_)), Some(workspace)) if !workspace.is_linked(project.root()) => Some(format!(
            "hint: the workspace at {} does not link this project, so nothing installed in \
                 it {carried}; `mind link` links it, and -W names one that links it from \
                 elsewhere",
            workspace.root().display()
        )),
        (Ok(Some(_)), None) => Some(format!(
            "hint: no workspace here or above links this project, so nothing installed in it \
             {carried}; -W names one that links it from elsewhere"
        )),
        _ => None,
    };
    match result {
        Ok(outcome) => {
            let mut outcome = finished(outcome, project, asked);
            outcome.stderr.extend(hint.map(Warning::from));
            if let Some(error) = unread {
                outcome
                    .stderr
                    .push(Warning::coded(error.code(), format!("warning: {error}")));
                outcome.ok = false;
            }
            Ok(outcome)
        }
        Err(stopped) => Err(stopped.failure()),
    }
}

/// `mindweaver migrate -p`: one project the workspace links moved to a
/// `settings.toml`, carrying what its ledger records along.
pub(crate) fn migrate_one(
    workspace: &WeaverWorkspace,
    project: &MindProject,
    asked: &Asked,
    homes: &AgentHomes,
) -> Result<Outcome, Failure> {
    match laid_out(Some(workspace), project, asked, homes) {
        Ok(outcome) => Ok(finished(outcome, project, asked)),
        Err(stopped) => Err(stopped.failure()),
    }
}

/// `mindweaver migrate --all-projects`: every project the workspace links that
/// a `mind.toml` or a `settings.json` marks, moved to a `settings.toml` one
/// at a time.
///
/// Each is all or nothing on its own, so one that is refused, or fails, is
/// said with `error:` beside what the others did, naming it, and the command
/// exits non-zero; the others go ahead, as a stale entry is a warning beside
/// them. Without `--yes`, each one's lines, a blank line between two, and
/// nothing written. Those their `settings.toml` marks already are not
/// listed, there being nothing to say of them.
pub(crate) fn migrate_all(
    workspace: &WeaverWorkspace,
    asked: &Asked,
    homes: &AgentHomes,
) -> Result<Outcome, Failure> {
    let mut outcome = Outcome::plain(String::new());
    let mut laid = false;
    for member in workspace.members() {
        let project = match member.project {
            Ok(project) => project,
            Err(error) => {
                outcome
                    .stderr
                    .push(Warning::coded(error.code(), format!("warning: {error}")));
                outcome.ok = false;
                continue;
            }
        };
        match laid_out(Some(workspace), &project, asked, homes) {
            Ok(None) => {}
            Ok(Some(one)) => {
                if laid {
                    outcome.stdout.push('\n');
                }
                laid = true;
                outcome.stdout.push_str(&one.stdout);
                outcome.stderr.extend(one.stderr);
                outcome.ok &= one.ok;
            }
            Err(stopped) => {
                outcome.stderr.extend(stopped.lines(&project));
                outcome.ok = false;
            }
        }
    }
    if laid && !asked.yes {
        outcome.stdout.push_str(UNCHANGED);
    } else if !laid && outcome.ok {
        outcome.stdout = String::from(
            "nothing to migrate: every project the workspace links is marked by its settings.toml \
             already\n",
        );
    }
    Ok(outcome)
}

/// What a dry run of a project's move ends with.
const UNCHANGED: &str = "nothing changed: add --yes to go ahead\n";

/// One project's move as a single command says it: [`laid_out`], ended as a
/// dry run is, or what a project with nothing to move says.
fn finished(outcome: Option<Outcome>, project: &MindProject, asked: &Asked) -> Outcome {
    match outcome {
        Some(mut outcome) => {
            if !asked.yes {
                outcome.stdout.push_str(UNCHANGED);
            }
            outcome
        }
        None => Outcome::plain(format!(
            "nothing to migrate: {} is marked by its {} already\n",
            project.name(),
            MarkerFile::SettingsToml
        )),
    }
}

/// Why one project's move did not happen: refused before anything was
/// written, or stopped by a failure, which may have put back less than all.
enum Stopped {
    Refused(Vec<migrate::Refusal>),
    Failed(Failure),
}

impl Stopped {
    /// As a command given one project fails: every refusal at once, the
    /// first as the error, as `mindweaver migrate --library` says them.
    fn failure(self) -> Failure {
        match self {
            // Never without a reason, so the second is not reached.
            Stopped::Refused(refusals) => refused(refusals)
                .err()
                .unwrap_or_else(|| CliError::Migrate(MigrateError::Refused(Vec::new())).into()),
            Stopped::Failed(failure) => failure,
        }
    }

    /// As lines beside what other projects did, each error naming the
    /// project it is about.
    fn lines(self, project: &MindProject) -> Vec<Warning> {
        let at = project.root().display();
        match self {
            Stopped::Refused(refusals) => refusals
                .into_iter()
                .map(|refusal| {
                    let refused = CliError::Refused(refusal);
                    Warning::coded(refused.code(), format!("error: {at}: {refused}"))
                })
                .collect(),
            Stopped::Failed(failure) => {
                let mut lines = failure.warnings;
                lines.push(Warning::coded(
                    failure.error.code(),
                    format!("error: {at}: {}", failure.error),
                ));
                lines
            }
        }
    }
}

/// One project's move, planned without `yes` and made with it, laid out;
/// `None` when its `settings.toml` marks it already.
///
/// The dry run is a report, as `mindweaver migrate --library`'s is: a line for
/// each thing that would change, to read before saying yes. Made, a copy
/// left out of a folder an agent the project names reads is a warning, as
/// it is for `mind agents`.
fn laid_out(
    workspace: Option<&WeaverWorkspace>,
    project: &MindProject,
    asked: &Asked,
    homes: &AgentHomes,
) -> Result<Option<Outcome>, Stopped> {
    let failed = |error: MigrateError| Stopped::Failed(CliError::Migrate(error).into());
    if !asked.yes {
        let Some(mut plan) =
            migrate::project_plan(workspace, project, asked.agents, asked.share, homes)
                .map_err(failed)?
        else {
            return Ok(None);
        };
        if !plan.refusals.is_empty() {
            return Err(Stopped::Refused(std::mem::take(&mut plan.refusals)));
        }
        let mut outcome = Outcome::plain(project_report(project, &plan, false));
        outcome.stderr.extend(beside(project, &plan));
        return Ok(Some(outcome));
    }

    let migrated = match migrate::project(workspace, project, asked.agents, asked.share, homes) {
        Ok(Some(migrated)) => migrated,
        Ok(None) => return Ok(None),
        Err(MigrateError::Refused(refusals)) => return Err(Stopped::Refused(refusals)),
        Err(error) => {
            // Said with the error, being what the move left that it could
            // not put back.
            let warnings = error.left_behind().iter().map(stranded).collect();
            return Err(Stopped::Failed(Failure {
                error: Box::new(CliError::Migrate(error)),
                warnings,
            }));
        }
    };
    let said = scrubbed(&migrated.scrub);
    let plan = &migrated.plan;
    let mut outcome = Outcome::plain(project_report(project, plan, true));
    for line in cleared(project, &migrated.swept) {
        let _ = writeln!(outcome.stdout, "  {line}");
    }
    for retarget in &plan.installations {
        let warnings = not_copied(project, retarget);
        outcome.ok &= warnings.is_empty();
        outcome.stderr.extend(warnings);
    }
    outcome.stderr.extend(beside(project, plan));
    Ok(Some(said.before(outcome)))
}

/// A line for each thing a project's move does, as what would happen in a
/// dry run, and as what did once it is made: the settings and the marker
/// first, then what keeps `.mind` out of git, each installation's copies,
/// what is left where it is, and what is kept out of git.
fn project_report(project: &MindProject, plan: &ProjectPlan, done: bool) -> String {
    let would = |now: &'static str, then: &'static str| if done { now } else { then };
    let mut text = String::new();
    let _ = writeln!(
        text,
        "{} {} for {}: agents {}, share {}",
        would("wrote", "would write"),
        shown(project, &plan.settings_file),
        project.name(),
        agents(&plan.settings.agents),
        plan.settings.share
    );
    let _ = writeln!(
        text,
        "{} {} to {}, as it {}",
        would("moved", "would move"),
        shown(project, &plan.marker),
        shown(project, &plan.backup),
        would("was", "is")
    );
    if let Some(path) = &plan.ignore {
        let _ = writeln!(
            text,
            "{} {}, keeping .mind out of git",
            would("wrote", "would write"),
            shown(project, path)
        );
    }
    let local = plan.settings.share == Share::Local;
    for retarget in &plan.installations {
        moved(&mut text, project, retarget, done, local);
    }
    for path in &plan.authored {
        let _ = writeln!(
            text,
            "{} {} where it is: the project's own",
            would("left", "would leave"),
            shown(project, path)
        );
    }
    for path in &plan.foreign {
        let _ = writeln!(
            text,
            "{} {} where it is: Mindweaver did not install it",
            would("left", "would leave"),
            shown(project, path)
        );
    }
    for path in &plan.excluded {
        let _ = writeln!(
            text,
            "{} {} out of git",
            would("kept", "would keep"),
            shown(project, path)
        );
    }
    text
}

/// What a project's move says on stderr beside its lines: that nothing
/// keeps a local project's copies out of git outside a work tree, and how
/// to stop git tracking a `.mind` that a local project's `.gitignore` keeps
/// none of out, which no ignore rule does and Mindweaver runs no git to do.
fn beside(project: &MindProject, plan: &ProjectPlan) -> Vec<Warning> {
    let mut said: Vec<Warning> = outside_note(project, plan.outside).into_iter().collect();
    if plan.tracked {
        said.push(
            format!(
                "hint: git tracks {}, and a .gitignore keeps out only what git does not track; \
                 `git rm -r --cached .mind` in {} stops that, and leaves the files where they \
                 are",
                project.mind_dir().display(),
                project.root().display()
            )
            .into(),
        );
    }
    said
}

/// What a project's move that failed says of something it could not put
/// back.
fn stranded(left: &Stranded) -> Warning {
    let said = match left {
        Stranded::Copy(path) => format!(
            "warning: {} was copied and could not be taken back out; remove it yourself",
            path.display()
        ),
        Stranded::Retired { name, path } => format!(
            "warning: {} was moved out of the way and could not be put back; `mind install \
             {name}` puts it back",
            path.display()
        ),
        Stranded::Settings { path, marker } => format!(
            "warning: {} was written and could not be removed, so the project is read from it; \
             remove it to read the project from its {marker} again",
            path.display()
        ),
        Stranded::Marker { marker, backup } => format!(
            "warning: {} could not be moved back to {}; move it yourself",
            backup.display(),
            marker.display()
        ),
        Stranded::Ignore(path) => format!(
            "warning: {} was written and could not be removed, so .mind is kept out of git; \
             remove it yourself",
            path.display()
        ),
    };
    said.into()
}
