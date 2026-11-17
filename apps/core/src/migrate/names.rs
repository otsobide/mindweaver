//! Moving a workspace a Mindflayer build made to Mindweaver's names (D-26),
//! when somebody asks for it: `mindweaver migrate --names`.
//!
//! Such a workspace is read as it was written, and written under its own
//! names, as every format is ([`crate::names`]): nothing moves it but this.
//! [`names_plan`] says what moving it would do, and [`names`] does it: its
//! `.mindflayer` renamed `.mindweaver`, and in it `flayer.toml` to
//! `weaver.toml`, `mindflayer.db` to `mindweaver.db` and every copy kept of
//! either beside it; each folder its marker names inside the old folder
//! pointed at the same place in the new one, and each gathered artifact the
//! ledger files there with it. In each project it links, the markers of
//! the block of `AGENTS.md` and of the import in `CLAUDE.md` are spelled the
//! new way, the block of the exclude file git reads for it too, and each of
//! Claude's rule files the ledger records in `.claude/rules/mindflayer` is
//! carried to `.claude/rules/mindweaver`, its record and its exclude line
//! with it; one that is not there has its record and its line pointed
//! there all the same, so the update that puts it back puts it under the
//! new name.
//!
//! Only what Mindweaver wrote moves, and only as it wrote it: a rule file
//! changed since, or something else in its place, is left where it is, its
//! record naming it there, which a render finds; one nothing records is
//! never looked at. A name already taken where something would go refuses
//! the whole move, and so does a block that cannot be read, a project whose
//! marker cannot, and a ledger another process has open, whose log a rename
//! would leave behind. The ledger is renamed, never migrated: it is the
//! same schema under another name.
//!
//! A workspace under Mindweaver's names is planned too, its projects alone,
//! so a second move takes what the first could not: a project it did not
//! find, a rule file put back as it was written.
//!
//! Git is never run. A project's rule files git tracks are moved from under
//! it all the same, as the folder they are in is Mindweaver's, and the plan
//! says so for a front end to say what to commit.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use super::{MigrateError, Refusal};
use crate::agents_md::{self, AgentsMd, AgentsMdError};
use crate::claude_md::{ClaudeMd, ClaudeMdError, CLAUDE_MD};
use crate::claude_rules;
use crate::code::codes;
use crate::exclude::{self, Entry, ExcludeError, ExcludeFile};
use crate::hash;
use crate::install;
use crate::install::rules::Withheld;
use crate::kind::Kind;
use crate::ledger::{Ledger, LedgerError, Target, FLAYER_LEDGER_FILE, LEDGER_FILE};
use crate::names::Names;
use crate::paths;
use crate::workspace::{
    replace_bytes, MindProject, WeaverWorkspace, WorkspaceError, FLAYER_CONFIG, FLAYER_DIR,
    WEAVER_CONFIG, WEAVER_DIR,
};

/// What moving a workspace to Mindweaver's names would do, and what would
/// stop it: [`names_plan`], or what [`names`] did.
#[derive(Debug, Default)]
pub struct NamesPlan {
    /// The workspace's folder and where it goes: `None` for a workspace
    /// under Mindweaver's names already, whose folder, marker and ledger
    /// stay as they are, only its projects planned.
    pub folder: Option<Renamed>,
    /// Each file the folder holds that is named after the product, and
    /// where it is once the folder has moved: the marker, each copy of it a
    /// migration kept, the ledger and each copy of it a schema step kept.
    pub files: Vec<Renamed>,
    /// Each folder the marker names inside the old folder, and the same
    /// place in the new one, its line in the marker pointed there.
    pub directories: Vec<Repointed>,
    /// How many gathered artifacts the ledger files inside the old folder,
    /// whose paths move with it.
    pub shelf: usize,
    /// What moves in each project the workspace links, in its order.
    pub projects: Vec<ProjectNames>,
    /// Each exclude file whose block's markers are spelled the new way,
    /// once, whichever projects' groups it holds: a file has one block.
    pub excludes: Vec<PathBuf>,
    /// Why it cannot go ahead. Nothing is moved while there is any.
    pub refusals: Vec<Refusal>,
}

impl NamesPlan {
    /// Whether moving changes nothing at all: the workspace is under
    /// Mindweaver's names already, and nothing in its projects is left
    /// under Mindflayer's that the move would change. What it leaves as it
    /// is ([`ProjectNames::kept`], [`ProjectNames::left`]) is no change.
    pub fn is_empty(&self) -> bool {
        self.folder.is_none()
            && self.excludes.is_empty()
            && self.projects.iter().all(|project| {
                project.blocks.is_empty() && project.rules.is_empty() && project.absent.is_empty()
            })
    }
}

/// Something renamed: where it is, and where it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renamed {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// A line of the marker's `directories`, by its key, pointed from inside
/// the old folder to the same place in the new one, each a route from the
/// workspace's root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repointed {
    pub key: String,
    pub from: PathBuf,
    pub to: PathBuf,
}

/// What moves in one project the workspace links.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectNames {
    pub root: PathBuf,
    /// Its `AGENTS.md`, `CLAUDE.md` or `.claude/CLAUDE.md` whose block's
    /// markers are spelled the new way.
    pub blocks: Vec<PathBuf>,
    /// Each of those left under Mindflayer's markers, which every reader
    /// takes, because the project may not edit it: git tracks it in a local
    /// project without `edit_tracked` (D-02), as a render leaves the block
    /// or the import there too, a team's file being nobody's to change for
    /// a name.
    pub kept: Vec<KeptBlock>,
    /// Each of Claude's rule files carried to `.claude/rules/mindweaver`,
    /// its record with it.
    pub rules: Vec<CarriedRule>,
    /// Each one the ledger records in `.claude/rules/mindflayer` that is not
    /// there: nothing is moved, and its record and its exclude line are
    /// pointed at `.claude/rules/mindweaver`, as a carried one's are, so
    /// the update that puts it back puts it there. Left pointing at the old
    /// folder, it would be written there again under a workspace moved to
    /// the new names.
    pub absent: Vec<CarriedRule>,
    /// Each one left in `.claude/rules/mindflayer`, its record naming it
    /// there, which a render finds as it does any recorded file.
    pub left: Vec<LeftRule>,
}

/// A file whose block's markers are left as a Mindflayer build spelled them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptBlock {
    pub path: PathBuf,
    pub why: Withheld,
}

/// One of Claude's rule files carried to the new folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarriedRule {
    pub route: String,
    pub from: PathBuf,
    pub to: PathBuf,
    /// Whether git tracks the file where it was, which the move takes out
    /// from under it: for a front end to say what to commit, which is not
    /// Mindweaver's to do. Told of each file rather than of the folder,
    /// which may hold one the move leaves there, still tracked.
    pub tracked: bool,
}

/// One of Claude's rule files left where a Mindflayer build filed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftRule {
    pub route: String,
    pub path: PathBuf,
    pub why: Left,
}

/// Why a rule file is left where it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Left {
    /// It holds something else than what was written: somebody's edit,
    /// which a move would carry as Mindweaver's.
    Edited,
    /// Something other than a file it can read is in its place, a link or
    /// a folder: nothing a file of Mindweaver's would be, so somebody's,
    /// which a render leaves as it is too.
    Unlike,
}

/// What [`names`] did.
#[derive(Debug)]
pub struct NamesMigrated {
    /// What was planned under the lock, and so done.
    pub plan: NamesPlan,
}

/// A step of a move to Mindweaver's names that failed: every step before it
/// is put back, but for what [`MigrateError::left_behind_names`] names.
#[derive(Debug, Error)]
pub enum NamesFailure {
    #[error("{from}: cannot be moved to {to}: {source}")]
    Rename {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: cannot be written: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// Another process opened the ledger while it was being moved, and
    /// keeps its log beside it, which a rename would leave behind.
    #[error(
        "{path}: opened by another process while the workspace was being moved, so it is not \
         renamed; close what has it open and migrate again"
    )]
    LedgerOpened { path: PathBuf },
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Marker(#[from] WorkspaceError),
    #[error(transparent)]
    Exclude(#[from] ExcludeError),
}

codes!(NamesFailure {
    Self::Rename { .. } => "migrate_rename_failed",
    Self::Write { .. } => "migrate_names_write_failed",
    Self::LedgerOpened { .. } => "migrate_ledger_opened",
} carried {
    Self::Ledger(error) => error,
    Self::Marker(error) => error,
    Self::Exclude(error) => error,
});

/// What moving `workspace` to Mindweaver's names would do, read from what
/// is on disk and changing none of it, the ledger read without being made.
///
/// Nothing for a workspace not made yet. For one under Mindweaver's names
/// already, only what its projects still hold under Mindflayer's: what an
/// earlier move left, a project it could not open then, a rule file
/// changed since and put back as it was, which a second move takes to the
/// new names. Every refusal at once, so a dry run says what the real one
/// will.
pub fn names_plan(workspace: &WeaverWorkspace) -> Result<NamesPlan, MigrateError> {
    let mut plan = NamesPlan::default();
    if !workspace.is_made() {
        return Ok(plan);
    }
    // Read first, as any read of it is: a ledger an older schema left is
    // brought forward here, its copy kept beside it, which is then among
    // what is renamed.
    let ledger = workspace.ledger_for_reading()?;
    if workspace.names() == Names::Mindflayer {
        plan_folder(workspace, &ledger, &mut plan)?;
    }
    let mut excludes: Vec<PathBuf> = Vec::new();
    for member in workspace.members() {
        let project = match member.project {
            Ok(project) => project,
            // An entry that leads to nothing holds nothing to move.
            Err(WorkspaceError::NotAProject { .. }) => continue,
            Err(error) => {
                plan.refusals.push(Refusal::Unopened(error));
                continue;
            }
        };
        let names = project_names(workspace, &ledger, &project, &mut plan.refusals)?;
        match exclude_block(&project) {
            Ok(Some(file)) if !excludes.contains(&file) => excludes.push(file),
            Ok(_) => {}
            Err(error) => plan.refusals.push(Refusal::Excluded(error)),
        }
        plan.projects.push(names);
    }
    plan.excludes = excludes;
    // Closed first, so what is left beside it is another's. Only a ledger
    // that is renamed is in the way of a process holding it open.
    drop(ledger);
    if plan.folder.is_some() {
        if let Some(path) = ledger_in_use(workspace) {
            plan.refusals.push(Refusal::LedgerOpen { path });
        }
    }
    Ok(plan)
}

/// What moves of `workspace`'s own folder, a Mindflayer build's, into
/// `plan`: the folder, the files in it named after the product, the
/// marker's folders inside it and the shelf the ledger files there.
fn plan_folder(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    plan: &mut NamesPlan,
) -> Result<(), MigrateError> {
    let old = workspace.weaver_dir();
    let new = workspace.root().join(WEAVER_DIR);
    if is_there(&new) {
        plan.refusals.push(Refusal::Taken {
            from: old.clone(),
            to: new.clone(),
        });
    }
    plan.folder = Some(Renamed {
        from: old.clone(),
        to: new.clone(),
    });
    for (from, to) in renamed_inside(&old)? {
        if is_there(&old.join(&to)) {
            plan.refusals.push(Refusal::Taken {
                from: old.join(&from),
                to: old.join(&to),
            });
        }
        plan.files.push(Renamed {
            from: old.join(from),
            to: new.join(to),
        });
    }
    for (key, directory) in workspace.config().directories.entries() {
        if let Some(to) = moved_route(directory) {
            plan.directories.push(Repointed {
                key: key.to_owned(),
                from: directory.to_path_buf(),
                to,
            });
        }
    }

    plan.shelf = ledger
        .gathered()?
        .iter()
        .filter(|gathered| gathered.path.starts_with(&shelf_prefix(Names::Mindflayer)))
        .count();
    Ok(())
}

/// Move `workspace` to Mindweaver's names, as [`names_plan`] says.
///
/// Planned again once the workspace's lock is held, from the disk rather
/// than `workspace`, so what is moved is what is there, and refused as a
/// whole while the plan has any [`Refusal`]; a no-op, writing nothing, for
/// a workspace under Mindweaver's names already whose projects hold
/// nothing more to move, which is what makes a second run harmless, and
/// one that finishes what the first could not.
///
/// Each project first, then the ledger's records, then the marker, the files
/// in the folder and the folder itself, each step written whole: a file
/// replaced through a temporary and a rename, a rule file renamed into its
/// new folder. A step that fails puts back every step taken before it, the
/// last first, so the workspace and its projects read as they did; what
/// could not be put back is named ([`MigrateError::left_behind_names`]).
/// The lock is held throughout and follows its file, so no writer gets in
/// between: the folder is renamed whole, and on Windows, which renames no
/// folder holding a file open, made again and given each of its entries
/// ([`entry_by_entry`]).
pub fn names(workspace: &mut WeaverWorkspace) -> Result<NamesMigrated, MigrateError> {
    if !workspace.is_made() {
        return Ok(NamesMigrated {
            plan: NamesPlan::default(),
        });
    }
    let mut lock = workspace.lock().map_err(WorkspaceError::from)?;
    let mut current = WeaverWorkspace::open(workspace.root())?;
    let plan = names_plan(&current)?;
    if !plan.refusals.is_empty() {
        return Err(MigrateError::Refused(plan.refusals));
    }
    if plan.is_empty() {
        *workspace = current;
        return Ok(NamesMigrated { plan });
    }
    let mut done = Done::default();
    if let Err(failure) = carry_out(&mut current, &plan, &mut done) {
        let stranded = done.put_back(&current);
        return Err(MigrateError::Names {
            failure: Box::new(failure),
            stranded,
        });
    }
    if plan.folder.is_some() {
        lock.moved(&current.root().join(WEAVER_DIR));
    }
    *workspace = WeaverWorkspace::open(current.root())?;
    Ok(NamesMigrated { plan })
}

/// Every step of the move, in turn, each noted in `done` once it is taken.
fn carry_out(
    workspace: &mut WeaverWorkspace,
    plan: &NamesPlan,
    done: &mut Done,
) -> Result<(), NamesFailure> {
    // What the ledger records of each project, read before anything moves;
    // none when there is no ledger, which is not made for the occasion: a
    // file made now would not be among those the plan renames.
    let ledger = match Ledger::open_existing(workspace.ledger_path())? {
        Some(ledger) => ledger,
        None => Ledger::in_memory()?,
    };
    let mut retargets: Vec<(String, String, Vec<Target>, Vec<Target>)> = Vec::new();
    let mut lines: BTreeMap<PathBuf, Vec<(String, String, String)>> = BTreeMap::new();
    for names in &plan.projects {
        let Ok(project) = MindProject::open(&names.root) else {
            continue;
        };
        let entry = install::entry_for_root(workspace, &names.root);
        let key = install::local(&project).map(str::to_owned);
        let location = exclude::locate(&names.root)?;
        for record in ledger.records(&entry, Kind::Rule)? {
            let carried: Vec<&CarriedRule> = names
                .rules
                .iter()
                .chain(&names.absent)
                .filter(|carried| carried.route == record.name.as_str())
                .collect();
            if carried.is_empty() {
                continue;
            }
            let before = record.targets.clone();
            let after: Vec<Target> = before
                .iter()
                .map(|target| {
                    let mut target = target.clone();
                    if install::rules::claude_file_names(&target.path, &record.name)
                        == Some(Names::Mindflayer)
                    {
                        target.path =
                            install::rules::claude_file_under(Names::Mindweaver, &record.name);
                    }
                    target
                })
                .collect();
            retargets.push((entry.clone(), record.name.to_string(), before, after));
            if let (Some(key), Some(location)) = (&key, &location) {
                let pattern = |names: Names| {
                    Entry::file(claude_rules::file_under(names, &record.name))
                        .pattern(&location.route)
                };
                lines.entry(location.file.clone()).or_default().push((
                    key.clone(),
                    pattern(Names::Mindflayer)?,
                    pattern(Names::Mindweaver)?,
                ));
            }
        }
    }

    // Each project's own files: Claude's rule files, then the blocks.
    for names in &plan.projects {
        for carried in &names.rules {
            carry_rule(carried, done)?;
        }
        let top = names
            .root
            .join(claude_rules::folder_under(Names::Mindflayer));
        for carried in &names.rules {
            prune(&top, carried.from.parent());
        }
        // A folder an absent file left empty goes too, as a render's does,
        // so the old name is not left on the disk with nothing in it.
        for absent in &names.absent {
            let Some(folder) = absent.from.parent().filter(|folder| folder.is_dir()) else {
                continue;
            };
            prune(&top, Some(folder));
            if !is_there(folder) {
                done.0.push(Step::Pruned {
                    folder: folder.to_path_buf(),
                });
            }
        }
        for file in &names.blocks {
            respell_block(&names.root, file, done)?;
        }
    }
    let mut files: Vec<&PathBuf> = plan.excludes.iter().collect();
    files.extend(lines.keys().filter(|file| !plan.excludes.contains(file)));
    for file in files {
        respell_exclude(file, lines.get(file).map_or(&[][..], Vec::as_slice), done)?;
    }

    // The ledger's records, before it is closed and renamed.
    for (entry, name, before, after) in &retargets {
        ledger.retargeted(entry, Kind::Rule, name, after)?;
        done.0.push(Step::Retargeted {
            entry: entry.clone(),
            name: name.clone(),
            targets: before.clone(),
        });
    }
    if plan.shelf > 0 {
        ledger.shelf_moved(
            &shelf_prefix(Names::Mindflayer),
            &shelf_prefix(Names::Mindweaver),
        )?;
        done.0.push(Step::Shelf);
    }
    drop(ledger);
    if plan.folder.is_some() {
        if let Some(path) = ledger_in_use(workspace) {
            return Err(NamesFailure::LedgerOpened { path });
        }
    }

    // The marker, the files beside it, and the folder.
    if !plan.directories.is_empty() {
        let path = workspace.config_path();
        let before = fs::read(&path).map_err(|source| NamesFailure::Write {
            path: path.clone(),
            source,
        })?;
        workspace.edit_marker_held(|document| repoint(document, &plan.directories))?;
        done.0.push(Step::Written { path, before });
    }
    let old = workspace.weaver_dir();
    for renamed in &plan.files {
        let to = old.join(renamed.to.file_name().unwrap_or_default());
        rename(&renamed.from, &to)?;
        done.0.push(Step::Renamed {
            from: renamed.from.clone(),
            to,
        });
    }
    if let Some(folder) = &plan.folder {
        if cfg!(windows) {
            entry_by_entry(&folder.from, &folder.to, done)?;
        } else {
            rename(&folder.from, &folder.to)?;
            done.0.push(Step::Renamed {
                from: folder.from.clone(),
                to: folder.to.clone(),
            });
        }
    }
    Ok(())
}

/// Move the folder `from` to `to`, which is not there yet, as Windows lets
/// one holding a file open be moved: `to` made, every entry of `from`
/// renamed into it in turn, and `from` taken away once empty, each a step
/// put back on a failure. Windows renames no folder holding a file open,
/// and this one holds the workspace's lock, which the move keeps
/// throughout; a file opened letting others delete it, as `std` opens the
/// lock, is renamed, and its lock goes with it. A rename of the folder
/// whole elsewhere, so a reader there sees the workspace under one name or
/// the other, never between: a reader on Windows may find it half moved
/// for as long as the renames take (docs/architecture.md, open questions).
fn entry_by_entry(from: &Path, to: &Path, done: &mut Done) -> Result<(), NamesFailure> {
    let failed = |source| NamesFailure::Rename {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        source,
    };
    fs::create_dir(to).map_err(failed)?;
    done.0.push(Step::Made {
        folder: to.to_path_buf(),
    });
    let mut names = Vec::new();
    for entry in fs::read_dir(from).map_err(failed)? {
        names.push(entry.map_err(failed)?.file_name());
    }
    names.sort();
    for name in names {
        let (from, to) = (from.join(&name), to.join(&name));
        rename(&from, &to)?;
        done.0.push(Step::Renamed { from, to });
    }
    fs::remove_dir(from).map_err(failed)?;
    done.0.push(Step::Pruned {
        folder: from.to_path_buf(),
    });
    Ok(())
}

/// Every step taken, in the order taken, to be put back in the other.
#[derive(Default)]
struct Done(Vec<Step>);

enum Step {
    /// A file or a folder renamed.
    Renamed { from: PathBuf, to: PathBuf },
    /// A folder made, taken away empty to put back.
    Made { folder: PathBuf },
    /// A file written over, and what it held.
    Written { path: PathBuf, before: Vec<u8> },
    /// A rule file carried from `from` to `to`, folders made on the way.
    Carried {
        from: PathBuf,
        to: PathBuf,
        top: PathBuf,
    },
    /// Empty folders taken away from `folder` up, made again to put back.
    Pruned { folder: PathBuf },
    /// An installation's targets, as they were.
    Retargeted {
        entry: String,
        name: String,
        targets: Vec<Target>,
    },
    /// The shelf's paths moved in the ledger.
    Shelf,
}

impl Done {
    /// Put every step back, the last first, and name each thing that could
    /// not be: the ledger is opened again where it was, under the old name,
    /// once the folder and the files in it are put back.
    fn put_back(self, workspace: &WeaverWorkspace) -> Vec<PathBuf> {
        let mut stranded = Vec::new();
        // Where it was before the move, the folder and the files in it put
        // back before any record is.
        let ledger_path = workspace.ledger_path();
        let mut ledger: Option<Ledger> = None;
        for step in self.0.into_iter().rev() {
            match step {
                Step::Renamed { from, to } => {
                    if fs::rename(&to, &from).is_err() {
                        stranded.push(to);
                    }
                }
                Step::Made { folder } => {
                    if fs::remove_dir(&folder).is_err() {
                        stranded.push(folder);
                    }
                }
                Step::Written { path, before } => {
                    if replace_bytes(&path, &before).is_err() {
                        stranded.push(path);
                    }
                }
                Step::Carried { from, to, top } => {
                    let back = from
                        .parent()
                        .map_or(Ok(()), fs::create_dir_all)
                        .and_then(|()| fs::rename(&to, &from));
                    if back.is_err() {
                        stranded.push(to);
                    } else {
                        prune(&top, to.parent());
                    }
                }
                Step::Pruned { folder } => {
                    if fs::create_dir_all(&folder).is_err() {
                        stranded.push(folder);
                    }
                }
                Step::Retargeted {
                    entry,
                    name,
                    targets,
                } => {
                    let back = opened(&mut ledger, &ledger_path).and_then(|ledger| {
                        ledger.retargeted(&entry, Kind::Rule, &name, &targets).ok()
                    });
                    if back.is_none() {
                        stranded.push(ledger_path.clone());
                    }
                }
                Step::Shelf => {
                    let back = opened(&mut ledger, &ledger_path).and_then(|ledger| {
                        ledger
                            .shelf_moved(
                                &shelf_prefix(Names::Mindweaver),
                                &shelf_prefix(Names::Mindflayer),
                            )
                            .ok()
                    });
                    if back.is_none() {
                        stranded.push(ledger_path.clone());
                    }
                }
            }
        }
        stranded.dedup();
        stranded
    }
}

/// The ledger at `path`, opened once for every step that puts a record
/// back.
fn opened<'a>(ledger: &'a mut Option<Ledger>, path: &Path) -> Option<&'a Ledger> {
    if ledger.is_none() {
        *ledger = Ledger::open_existing(path).ok().flatten();
    }
    ledger.as_ref()
}

/// The files `old`, a workspace's `.mindflayer`, holds that are named after
/// the product, each with the name it takes: its marker, the copies of the
/// marker a migration kept (`flayer.toml.v<N>`), its ledger and the copies
/// a schema step kept (`mindflayer.db.v<N>.bak`). Nothing else: a
/// temporary somebody's interrupted run left is left as it is, and moves
/// with the folder.
fn renamed_inside(old: &Path) -> Result<Vec<(String, String)>, MigrateError> {
    let entries = fs::read_dir(old).map_err(|source| {
        MigrateError::Workspace(WorkspaceError::Read {
            path: old.to_path_buf(),
            source,
        })
    })?;
    let mut renamed = Vec::new();
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let to = if name == FLAYER_CONFIG {
            Some(String::from(WEAVER_CONFIG))
        } else if let Some(version) = name
            .strip_prefix(FLAYER_CONFIG)
            .and_then(|rest| rest.strip_prefix(".v"))
            .filter(|version| is_version(version))
        {
            Some(format!("{WEAVER_CONFIG}.v{version}"))
        } else if name == FLAYER_LEDGER_FILE {
            Some(String::from(LEDGER_FILE))
        } else {
            name.strip_prefix(FLAYER_LEDGER_FILE)
                .and_then(|rest| rest.strip_prefix(".v"))
                .and_then(|rest| rest.strip_suffix(".bak"))
                .filter(|version| is_version(version))
                .map(|version| format!("{LEDGER_FILE}.v{version}.bak"))
        };
        if let Some(to) = to {
            renamed.push((name, to));
        }
    }
    renamed.sort();
    Ok(renamed)
}

/// Whether `text` is a version's number, as a copy is named after.
fn is_version(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// `route`, a folder the marker names, moved with the folder when it is
/// inside `.mindflayer`: the same place inside `.mindweaver`.
///
/// Normalized first, as a marker's folder is when it is judged to stay
/// inside the workspace, so one written as it was typed
/// (`./.mindflayer/library/skills`) moves as the folder it names, rather
/// than being left naming one the move takes away.
fn moved_route(route: &Path) -> Option<PathBuf> {
    let route = paths::normalize(route);
    let mut components = route.components();
    match components.next() {
        Some(Component::Normal(first)) if first == FLAYER_DIR => {
            Some(Path::new(WEAVER_DIR).join(components.as_path()))
        }
        _ => None,
    }
}

/// What the ledger's paths of the shelf start with under `names`.
fn shelf_prefix(names: Names) -> String {
    format!("{}/", names.workspace_dir())
}

/// What moves in `project`, and each reason it cannot, into `refusals`.
fn project_names(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    refusals: &mut Vec<Refusal>,
) -> Result<ProjectNames, MigrateError> {
    let root = project.root().to_path_buf();
    let mut names = ProjectNames {
        root: root.clone(),
        ..ProjectNames::default()
    };

    let agents = root.join(agents_md::FILE);
    match fs::read(&agents) {
        Ok(bytes) => match AgentsMd::parse(&bytes) {
            Ok(file) if file.names() == Some(Names::Mindflayer) => {
                match agents_md::linked_inside(&root, &agents) {
                    Ok(()) => names.block(project, agents, Path::new(agents_md::FILE)),
                    Err(error) => refusals.push(Refusal::AgentsMd(error)),
                }
            }
            Ok(_) => {}
            Err(problem) => refusals.push(Refusal::AgentsMd(AgentsMdError::Malformed {
                file: agents,
                problem,
            })),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(source) => refusals.push(Refusal::AgentsMd(AgentsMdError::Read {
            path: agents,
            source,
        })),
    }
    // Only a file in its place: the import is never written through a
    // link, so none an older build wrote is behind one. Nor one that is the
    // project's `AGENTS.md`, which a link from it leads to, as Claude then
    // reads the block itself there (`Reach::Same`): its markers are spelled
    // the new way through the `AGENTS.md`, and read as a `CLAUDE.md` it is
    // a block of rules, which no import ever is.
    let agents_real = crate::paths::canonical(root.join(agents_md::FILE)).ok();
    for route in [
        PathBuf::from(CLAUDE_MD),
        Path::new(".claude").join(CLAUDE_MD),
    ] {
        let claude = root.join(&route);
        if !fs::symlink_metadata(&claude).is_ok_and(|found| found.is_file()) {
            continue;
        }
        if agents_real.is_some() && crate::paths::canonical(&claude).ok() == agents_real {
            continue;
        }
        match fs::read(&claude).map(|bytes| ClaudeMd::parse(&bytes)) {
            Ok(Ok(file)) if file.names() == Some(Names::Mindflayer) => {
                names.block(project, claude, &route);
            }
            Ok(Ok(_)) => {}
            Ok(Err(problem)) => refusals.push(Refusal::ClaudeMd(ClaudeMdError::Malformed {
                file: claude,
                problem,
            })),
            Err(source) => refusals.push(Refusal::ClaudeMd(ClaudeMdError::Read {
                path: claude,
                source,
            })),
        }
    }

    let entry = install::entry_for_root(workspace, &root);
    for record in ledger.records(&entry, Kind::Rule)? {
        for target in &record.targets {
            if install::rules::claude_file_names(&target.path, &record.name)
                != Some(Names::Mindflayer)
            {
                continue;
            }
            let from = paths::joined(&root, &target.path);
            let to = root.join(claude_rules::file_under(Names::Mindweaver, &record.name));
            let found = fs::symlink_metadata(&from).ok();
            let held = found
                .as_ref()
                .filter(|found| found.is_file())
                .and_then(|_| fs::read(&from).ok());
            let why = match (&found, &held, &target.content_hash) {
                (_, Some(bytes), Some(written)) if hash::bytes(bytes) == *written => None,
                (_, Some(_), _) => Some(Left::Edited),
                (Some(_), None, _) => Some(Left::Unlike),
                (None, None, _) => None,
            };
            if let Some(why) = why {
                names.left.push(LeftRule {
                    route: record.name.to_string(),
                    path: from,
                    why,
                });
                continue;
            }
            if is_there(&to) {
                refusals.push(Refusal::Taken {
                    from: from.clone(),
                    to: to.clone(),
                });
            }
            let mut carried = CarriedRule {
                route: record.name.to_string(),
                from,
                to,
                tracked: false,
            };
            if found.is_some() {
                carried.tracked =
                    exclude::tracks_file(&root, Path::new(&target.path)).unwrap_or(false);
                names.rules.push(carried);
            } else {
                names.absent.push(carried);
            }
        }
    }
    Ok(names)
}

impl ProjectNames {
    /// `file`, at `route` from `project`'s root, among the blocks spelled
    /// the new way, or kept as it is when the project may not edit it.
    fn block(&mut self, project: &MindProject, file: PathBuf, route: &Path) {
        match install::rules::withheld_from(project, route) {
            None => self.blocks.push(file),
            Some(why) => self.kept.push(KeptBlock { path: file, why }),
        }
    }
}

/// The exclude file git reads for `project`, when its block is spelled
/// under Mindflayer's markers: refused when it cannot be read as a block.
fn exclude_block(project: &MindProject) -> Result<Option<PathBuf>, ExcludeError> {
    let Some(location) = exclude::locate(project.root())? else {
        return Ok(None);
    };
    let bytes = match fs::read(&location.file) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ExcludeError::Read {
                path: location.file,
                source,
            })
        }
    };
    let file = ExcludeFile::parse(&bytes).map_err(|problem| ExcludeError::Malformed {
        file: location.file.clone(),
        problem,
    })?;
    Ok((file.names() == Some(Names::Mindflayer)).then_some(location.file))
}

/// The ledger's log beside it, which a process holding it open keeps, or
/// one left by a process that stopped: what a rename of the ledger alone
/// would leave behind, holding writes the file itself does not.
fn ledger_in_use(workspace: &WeaverWorkspace) -> Option<PathBuf> {
    let ledger = workspace.ledger_path();
    ["-wal", "-shm", "-journal"].into_iter().find_map(|suffix| {
        let mut name = ledger.file_name().unwrap_or_default().to_os_string();
        name.push(suffix);
        let beside = ledger.with_file_name(name);
        is_there(&beside).then_some(beside)
    })
}

/// Carry one rule file into the new folder, making the folders it goes in,
/// and never onto something there by now.
fn carry_rule(carried: &CarriedRule, done: &mut Done) -> Result<(), NamesFailure> {
    let failed = |source| NamesFailure::Rename {
        from: carried.from.clone(),
        to: carried.to.clone(),
        source,
    };
    if let Some(parent) = carried.to.parent() {
        fs::create_dir_all(parent).map_err(failed)?;
    }
    if is_there(&carried.to) {
        return Err(failed(io::Error::from(io::ErrorKind::AlreadyExists)));
    }
    fs::rename(&carried.from, &carried.to).map_err(failed)?;
    let top = carried
        .to
        .ancestors()
        .find(|folder| folder.ends_with(claude_rules::folder_under(Names::Mindweaver)))
        .map_or_else(|| carried.to.clone(), Path::to_path_buf);
    done.0.push(Step::Carried {
        from: carried.from.clone(),
        to: carried.to.clone(),
        top,
    });
    Ok(())
}

/// Spell the markers of the block in `file`, a project's `AGENTS.md` or one
/// of its `CLAUDE.md` files, the new way, every other byte as it is.
fn respell_block(root: &Path, file: &Path, done: &mut Done) -> Result<(), NamesFailure> {
    let failed = |source| NamesFailure::Write {
        path: file.to_path_buf(),
        source,
    };
    let before = fs::read(file).map_err(failed)?;
    let invalid = |detail: String| failed(io::Error::new(io::ErrorKind::InvalidData, detail));
    let after = if file == root.join(agents_md::FILE) {
        let mut text = AgentsMd::parse(&before).map_err(|problem| invalid(problem.to_string()))?;
        text.renamed(Names::Mindweaver);
        text.to_bytes()
    } else {
        let mut text = ClaudeMd::parse(&before).map_err(|problem| invalid(problem.to_string()))?;
        text.renamed(Names::Mindweaver);
        text.to_bytes()
    };
    if after == before {
        return Ok(());
    }
    replace_bytes(file, &after).map_err(|(_, source)| failed(source))?;
    done.0.push(Step::Written {
        path: file.to_path_buf(),
        before,
    });
    Ok(())
}

/// Spell the markers of the block in the exclude file at `file` the new
/// way, and put each of `lines`, a group's key and a line of it, the new
/// line in its place.
fn respell_exclude(
    file: &Path,
    lines: &[(String, String, String)],
    done: &mut Done,
) -> Result<(), NamesFailure> {
    let before = match fs::read(file) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(NamesFailure::Write {
                path: file.to_path_buf(),
                source,
            })
        }
    };
    let mut text = ExcludeFile::parse(&before).map_err(|problem| ExcludeError::Malformed {
        file: file.to_path_buf(),
        problem,
    })?;
    text.renamed(Names::Mindweaver);
    for (key, from, to) in lines {
        text.replace(key, from, to)?;
    }
    let after = text.to_bytes();
    if after == before {
        return Ok(());
    }
    replace_bytes(file, &after).map_err(|(path, source)| NamesFailure::Write { path, source })?;
    done.0.push(Step::Written {
        path: file.to_path_buf(),
        before,
    });
    Ok(())
}

/// Point each line of `directories` at its folder in `.mindweaver`,
/// changing only the value of its line, so the comment beside it and every
/// other byte of the marker stay as they were.
fn repoint(document: &mut toml_edit::DocumentMut, directories: &[Repointed]) -> Result<(), String> {
    let table = document
        .get_mut("directories")
        .and_then(toml_edit::Item::as_table_like_mut)
        .ok_or_else(|| String::from("`directories` is not a table"))?;
    for repointed in directories {
        let to = paths::to_config_string(&repointed.to).unwrap_or_default();
        let value = table
            .get_mut(&repointed.key)
            .and_then(toml_edit::Item::as_value_mut)
            .ok_or_else(|| format!("`directories.{}` is not a value", repointed.key))?;
        let decor = value.decor().clone();
        *value = toml_edit::Value::from(to);
        *value.decor_mut() = decor;
    }
    Ok(())
}

/// `fs::rename`, never onto something there by now.
fn rename(from: &Path, to: &Path) -> Result<(), NamesFailure> {
    let failed = |source| NamesFailure::Rename {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        source,
    };
    if is_there(to) {
        return Err(failed(io::Error::from(io::ErrorKind::AlreadyExists)));
    }
    fs::rename(from, to).map_err(failed)
}

/// Remove the folders a carried file left empty, from `from` up to and
/// including `top`, Claude's rules folder of Mindflayer's, and no further,
/// as a render prunes them.
fn prune(top: &Path, from: Option<&Path>) {
    let mut current = from;
    while let Some(folder) = current {
        if !folder.starts_with(top) || fs::remove_dir(folder).is_err() {
            break;
        }
        if folder == top {
            break;
        }
        current = folder.parent();
    }
}

/// Whether anything at all is at `path`, a link that leads nowhere included.
fn is_there(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

#[cfg(test)]
#[path = "names_test.rs"]
mod tests;
