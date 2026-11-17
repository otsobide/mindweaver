//! Bringing a skill somebody keeps in a project's agents' folders into the
//! workspace's library, and making the project's copies of it an
//! installation from there: `mindweaver adopt` (#59).
//!
//! Many repositories keep hand-written skills in `.claude/skills` or
//! `.agents/skills` already. Mindweaver lists each as foreign, never its to
//! change, so nothing it does reaches one: no update, and no copy for an
//! agent that reads the other folder. Adopting one, when asked and never by
//! itself, copies it into the library and records the project's copies as
//! installed from that copy, with the hash they hold, so from then on they
//! are managed as anything installed is ([`crate::install`]): current while
//! nobody changes them, edited once somebody does, outdated when the library
//! copy moves on. The project's bytes are never written: what is there is
//! what is recorded.
//!
//! So what is adopted is held to what an install could have written there,
//! since that is what every later look at it compares it with: a skill an
//! agent loads, by the Agent Skills rules a write is held to; the same bytes
//! in every target that has it; and nothing a copy cannot hold, a link or a
//! top-level `.claude-plugin`, which an install leaves out. Anything else is
//! refused, with the reason, rather than recorded under a hash an update
//! would then disagree with.
//!
//! [`plan`] says what adopting would do, writing nothing, the ledger read
//! without being made, and [`apply`] does it: under the workspace's lock,
//! planned again there, the library copy and the record written together or
//! not at all.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::agent::{Agent, AgentHomes};
use crate::artifact::{self, issue_list, Artifact, ArtifactError, ValidationIssue};
use crate::catalog::{Catalog, Reference, QUALIFIER};
use crate::code::codes;
use crate::copy;
use crate::exclude::{self, ExcludeError, Excluded};
use crate::gather::Scrub;
use crate::hash;
use crate::install::{self, InstallError, WORKSPACE_ORIGIN};
use crate::kind::Kind;
use crate::ledger::{Action, InstallRecord, Ledger, LedgerError, Outcome, Target};
use crate::skill::SkillManifest;
use crate::value::{ArtifactName, ContentHash, Origin};
use crate::workspace::{same_directory, MindProject, WeaverWorkspace, WorkspaceError};

/// What the `metadata` keys start with that `gh skill` writes into a skill
/// it installed, saying where it came from: shown ([`Plan::provenance`]),
/// never acted on.
pub const PROVENANCE_PREFIX: &str = "github-";

/// What adopting a skill from a project would do: [`plan`].
#[derive(Debug, Clone)]
pub struct Plan {
    /// The project it is adopted from.
    pub project: MindProject,
    /// The skill's name, which it declares and its folder is called.
    pub name: ArtifactName,
    /// Where its copy in the library goes: the workspace's own folder for
    /// skills, under its name.
    pub library: PathBuf,
    /// Each folder an install of it into the project puts a copy in
    /// ([`install::targets`]), in that order, and whether one is there.
    pub copies: Vec<Copy>,
    /// The copy the library's is made from: the first that is there, every
    /// other one holding the same.
    pub source: PathBuf,
    /// What every copy that is there holds ([`hash::tree`]): what the
    /// library copy will hold, and the hash the record keeps.
    pub hash: ContentHash,
    /// Each `metadata` entry its `SKILL.md` declares under a key starting
    /// with [`PROVENANCE_PREFIX`], in the order written: where `gh skill`
    /// says it got it, for a front end to show. Nothing reads it further,
    /// and no other tool's lockfile is opened.
    pub provenance: Vec<(String, String)>,
    /// Whether the project is local and no git work tree holds it, so
    /// nothing keeps its copies out of git, and none is said to be: for a
    /// front end to say, as an install there says it
    /// ([`Excluded::Outside`]).
    pub outside: bool,
}

/// One folder an install of the skill into the project puts a copy in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copy {
    /// Where the copy is, or would be.
    pub path: PathBuf,
    /// The agents it is for, as the install's target has them.
    pub agents: Vec<Agent>,
    /// Whether it is there. One that is not is recorded all the same, and so
    /// reads as missing, for `mindweaver update` to put there from the library:
    /// adopting writes nothing in the project.
    pub there: bool,
    /// What a local project does with it in git's exclude file, when it is
    /// there: `None` for a shared project, and one a `mind.toml` marks,
    /// whose copies are the team's to commit, and for a local one no work
    /// tree holds, which has no exclude file ([`Plan::outside`]).
    pub git: Option<Git>,
}

/// What a local project's exclude file does with a copy adopted from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Git {
    /// Git tracks nothing in it, so it is kept out of git from now on, as
    /// what an install writes there is (D-02).
    KeptOut,
    /// Git tracks something in it, so it is left in git, the team's: a line
    /// would keep none of it out and hide what is added to it later
    /// ([`install::untracked`]).
    Tracked,
}

/// What [`apply`] did.
#[derive(Debug)]
pub struct Adopted {
    /// What was planned, under the lock, and is now done: the library copy
    /// made, and every copy recorded as installed from it.
    pub plan: Plan,
    /// What keeping a local project's untracked copies out of git came to:
    /// `None` when it had none to keep out, as one no work tree holds has
    /// not ([`Plan::outside`]).
    pub excluded: Option<Excluded>,
    /// What opening the ledger to write took out of it, for the front end
    /// to say.
    pub scrub: Scrub,
}

/// Why a skill could not be adopted, before anything was written.
#[derive(Debug, Error)]
pub enum AdoptError {
    /// Named for what it is not, and holding what it is, so a kind added
    /// is refused by the one condition, with the one code.
    #[error("{kind} `{name}` is not adopted: only a skill somebody keeps in an agent's folder is")]
    NotASkill { kind: Kind, name: String },
    #[error("{project} holds no skill `{name}` where an install of it would go ({folders}), so there is nothing to adopt")]
    NotHeld {
        project: String,
        name: String,
        folders: String,
    },
    #[error("skill `{name}` is installed in {project} already, from {origin}, so it is Mindweaver's to update or uninstall, not to adopt")]
    Installed {
        project: String,
        name: String,
        origin: Origin,
    },
    #[error("{path} is {project}'s own, in the folder it authors skills in, so it is not adopted: only somebody's skill in an agent's folder is")]
    Authored { project: String, path: PathBuf },
    #[error("{path} is inside {home}, which {reader} reads for every project, so it is not adopted: Mindweaver manages one project at a time")]
    UserScope {
        path: PathBuf,
        home: PathBuf,
        reader: &'static str,
    },
    #[error("{path} cannot be adopted as it is ({reason}): an install never writes that, so the copy could not be told from an edit later")]
    Unreproducible { path: PathBuf, reason: String },
    #[error("{path} holds {left}, which an install leaves out of every copy, since {reason}, so it is not adopted: move it out of the skill first")]
    LeftOut {
        path: PathBuf,
        left: String,
        reason: &'static str,
    },
    #[error("`{name}` cannot be adopted: {}", issue_list(.issues))]
    Unfit {
        name: String,
        issues: Vec<ValidationIssue>,
    },
    #[error("{first} and {second} do not hold the same, so which is the skill cannot be told: make them the same, or take one out, and adopt it then")]
    Differ { first: PathBuf, second: PathBuf },
    #[error("the library holds `{name}` already, at {library}, so {copy} is not adopted: rename one of them first")]
    InLibrary {
        name: String,
        library: PathBuf,
        copy: PathBuf,
    },
    #[error("{path}: cannot be written: {source}, so nothing was adopted")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{copy} changed while it was copied to {library}, so nothing was adopted and that copy was taken back out: adopt it again")]
    Changed { library: PathBuf, copy: PathBuf },
    #[error("{0}, and nothing was adopted, since its copies could not be kept out of git")]
    Exclude(#[from] ExcludeError),
    #[error(transparent)]
    Install(#[from] InstallError),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
}

codes!(AdoptError {
    Self::NotASkill { .. } => "adopt_not_a_skill",
    Self::NotHeld { .. } => "adopt_not_held",
    Self::Installed { .. } => "adopt_already_installed",
    Self::Authored { .. } => "adopt_authored",
    Self::UserScope { .. } => "adopt_user_scope",
    Self::Unreproducible { .. } => "adopt_unreproducible",
    Self::LeftOut { .. } => "adopt_left_out",
    Self::Unfit { .. } => "adopt_unfit",
    Self::Differ { .. } => "adopt_copies_differ",
    Self::InLibrary { .. } => "adopt_in_library",
    Self::Write { .. } => "adopt_write_failed",
    Self::Changed { .. } => "adopt_changed",
} carried {
    Self::Exclude(error) => error,
    Self::Install(error) => error,
    Self::Artifact(error) => error,
    Self::Ledger(error) => error,
    Self::Workspace(error) => error,
});

/// What adopting the skill `reference` names from `project`, which
/// `workspace` links, would do: nothing written, the ledger read without
/// being made.
///
/// `homes` are the agents' own folders, which the front end decides, as for
/// an install: a copy inside one is no one project's to adopt.
pub fn plan(
    workspace: &WeaverWorkspace,
    project: &MindProject,
    reference: &Reference,
    homes: &AgentHomes,
) -> Result<Plan, AdoptError> {
    let ledger = workspace.ledger_for_reading()?;
    plan_with(workspace, &ledger, project, reference, homes)
}

/// Adopt the skill `reference` names from `project`: [`plan`], done.
///
/// The ledger is opened to write, scrubbing it, before the workspace's lock
/// is taken, since a scrub takes it itself, and the lock is held until the
/// record is written, so what the ledger says was adopted is what the
/// library holds. Under it the project is read again and the plan made
/// again: one shown a minute before, or a change in between, decides
/// nothing.
///
/// Then, in that order: a local project's copies that git tracks nothing in
/// are kept out of git, so a block that cannot be edited refuses the whole
/// adoption with nothing written; the library copy is made, from the first
/// copy, and held to the plan's hash, so a copy changed while it was copied
/// is not recorded under a hash it no longer holds; and the installation is
/// recorded, every target an install would write, with that hash and the
/// workspace's own as its origin. A step that fails takes back what those
/// before it wrote: the library copy goes, and the exclude lines this added.
/// The action log is written last, and one that cannot be is an error, as
/// it is after an install, with what was adopted left adopted, both halves
/// of it there. Nothing in the project is written, ever.
pub fn apply(
    workspace: &WeaverWorkspace,
    project: &MindProject,
    reference: &Reference,
    homes: &AgentHomes,
) -> Result<Adopted, AdoptError> {
    let (ledger, scrub) = workspace.ledger_to_write()?;
    let _lock = workspace.lock().map_err(WorkspaceError::from)?;
    let project = MindProject::open(project.root())?;
    let plan = plan_with(workspace, &ledger, &project, reference, homes)?;

    let root = project.root();
    let out: Vec<PathBuf> = plan
        .copies
        .iter()
        .filter(|copy| copy.git == Some(Git::KeptOut))
        .map(|copy| copy.path.clone())
        .collect();
    let (excluded, fresh) = match install::local(&project) {
        Some(key) if !out.is_empty() => {
            let entries = install::entries(&project, &out);
            // Only the lines this adds are taken back out on a failure: one
            // the group held already, by hand or from an install whose
            // record has gone since, was not this adoption's to take.
            let fresh: Vec<exclude::Entry> = match exclude::listed(root, key)? {
                Some(listing) => entries
                    .iter()
                    .filter(|entry| {
                        entry
                            .pattern(&listing.route)
                            .is_ok_and(|line| !listing.lines.contains(&line))
                    })
                    .cloned()
                    .collect(),
                None => Vec::new(),
            };
            (Some(exclude::add(root, key, &entries)?), fresh)
        }
        _ => (None, Vec::new()),
    };
    let undo = |library: bool| {
        if library {
            let _ = fs::remove_dir_all(&plan.library);
        }
        if let (Some(key), false) = (install::local(&project), fresh.is_empty()) {
            let _ = exclude::remove(root, key, &fresh);
        }
    };

    if let Err(error) = into_library(&plan) {
        // A library copy that was there before it was made is somebody's,
        // and stays.
        undo(!matches!(error, AdoptError::InLibrary { .. }));
        return Err(error);
    }
    let entry = install::entry_for_root(workspace, root);
    let recorded = ledger.installed(
        &entry,
        &InstallRecord {
            kind: Kind::Skill,
            name: plan.name.clone(),
            source_id: None,
            origin: Some(Origin::from(WORKSPACE_ORIGIN)),
            content_hash: Some(plan.hash.clone()),
            targets: plan
                .copies
                .iter()
                .map(|copy| Target {
                    agent: install::agent_of(&copy.agents),
                    path: install::route(root, &copy.path),
                    // Every copy holds the tree `content_hash` names.
                    content_hash: None,
                })
                .collect(),
            entries: Vec::new(),
        },
    );
    if let Err(error) = recorded {
        undo(true);
        return Err(error.into());
    }
    let detail = format!(
        "{} from {} into the library at {}",
        plan.name,
        project.name(),
        install::route(workspace.root(), &plan.library)
    );
    ledger.log(Action::Adopt, Some(&entry), Outcome::Ok, Some(&detail))?;
    Ok(Adopted {
        plan,
        excluded,
        scrub,
    })
}

/// Make the library copy `plan` names, from its source, holding the hash
/// the plan recorded every copy holds.
///
/// Its folder is made with `create_dir`, which refuses one somebody made in
/// the meantime rather than copying into it, and the folder of the
/// workspace's own skills above it is made when it is missing, as the first
/// one added makes it. Built in place rather than beside: no agent reads
/// the library, and a rename into a folder made since would put it inside.
fn into_library(plan: &Plan) -> Result<(), AdoptError> {
    let write = |path: &Path, source: io::Error| AdoptError::Write {
        path: path.to_path_buf(),
        source,
    };
    if let Some(folder) = plan.library.parent() {
        fs::create_dir_all(folder).map_err(|source| write(folder, source))?;
    }
    fs::create_dir(&plan.library).map_err(|source| match source.kind() {
        io::ErrorKind::AlreadyExists => AdoptError::InLibrary {
            name: plan.name.to_string(),
            library: plan.library.clone(),
            copy: plan.source.clone(),
        },
        _ => write(&plan.library, source),
    })?;
    // Links are followed only inside the copy, and the plan refused any it
    // held, so one there now was put there since, and the hash refuses it.
    let copied = copy::tree(&plan.source, &plan.library, &plan.source)
        .and_then(|()| hash::tree(&plan.library));
    match copied {
        Ok(hash) if hash == plan.hash => Ok(()),
        Ok(_) => Err(AdoptError::Changed {
            library: plan.library.clone(),
            copy: plan.source.clone(),
        }),
        Err(source) => Err(write(&plan.library, source)),
    }
}

/// [`plan`], against `ledger`.
///
/// Only the targets an install of the skill uses are looked in
/// ([`install::targets`]): a copy anywhere else is one no install writes or
/// updates, and recording it would leave the next install, which records
/// its copies afresh, to make it somebody's again.
fn plan_with(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    reference: &Reference,
    homes: &AgentHomes,
) -> Result<Plan, AdoptError> {
    let name = reference.name().to_owned();
    // Matched on the kind, so a third one has to say whether it is adopted.
    match reference.kind() {
        None | Some(Kind::Skill) => {}
        // A rule reaches a project's agents as a section of the block in its
        // `AGENTS.md`, rendered from the library; one somebody wrote is no
        // folder to copy, and taking rules out of a hand-written `AGENTS.md`
        // is a change of its own.
        //
        // A server an agent is given is an entry in its own config file,
        // never a folder to copy either; adopting one is a change of its own,
        // once one is installed (#94).
        Some(kind @ (Kind::Rule | Kind::Mcp)) => return Err(AdoptError::NotASkill { kind, name }),
    }
    let kind = Kind::Skill;
    // Joined onto the project's folders below, and onto the library's.
    if !install::is_folder_name(&name) {
        return Err(InstallError::UnsafeName { name }.into());
    }
    let targets = install::targets(project, kind)?;
    let paths: Vec<PathBuf> = targets
        .iter()
        .map(|target| install::copy_at(&target.folder, kind, &name))
        .collect();
    if let Some((path, home, reader)) = install::in_agent_home(homes, &paths) {
        return Err(AdoptError::UserScope { path, home, reader });
    }

    let entry = install::entry_for_root(workspace, project.root());
    if let Some(record) = ledger.installation(&entry, kind, &name)? {
        return Err(AdoptError::Installed {
            project: project.name().to_owned(),
            name,
            origin: record
                .origin
                .unwrap_or_else(|| Origin::from("a place not recorded")),
        });
    }
    let there: Vec<&PathBuf> = paths
        .iter()
        .filter(|path| install::holds(path, kind))
        .collect();
    let Some(first) = there.first().copied() else {
        return Err(AdoptError::NotHeld {
            project: project.name().to_owned(),
            name,
            folders: targets
                .iter()
                .map(|target| install::route(project.root(), &target.folder))
                .collect::<Vec<_>>()
                .join(", "),
        });
    };
    // A target that is the folder the project authors in, as a `mind.toml`'s
    // always is, holds the project's own work, which a change from the
    // project reaches and an install's record would claim.
    if let Some(authored) = project.directory_for(kind) {
        if let Some(path) = there.iter().find(|path| {
            path.parent()
                .is_some_and(|folder| same_directory(folder, &authored))
        }) {
            return Err(AdoptError::Authored {
                project: project.name().to_owned(),
                path: (*path).clone(),
            });
        }
    }
    for path in &there {
        if let Some(loaded) = install::kept_by_a_load(workspace, kind, path) {
            return Err(InstallError::KeptByALoad {
                path: (*path).clone(),
                loaded,
            }
            .into());
        }
    }
    let library = in_library(workspace, &name, first)?;

    // Each copy held to what an install could have written, and every one
    // to the first: one hash is recorded, which each copy is told by.
    let manifest = held_to_an_install(first, &name)?;
    let hash = hash_of(first)?;
    for path in there.iter().skip(1) {
        held_to_an_install(path, &name)?;
        if hash_of(path)? != hash {
            return Err(AdoptError::Differ {
                first: first.clone(),
                second: (*path).clone(),
            });
        }
    }

    let local = install::local(project);
    let mut copies: Vec<Copy> = targets
        .iter()
        .zip(&paths)
        .map(|(target, path)| {
            let there = there.contains(&path);
            let git = (local.is_some() && there).then(|| {
                if install::untracked(project, std::slice::from_ref(path)).is_empty() {
                    Git::Tracked
                } else {
                    Git::KeptOut
                }
            });
            Copy {
                path: path.clone(),
                agents: target.agents.clone(),
                there,
                git,
            }
        })
        .collect();
    // Read only when a line would be written, so a dry run refuses a block
    // that cannot be edited as the adoption would, and a copy outside every
    // work tree, which git tracks nothing in, is not said to be kept out of
    // a git there is none of.
    let mut outside = false;
    if let Some(key) = local {
        let keeps_out = copies.iter().any(|copy| copy.git == Some(Git::KeptOut));
        if keeps_out && exclude::listed(project.root(), key)?.is_none() {
            outside = true;
            for copy in &mut copies {
                copy.git = None;
            }
        }
    }
    let provenance = manifest
        .metadata
        .entries
        .into_iter()
        .filter(|(key, _)| key.starts_with(PROVENANCE_PREFIX))
        .collect();
    Ok(Plan {
        project: project.clone(),
        name: ArtifactName::from(name),
        library,
        copies,
        source: first.clone(),
        hash,
        provenance,
        outside,
    })
}

/// Where the library copy of `name` goes, adopted from `copy`: refused when
/// the library holds that name already, by its folder or by what one of
/// its skills declares, which would be two skills of one name the
/// workspace offers.
fn in_library(workspace: &WeaverWorkspace, name: &str, copy: &Path) -> Result<PathBuf, AdoptError> {
    let own = workspace.own();
    let folder = own.required_directory(Kind::Skill)?;
    let library = install::copy_at(&folder, Kind::Skill, name);
    let held = if fs::symlink_metadata(&library).is_ok() {
        Some(library.clone())
    } else {
        let declared = Reference::parse(&format!("{}{QUALIFIER}{name}", Kind::Skill.slug()));
        Catalog::discover_kinds(std::slice::from_ref(&own), &[Kind::Skill])
            .find(&declared)
            .first()
            .map(|artifact| artifact.path().to_path_buf())
    };
    if let Some(library) = held {
        return Err(AdoptError::InLibrary {
            name: name.to_owned(),
            library,
            copy: copy.to_path_buf(),
        });
    }
    // One inside the other would copy a folder into itself.
    if let (Ok(from), Some(to)) = (crate::paths::canonical(copy), install::resolved(&library)) {
        if to.starts_with(&from) || from.starts_with(&to) {
            return Err(InstallError::Overlaps {
                from: copy.to_path_buf(),
                to: library,
            }
            .into());
        }
    }
    Ok(library)
}

/// What the copy at `path` holds ([`hash::tree`]), which refuses a link
/// inside it, or anything else neither a file nor a folder: no install
/// writes one, and following it could read anything.
fn hash_of(path: &Path) -> Result<ContentHash, AdoptError> {
    hash::tree(path).map_err(|error| AdoptError::Unreproducible {
        path: path.to_path_buf(),
        reason: error.to_string(),
    })
}

/// The manifest of the skill at `path`, once it is held to what an install
/// of `name` could have written there: a folder, not a link to one; a skill
/// that breaks none of the rules an install refuses a skill over
/// ([`artifact::strict_issues`]), filed under its name; and nothing at its
/// top that an install leaves out of every copy ([`install::LeftOut`]).
///
/// Strict, as installing is: the library copy is a write, and what an
/// agent would skip is not put there.
fn held_to_an_install(path: &Path, name: &str) -> Result<SkillManifest, AdoptError> {
    let unreproducible = |reason: String| AdoptError::Unreproducible {
        path: path.to_path_buf(),
        reason,
    };
    let metadata = fs::symlink_metadata(path).map_err(|error| unreproducible(error.to_string()))?;
    if metadata.file_type().is_symlink() {
        return Err(unreproducible(match fs::read_link(path) {
            Ok(target) => format!("it is a link to {}", target.display()),
            Err(_) => String::from("it is a link"),
        }));
    }
    if !metadata.is_dir() {
        return Err(unreproducible(String::from("it is no folder")));
    }
    let skill = Artifact::skill(path, path)?;
    let Some(manifest) = skill.manifest() else {
        unreachable!("a skill declares a skill's manifest");
    };
    let issues = artifact::strict_issues(manifest, name);
    if !issues.is_empty() {
        return Err(AdoptError::Unfit {
            name: name.to_owned(),
            issues,
        });
    }
    if let Some(left) = install::left_out(path).into_iter().next() {
        return Err(AdoptError::LeftOut {
            path: path.to_path_buf(),
            left: left
                .path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
            reason: left.reason,
        });
    }
    Ok(manifest.clone())
}

#[cfg(test)]
#[path = "adopt_test.rs"]
mod tests;
