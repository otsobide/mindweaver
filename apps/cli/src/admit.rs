//! A task from outside, held to the path policy before a line of it is
//! built: what the panel's backend runs in place of what the browser sent.
//!
//! Every task the panel sends is untrusted (D-18). The rule that no value is
//! read as an option or a command ([`crate::task`]) keeps a value from
//! changing what a line is; this keeps each value a browser could aim at
//! the disk to what the workspace the panel was launched on allows
//! ([`mindweaver_core::policy`]), and names each project to the command by
//! the path its entry resolves to, never as the browser spelled it:
//!
//! - **A holder** is an entry the registry holds, and becomes the `-C` of
//!   the folder it points at, absolute, in the one `-C` the line has, with
//!   `-W .` in front where `mind` would otherwise consult another workspace
//!   about it, as the TUI decides for its rows.
//! - **Each project an install, an uninstall, an adoption or a migration
//!   names** is an entry too, and becomes the `-p` of the same folder.
//! - **A yes an install carries** is one to an MCP server that asks first
//!   (`policy::confirmed`), the panel's confirmation of one, and no other.
//! - **A server's options an `add` or an `edit` sends** name nothing a
//!   line would say as another (`policy::sayable`): no name beside a value
//!   holding the `=` its option ends the name at.
//! - **An entry `unlink`, `unload` or `relink` takes** is one the marker
//!   holds, and goes to the command as the marker spells it, which is what
//!   those commands take an entry by.
//! - **A folder `link`, `load`, `init` or the minds screen's marks name**,
//!   the panel's marks among them, is absolute, spelled normalized and
//!   outside every `.mindweaver`, and goes to the command verbatim, so the
//!   preview shows the folder it acts on; one marked to unlink is one an
//!   entry points at. So is the folder an export writes into.
//! - **A gather's source and a backup's repository** are an address, which
//!   the command judges as one typed at a terminal, or such a folder.
//! - **A backup's token** is named by the variable holding it, one named
//!   for a token (`policy::token_variable`), and never sent.
//!
//! The walk over the variants is here, beside `Task`; what each value may be
//! is core's, so a second front end taking tasks from outside asks the same
//! questions and gets the same answers.

use mindweaver_core::install::Cell;
use mindweaver_core::policy::{self, PolicyError};
use mindweaver_core::{Member, WeaverWorkspace};

use crate::commands::consulted;
use crate::commands::workspace::same_directory;
use crate::homes::Lookup;
use crate::task::{Holder, MindsChange, Task};

/// What a task from outside may ask of `workspace`, the one the panel was
/// launched on, which `lookup` is where it looks for workspaces from.
///
/// Public, as [`Task`] is, for any front end that runs tasks it is sent.
pub struct Admission<'a> {
    pub workspace: &'a WeaverWorkspace,
    pub lookup: &'a Lookup,
}

impl Admission<'_> {
    /// `task`, with every value it holds allowed and every project it names
    /// named by its folder; or why the first value that is not allowed is
    /// not.
    ///
    /// One arm per variant with no wildcard, as `Refusal::of_mind`'s, so a
    /// new task does not compile until it says what of it a browser may
    /// name.
    pub fn task(&self, task: &Task) -> Result<Task, PolicyError> {
        let mut admitted = task.clone();
        match &mut admitted {
            Task::InitWorkspace
            | Task::InitProject
            | Task::LinkHere
            | Task::Shelf
            | Task::Workspaces
            | Task::OpenInstall
            | Task::Refresh => {}
            // A query names nothing on the disk, and the line asks only the
            // finders the workspace enables, with no token.
            Task::SearchSkills { .. } => {}
            // A query, a server's name and a version name nothing on the
            // disk: the registry is the official one, which the lines name
            // no other than, and core holds a name to the registry's rule
            // before anything is asked.
            Task::SearchServers { .. } | Task::ImportServer { .. } => {}
            // An address is the gather's to judge, a credential in it among
            // what it refuses; a folder is held as a folder to link is. The
            // folder in the source and the reference are values, which the
            // gather holds to its own rules.
            Task::Gather { url, .. } => {
                policy::source(url)?;
            }
            // A source by its name, which names nothing on the disk: a value
            // core holds to its own rules, refusing one no source has, or
            // one that carries a credential, before anything is fetched or
            // removed.
            Task::UpdateSources { .. } | Task::RemoveSource { .. } => {}
            // The branch and the message are values, which the backup
            // holds to its own rules, a branch git would refuse among them.
            Task::Backup { url, token_env, .. } => {
                policy::source(url)?;
                if let Some(variable) = token_env {
                    policy::token_variable(variable)?;
                }
            }
            Task::Export { folder, .. } => {
                policy::path(folder)?;
            }
            Task::Link { path }
            | Task::InitAndLink { path }
            | Task::Load { path }
            | Task::LoadHere { path } => {
                policy::path(path)?;
            }
            Task::Unlink { entry } => {
                policy::linked(self.workspace, entry)?;
            }
            Task::Relink { old, new } => {
                policy::linked(self.workspace, old)?;
                policy::path(new)?;
            }
            Task::Unload { entry } => {
                policy::loaded(self.workspace, entry)?;
            }
            Task::UnloadHere { path } => {
                policy::loaded_path(self.workspace, path)?;
            }
            Task::Minds { init, link, unlink } => {
                for path in init.iter().chain(link.iter()) {
                    policy::path(path)?;
                }
                for path in unlink {
                    policy::linked_path(self.workspace, path)?;
                }
            }
            Task::Marks { marks } => {
                for mark in marks {
                    match mark.change {
                        MindsChange::Link | MindsChange::InitAndLink => {
                            policy::path(&mark.path)?;
                        }
                        MindsChange::Unlink => {
                            policy::linked_path(self.workspace, &mark.path)?;
                        }
                    }
                }
            }
            Task::Add { holder, server, .. } => {
                if let Some(given) = server {
                    policy::sayable(given)?;
                }
                *holder = self.holder(holder)?;
            }
            Task::EditServer { holder, server, .. } => {
                policy::sayable(server)?;
                *holder = self.holder(holder)?;
            }
            Task::Show { holder, .. }
            | Task::Edit { holder, .. }
            | Task::Save { holder, .. }
            | Task::Rename { holder, .. }
            | Task::Remove { holder, .. }
            | Task::Validate { holder }
            | Task::Targets { holder, .. }
            | Task::Settings { holder, .. }
            | Task::Toggled { holder, .. }
            | Task::Update { holder, .. }
            | Task::UninstallOrphaned { holder } => *holder = self.holder(holder)?,
            Task::Install {
                projects,
                kind,
                name,
                from,
                yes,
            } => {
                if *yes {
                    policy::confirmed(*kind, name, from.as_deref())?;
                }
                for project in projects {
                    *project = self.project(project)?;
                }
            }
            Task::Uninstall { projects, .. } => {
                for project in projects {
                    *project = self.project(project)?;
                }
            }
            Task::Adopt { project, .. } | Task::Migrate { project, .. } => {
                *project = self.project(project)?;
            }
            Task::ApplyInstall { changes } => {
                for change in changes {
                    *change = self.task(change)?;
                }
            }
        }
        Ok(admitted)
    }

    /// Every project `selection` names, each box's by its entry, one the
    /// workspace's registry holds: what a plan the panel asks for may name
    /// (the backend's `Query::Plan`), as a task's projects may.
    ///
    /// Asked here, before core plans, as of every value a browser sends,
    /// though core's plan refuses an entry it does not link as well: one
    /// walk over what a browser sent holds it to the policy, whichever
    /// reader it is for. Nothing is spelled again, since a plan runs no
    /// line: the tasks it hands back name each project by its entry, and are
    /// admitted when they come back to run.
    pub fn selection(&self, selection: &[Cell]) -> Result<(), PolicyError> {
        for cell in selection {
            policy::linked(self.workspace, &cell.project)?;
        }
        Ok(())
    }

    /// `holder`, a project's by the folder its entry points at.
    fn holder(&self, holder: &Holder) -> Result<Holder, PolicyError> {
        let (Holder::Project(entry) | Holder::Elsewhere(entry)) = holder else {
            return Ok(Holder::Workspace);
        };
        let member = policy::linked(self.workspace, entry)?;
        let folder = policy::spelled(&member.root)?;
        Ok(if self.elsewhere(&member) {
            Holder::Elsewhere(folder)
        } else {
            Holder::Project(folder)
        })
    }

    /// The folder the project `entry` names, as a `-p` names it.
    ///
    /// The path rather than [`WeaverWorkspace::naming`], which the TUI's
    /// lines name a project by: `naming` is an entry, or a project's name
    /// would do, and either is a second spelling of what the registry
    /// resolved, where the path is the folder the policy checked.
    fn project(&self, entry: &str) -> Result<String, PolicyError> {
        policy::spelled(&policy::linked(self.workspace, entry)?.root)
    }

    /// Whether `mind` consults some other workspace about `member`'s
    /// project, or none, from where the panel was launched: then its lines
    /// say `-W .`, so that read again from the root they consult this one,
    /// as they do when they run, and as the TUI's rows decide it. A project
    /// that does not open is no project's line to decide for: the command
    /// says why it cannot run.
    fn elsewhere(&self, member: &Member) -> bool {
        let Ok(project) = &member.project else {
            return false;
        };
        !matches!(
            consulted(project, self.lookup),
            Ok(Some(workspace)) if same_directory(workspace.root(), self.workspace.root())
        )
    }
}

#[cfg(test)]
#[path = "admit_test.rs"]
mod tests;
