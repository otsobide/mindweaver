//! Changing what a project holds: adding to it, finding an artifact by what
//! somebody typed, editing it, renaming it, removing it.
//!
//! Finding is more forgiving than listing. A skill whose front matter does not
//! parse is left out of every listing, but it is exactly the one somebody
//! needs to open and fix, so an artifact is also found by where its name would
//! put it on disk, whether or not it reads.
//!
//! Each change also sees to the ledger's record of what was installed under
//! the names it touches, and says in its result what it did ([`Record`]). The
//! rule is D-04's: an installed copy that is edited or renamed stays
//! recorded, told by the hash of what was written, and an edit, a skill's
//! rename among them, reads as edited, never written over or deleted unless
//! forced; one that is removed is uninstalled, its record going with it; and
//! one added where a record outlived its copy drops that record. It lives
//! here rather than in a front end, so that no front end can apply it
//! differently. Only one that changes things through these functions gets
//! it, though: [`create`] and [`create_from`], the writers [`add`] is built
//! on, see to no record, and a front end that added with them would leave a
//! stale one in place.
//!
//! [`create`]: crate::create::create
//! [`create_from`]: crate::create::create_from
//!
//! Changing a record opens the ledger to write, which scrubs it first
//! ([`WeaverWorkspace::ledger_to_write`]), so each result that may also hands
//! back what that took out ([`Scrub`]) for the front end to say. Said
//! nowhere, it would go unsaid for good: a scrub says what it found only the
//! first time. An edit changes none, and only reads the ledger.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::artifact::{self, issue_list, Artifact, ArtifactError, ValidationIssue};
use crate::catalog::{Catalog, Reference, QUALIFIER};
use crate::code::codes;
use crate::create::{self, scaffold, CreateError, Start};
use crate::frontmatter;
use crate::gather::Scrub;
use crate::hash;
use crate::install::{self, InstallError};
use crate::kind::{Kind, Layout};
use crate::ledger::Carried;
use crate::mcp::{self, Given, McpDefinition};
use crate::paths;
use crate::skill::SkillManifest;
use crate::value::{ArtifactName, ContentHash};
use crate::workspace::{
    replace_bytes_unresolved, replace_file, MindProject, WeaverWorkspace, WorkspaceError,
};

/// An artifact found by what somebody typed.
#[derive(Debug, Clone)]
pub struct Target {
    pub kind: Kind,
    /// What it declares, when it reads; otherwise the name its place on disk
    /// gives it.
    pub name: ArtifactName,
    /// The directory a skill is, or the file a rule is.
    pub path: PathBuf,
    /// The artifact itself, when it can be read.
    pub artifact: Option<Artifact>,
}

impl Target {
    /// The file to open to change it: a skill's manifest, or the rule itself.
    pub fn file(&self) -> PathBuf {
        file_of(self.kind, &self.path)
    }

    /// `skill:deploy`.
    pub fn qualified_name(&self) -> String {
        format!("{}{QUALIFIER}{}", self.kind.slug(), self.name)
    }
}

/// What a change did to the ledger's record that Mindweaver installed what
/// is under one name.
///
/// That record is what gives an install leave to overwrite and an uninstall
/// leave to delete, so it must not outlive the copy it was about: whatever
/// somebody writes there next would inherit it. While the copy is there,
/// edited or renamed, it stays: the hash it holds tells an edit apart, and
/// what was installed is still said, still taken out when it is uninstalled,
/// with `--force` once it holds an edit (D-04).
///
/// A change reports it as `Result<Record, InstallError>` beside what it made
/// rather than failing with it: by the time the record is seen to, the change
/// has been made, and reporting it as not made would be false.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Record {
    /// No record was changed: there was none (no workspace was consulted,
    /// the one consulted does not manage the project, or it installed nothing
    /// under that name) or the change gave no reason to touch it.
    Untouched,
    /// The copy the change reached is recorded, and stays so as it was: an
    /// edit leaves it holding something other than what was installed, which
    /// reads as edited.
    Kept,
    /// The record of the copy the change reached names it under its new name
    /// and place, from where it came. `edited` says whether the copy now
    /// holds something other than was installed, and so is left alone by an
    /// install and an uninstall unless forced: a skill's, whose `name` line
    /// the rename rewrote, always; a rule's, whose file was moved as it was,
    /// only when it was edited before, and otherwise it is what was
    /// installed, which an uninstall takes out unforced.
    Moved { edited: bool },
    /// The record of the copy the change reached was dropped: it was
    /// removed, and so uninstalled; or the record outlived its copy; or a
    /// rename put it under a name installed from elsewhere, and what is there
    /// now is its author's. Copies the same install wrote in other folders,
    /// which the change did not reach, are still recorded.
    Dropped,
}

/// What adding an artifact made, and what it did to a record under its name.
#[derive(Debug)]
pub struct Added {
    pub artifact: Artifact,
    /// A record of an install under the same name. Nothing was there for the
    /// new artifact to be written over, so the copy it was about had gone,
    /// and left in place the record would make somebody's new work look like
    /// something Mindweaver put there.
    pub stale: Result<Record, InstallError>,
    /// What opening the ledger to see to that record scrubbed out of it.
    pub scrub: Scrub,
}

/// Write a new artifact into `project`, from what `start` says beside its
/// name and its description (from scratch, a template, or for an MCP
/// server how it is started or reached, as [`create`], [`create_from`] and
/// [`create_server`] write one), and drop any record of an install under
/// its name.
///
/// `workspace` is the one the front end consulted about the project, given
/// rather than found by walking up from it: a walk misses every workspace
/// that links the project from somewhere that is not above it. With none,
/// there is no record to drop.
///
/// [`create`]: crate::create::create
/// [`create_from`]: crate::create::create_from
/// [`create_server`]: crate::create::create_server
pub fn add(
    workspace: Option<&WeaverWorkspace>,
    project: &MindProject,
    kind: Kind,
    name: &str,
    description: &str,
    start: Start<'_>,
) -> Result<Added, CreateError> {
    let artifact = scaffold(project, kind, name, description, start)?;
    let mut scrub = Scrub::default();
    let stale = disowned(workspace, project, kind, name, artifact.path(), &mut scrub);
    Ok(Added {
        artifact,
        stale,
        scrub,
    })
}

/// Where [`add`] would write `kind` `name`, described as `description`,
/// from what `start` says, into `project`, worked out and nothing written:
/// every check `add` makes before it writes, and then whether something is
/// where it would go already, which `add` leaves alone and refuses.
///
/// What a command that writes only once it is told yes says beforehand
/// (`mindweaver gather mcp add` without `--yes`), so the dry run refuses
/// what the write would, by the same checks rather than a copy of them.
/// What a write alone can tell it tells when it writes: a name taken a
/// moment later is still refused then.
pub fn would_add(
    project: &MindProject,
    kind: Kind,
    name: &str,
    description: &str,
    start: Start<'_>,
) -> Result<PathBuf, CreateError> {
    let scaffold = create::plan(project, kind, name, description, start)?;
    match scaffold.taken() {
        Some(taken) => Err(taken),
        None => Ok(scaffold.path().to_path_buf()),
    }
}

/// The one artifact in `project` a reference names.
///
/// By what it declares first, then by where the name would put it, so a
/// skill that does not read is still found by its folder, and so is one whose
/// folder disagrees with what it declares. More than one match is an error
/// rather than a choice made quietly: changing the wrong artifact is not
/// something to guess about.
///
/// Only in the folders the project authors in
/// ([`Catalog::discover_authored`]): a listing also shows what sits in its
/// agents' folders, an install's copies or somebody's own, and none of those
/// is the project's to edit, rename or remove. One named that is there and
/// nowhere the project authors is refused as such
/// ([`LifecycleError::NotAuthored`]), rather than as nothing at all, since a
/// listing has just shown it.
pub fn find(project: &MindProject, reference: &Reference) -> Result<Target, LifecycleError> {
    let kinds = reference
        .kind()
        .map_or_else(|| Kind::ALL.to_vec(), |kind| vec![kind]);
    let catalog = Catalog::discover_authored(std::slice::from_ref(project), &kinds);

    let mut found: Vec<Target> = catalog
        .find(reference)
        .into_iter()
        .map(|artifact| Target {
            kind: artifact.kind(),
            name: artifact.name().to_owned(),
            path: artifact.path().to_path_buf(),
            artifact: Some(artifact.clone()),
        })
        .collect();

    // Only a name made of plain folder names is turned into a place: one with
    // `..` in it would otherwise be a way out of the project. Not the naming
    // rules, which are for what Mindweaver writes: a folder from before they
    // were tightened, or one somebody made by hand, is exactly what has to be
    // found to be fixed or removed.
    let name = reference.name();
    if name.split('/').all(install::is_folder_name) {
        for &kind in &kinds {
            let Some(path) = place(project, kind, name) else {
                continue;
            };
            if !holds(kind, &path) || found.iter().any(|target| target.path == path) {
                continue;
            }
            let artifact = load(project, kind, &path, name).ok();
            found.push(Target {
                kind,
                name: ArtifactName::from(
                    artifact
                        .as_ref()
                        .map_or(name, |artifact| artifact.name().as_str()),
                ),
                path,
                artifact,
            });
        }
    }

    match found.len() {
        0 => {
            let listed = Catalog::discover_kinds(std::slice::from_ref(project), &kinds);
            Err(match listed.find(reference).first() {
                Some(artifact) => LifecycleError::NotAuthored {
                    typed: reference.typed().to_owned(),
                    path: artifact.path().to_path_buf(),
                },
                None => LifecycleError::NotFound {
                    typed: reference.typed().to_owned(),
                },
            })
        }
        1 => Ok(found.remove(0)),
        _ => Err(LifecycleError::Ambiguous {
            typed: reference.typed().to_owned(),
            matches: found
                .iter()
                .map(|target| {
                    let at = paths::relative_to(&target.path, project.root())
                        .unwrap_or_else(|| target.path.clone());
                    format!("{} at {}", target.qualified_name(), at.display())
                })
                .collect(),
        }),
    }
}

/// [`find`], for a change made with `workspace` consulted: a name the project
/// holds nothing of where it authors, but received as a rule or as an MCP
/// server, is refused as such ([`LifecycleError::Received`],
/// [`LifecycleError::ReceivedServer`]) rather than as nothing at all.
///
/// A rule a project marked by its settings received is no file of its own
/// but a section of the block in its `AGENTS.md`, and maybe a file for
/// Claude, rendered from where it was installed from (D-23), and a server
/// it received is an entry in each of its agents' config files, written
/// from the definition it was installed from: a change made there is
/// undone by the next install, or read as an edit, and what reaches it is
/// a change made where it is offered. `status` lists either, so "nothing
/// named" would deny what the report just showed; the ledger says whether
/// it is one, and the refusal says how to take it out instead. A rule is
/// asked of first, a name received as both being one a qualifier settles.
pub fn find_to_change(
    workspace: Option<&WeaverWorkspace>,
    project: &MindProject,
    reference: &Reference,
) -> Result<Target, LifecycleError> {
    let typed = match find(project, reference) {
        Err(LifecycleError::NotFound { typed }) => typed,
        found => return found,
    };
    let name = reference.name();
    let received = |kind: Kind| {
        project.settings().is_some()
            && reference.kind().is_none_or(|asked| asked == kind)
            && install::is_installable_name(kind, name)
            && workspace.is_some_and(|workspace| {
                let entry = install::entry_for_root(workspace, project.root());
                workspace
                    .ledger_for_reading()
                    .and_then(|ledger| ledger.installation(&entry, kind, name))
                    .is_ok_and(|record| record.is_some())
            })
    };
    // Typed bare, an uninstall finds the server when the ledger records
    // the name as no other kind a bare name could be ([`install::kinds_named`],
    // `uninstall_typed`); one it records as a skill too needs its qualifier.
    let recorded = |kind: Kind| {
        workspace.is_some_and(|workspace| {
            let entry = install::entry_for_root(workspace, project.root());
            workspace
                .ledger_for_reading()
                .and_then(|ledger| ledger.installation(&entry, kind, name))
                .is_ok_and(|record| record.is_some())
        })
    };
    let alone = install::kinds_named(&Reference::parse(name))
        .into_iter()
        .filter(|&kind| kind != Kind::Mcp)
        .all(|kind| !recorded(kind));
    let project = project.name().to_owned();
    Err(if received(Kind::Rule) {
        LifecycleError::Received { typed, project }
    } else if received(Kind::Mcp) {
        LifecycleError::ReceivedServer {
            typed,
            project,
            named: if alone {
                name.to_owned()
            } else {
                format!("{}{QUALIFIER}{name}", Kind::Mcp.slug())
            },
        }
    } else {
        LifecycleError::NotFound { typed }
    })
}

/// Where an artifact of `kind` called `name` lives in `project`, whether or
/// not it is there; `None` when the project keeps no folder for `kind`.
pub fn place(project: &MindProject, kind: Kind, name: &str) -> Option<PathBuf> {
    Some(place_in(&project.directory_for(kind)?, kind, name))
}

/// Where an artifact of `kind` called `name` lives in `directory`, the
/// folder that kind is kept in.
fn place_in(directory: &Path, kind: Kind, name: &str) -> PathBuf {
    match kind.layout() {
        Layout::Directory { .. } => directory.join(name),
        Layout::Files { extension } | Layout::Flat { extension } => {
            directory.join(format!("{name}.{extension}"))
        }
    }
}

/// Load the artifact at `path`, named `name` if it is the kind whose name
/// comes from where it sits.
///
/// An MCP server's definition is told of those beside it named as it is
/// in another case, as discovery tells it ([`twins_beside`]), so that a
/// verdict on one file, the one `edit` gives once the editor closes, is
/// the one `validate` gives.
pub fn load(
    project: &MindProject,
    kind: Kind,
    path: &Path,
    name: &str,
) -> Result<Artifact, ArtifactError> {
    match kind {
        Kind::Skill => Artifact::skill(path, project.root()),
        Kind::Rule => Artifact::rule(path, name.to_owned(), project.root()),
        Kind::Mcp => {
            let mut artifact = Artifact::mcp(path, name.to_owned(), project.root())?;
            if let Some(folder) = path.parent() {
                artifact.twinned(twins_beside(folder, name, path));
            }
            Ok(artifact)
        }
    }
}

/// Which artifact a file belongs to, when it sits where one would.
///
/// What lets a file that cannot be read still be named (and so opened,
/// renamed or removed) when all a listing has for it is the error.
pub fn identify(project: &MindProject, file: &Path) -> Option<(Kind, String)> {
    Kind::ALL.into_iter().find_map(|kind| {
        let below = file.strip_prefix(project.directory_for(kind)?).ok()?;
        match kind.layout() {
            Layout::Directory { manifest } => {
                let mut parts = below.components();
                let folder = parts.next()?.as_os_str().to_str()?;
                let file = parts.next()?.as_os_str();
                (file == manifest && parts.next().is_none()).then(|| (kind, folder.to_owned()))
            }
            Layout::Files { extension } => {
                let matches = below
                    .extension()
                    .is_some_and(|found| found.eq_ignore_ascii_case(extension));
                let name = paths::to_config_string(&below.with_extension(""))?;
                matches.then_some((kind, name))
            }
            Layout::Flat { extension } => {
                let mut parts = below.components();
                let file = Path::new(parts.next()?.as_os_str());
                let matches = parts.next().is_none()
                    && file
                        .extension()
                        .is_some_and(|found| found.eq_ignore_ascii_case(extension));
                let name = file.file_stem()?.to_str()?;
                matches.then(|| (kind, name.to_owned()))
            }
        }
    })
}

/// An artifact opened to be edited, with what its file held then, so that
/// finishing the edit can tell whether anything changed.
///
/// Running the editor is the front end's, since which one to run comes from
/// the environment, and core never reads that. What the edit came to, and
/// what that means for the copy's record, is core's.
#[derive(Debug, Clone)]
pub struct Edit {
    kind: Kind,
    name: String,
    /// The artifact, which is what a record names: a skill's directory, or
    /// a rule's file.
    path: PathBuf,
    file: PathBuf,
    before: Option<Vec<u8>>,
}

impl Edit {
    /// The file to open in the editor.
    pub fn file(&self) -> &Path {
        &self.file
    }
}

/// What an edit came to.
#[derive(Debug)]
pub struct Edited {
    /// Whether the file is any different from when the edit started.
    pub changed: bool,
    /// The record of the copy edited, kept when it changed and was
    /// installed: it holds something other than what was installed now,
    /// which is what tells an edit apart, so no install writes over it and
    /// no uninstall takes it out unless forced.
    pub copy: Result<Record, InstallError>,
}

/// Start editing `target`, noting what its file holds now.
pub fn start_edit(target: &Target) -> Edit {
    let file = target.file();
    Edit {
        kind: target.kind,
        name: target.name.to_string(),
        path: target.path.clone(),
        before: fs::read(&file).ok(),
        file,
    }
}

/// Finish an edit of an artifact in `project` once the editor has closed,
/// saying whether the file changed and whether an install of it stays
/// recorded. `workspace` is the one consulted, as for [`add`].
///
/// The record stays as it was (D-04). Dropping it, as an edit once did,
/// made the copy somebody's that no update could reach and no uninstall
/// could take out; kept, its hash is what tells the edit apart, and every
/// install and uninstall leaves the copy alone unless forced. So the ledger
/// is only read, to say so, never written.
///
/// A front end finishes the edit whether or not the editor succeeded: one
/// that wrote the file before it failed has changed the copy all the same.
pub fn finish_edit(
    workspace: Option<&WeaverWorkspace>,
    project: &MindProject,
    edit: Edit,
) -> Edited {
    let changed = fs::read(&edit.file).ok() != edit.before;
    let copy = kept(
        workspace.filter(|_| changed),
        project,
        edit.kind,
        &edit.name,
        &edit.path,
    );
    Edited { changed, copy }
}

/// Replace what `target` in `project` holds with `contents`, the whole of a
/// skill's manifest or a rule's file, as an editor would have written it:
/// the edit for a front end that has no editor to run, or a script that
/// writes the text itself. `workspace` is the one consulted, as for [`add`].
///
/// Held to the rules every write is held to before a byte is written. A
/// skill's must be one Mindweaver would write ([`artifact::strict_issues`]):
/// front matter that reads, a name and a description that pass, and the
/// name its folder has. A different name is refused rather than written,
/// since a name changed there and not in the folder is a skill no agent
/// finds by both; [`rename`] changes the two together. A rule's must say
/// something, and its front matter must read whole
/// ([`artifact::rule_text_issues`]), as a template's must. Anything else
/// the file holds is the writer's, written as given.
///
/// `expect`, when given, is the SHA-256 of what the file held when it was
/// read ([`hash::bytes`]): a file that holds anything else now was changed
/// by somebody else since, and is not written over. One that is no SHA-256
/// at all ([`hash::is_digest`]) is refused before anything is read, rather
/// than said to be what the file no longer holds, which would send its
/// writer to read again a file nobody changed. Without it, the write is the
/// caller's to keep from crossing another, as an editor's is.
///
/// Written where the artifact is kept and nowhere else: the file is
/// followed through its links, and one leading out of where it may be
/// written ([`written_at`]) is refused rather than written through. What
/// is written is a temporary beside the file, renamed onto it, so an agent
/// reading it finds the old contents or the new, never half of each.
///
/// The record stays as [`finish_edit`] leaves it (D-04): an installed copy
/// replaced holds something other than was installed, and reads as edited.
/// Contents the same as the file's write nothing, and say so. No lock is
/// taken, as none is for an edit, a rename or a removal: what changes is
/// the artifact's file, not anything of the workspace's, whose ledger is
/// only read.
pub fn replace_contents(
    workspace: Option<&WeaverWorkspace>,
    project: &MindProject,
    target: &Target,
    contents: &[u8],
    expect: Option<&str>,
) -> Result<Edited, LifecycleError> {
    if let Some(given) = expect.filter(|given| !hash::is_digest(given)) {
        return Err(LifecycleError::HashMalformed {
            given: given.to_owned(),
        });
    }
    let file = target.file();
    let resolved = replaceable(workspace, project, target, contents)?;

    // The resolved path from here on, never the one checked: a link put in
    // its place since would otherwise be written through.
    let held = fs::read(&resolved).map_err(|source| LifecycleError::Io {
        path: file.clone(),
        source,
    })?;
    if let Some(expected) = expect {
        let found = hash::bytes(&held);
        if !found.eq_ignore_ascii_case(expected) {
            return Err(LifecycleError::Stale {
                path: file,
                expected: expected.to_owned(),
                found,
            });
        }
    }
    if held == contents {
        return Ok(Edited {
            changed: false,
            copy: Ok(Record::Untouched),
        });
    }
    replace_bytes_unresolved(&resolved, contents)
        .map_err(|(path, source)| LifecycleError::Io { path, source })?;

    let copy = kept(workspace, project, target.kind, &target.name, &target.path);
    Ok(Edited {
        changed: true,
        copy,
    })
}

/// Change how the MCP server `target` in `project` is started or reached,
/// and what is supplied for it, to what `given` says, as `add` would write
/// them from a command line's options; every other key of its definition,
/// its name, description, provenance and extras, kept as the file holds
/// it (`mcp::retransported`). `workspace` is the one consulted, as for
/// [`replace_contents`].
///
/// What `edit` does when it is told a server's options rather than its
/// contents: the edit for a form, which has the options' words and no
/// text. The definition is read as it is now, rewritten, and written as
/// contents given whole are ([`replace_contents`]), held to every rule
/// they are, so it refuses what `validate` would call invalid, a token
/// written as it is among it, and writes nothing when nothing changes.
/// Refused for any other kind, as `add` refuses a transport for one
/// (`edit_transport_not_mcp`), and before it reads the file for what no
/// definition can be written from as given, a credential typed into the
/// address and a key given twice, by `add`'s own checks.
pub fn retransport(
    workspace: Option<&WeaverWorkspace>,
    project: &MindProject,
    target: &Target,
    given: &Given,
) -> Result<Edited, LifecycleError> {
    let text = retransported(target, given)?;
    replace_contents(workspace, project, target, text.as_bytes(), None)
}

/// What [`retransport`] would write into `target`: its definition, read
/// as the file holds it now, rewritten from `given`. Apart from the write
/// so a draft is judged as the write is ([`crate::draft::retransported`]).
pub(crate) fn retransported(target: &Target, given: &Given) -> Result<String, LifecycleError> {
    if target.kind != Kind::Mcp {
        return Err(LifecycleError::TransportNotMcp { kind: target.kind });
    }
    create::writable(given).map_err(LifecycleError::Server)?;
    let file = target.file();
    let text = fs::read_to_string(&file).map_err(|source| LifecycleError::Io {
        path: file.clone(),
        source,
    })?;
    mcp::retransported(&text, given)
        .map_err(|reason| LifecycleError::ServerUnreadable { path: file, reason })
}

/// Where `contents` given whole for `target` in `project` would be written,
/// once every rule [`replace_contents`] holds them to before it reads the
/// file has passed: the target inside its kind's folder, the contents fit
/// for it ([`fit`]), and the file leading nowhere it may not be written
/// ([`written_at`]). In that order, so a refusal is the one the write
/// would give.
///
/// Apart from the write so that what it would be refused for can be asked
/// before anything is sent to be written ([`crate::draft::edited`]), by the
/// same checks rather than a copy of them. What the file holds now, and so
/// whether it is still what an edit was made from, is the write's alone to
/// tell, at the moment it writes.
pub(crate) fn replaceable(
    workspace: Option<&WeaverWorkspace>,
    project: &MindProject,
    target: &Target,
    contents: &[u8],
) -> Result<PathBuf, LifecycleError> {
    let directory = inside(project, target)?;
    fit(target, contents)?;
    written_at(workspace, project, target, &directory)
}

/// Where `target`'s file in `project` leads once its links are followed,
/// when contents given whole may be written there: inside the project's
/// root, the folder `directory` it keeps the kind in, and a skill's own
/// folder. Resolved once and held to every one of them, where
/// [`crate::copy::followed`] asks whether a path lies in any of its
/// bounds; by components, as it compares, so `/work/app-old` is not inside
/// `/work/app`.
///
/// Each for a reason of its own. The root, as a read of a project's file
/// for the panel is held to it ([`crate::contents`]): a kind's folder, or
/// a folder above it such as `.claude`, linked out of the repository would
/// otherwise carry the write along, into an agent's own home among other
/// places. Not for the workspace's own ([`library`]), whose root may be a
/// home with every secret of its user inside it: their folder alone bounds
/// them, as it bounds a read of them. The kind's folder, so a manifest
/// linked to a file elsewhere (`~/.bashrc`, in the workspace's own) is not
/// replaced with a skill. And a skill's own folder, so a manifest linked to
/// a sibling's is not rewritten under a name that folder does not have.
/// The widest first, so a refusal names the most the file leads out of.
fn written_at(
    workspace: Option<&WeaverWorkspace>,
    project: &MindProject,
    target: &Target,
    directory: &Path,
) -> Result<PathBuf, LifecycleError> {
    let root = (!library(workspace, target.kind, directory)).then(|| project.root());
    // Matched on the layout, so a kind kept another way says what bounds it.
    let own = match target.kind.layout() {
        Layout::Directory { .. } => Some(target.path.as_path()),
        Layout::Files { .. } | Layout::Flat { .. } => None,
    };
    let canonical = |path: &Path| {
        fs::canonicalize(path).map_err(|source| LifecycleError::Io {
            path: path.to_path_buf(),
            source,
        })
    };
    let file = target.file();
    let resolved = canonical(&file)?;
    for bound in root.into_iter().chain([directory]).chain(own) {
        if !resolved.starts_with(canonical(bound)?) {
            return Err(LifecycleError::LeadsOut {
                path: file,
                resolved,
                directory: bound.to_path_buf(),
            });
        }
    }
    Ok(resolved)
}

/// Whether `directory`, where a holder keeps `kind`, is the folder
/// `workspace` keeps its own of that kind in ([`WeaverWorkspace::own`]),
/// of a kind it keeps apart from a project at its root
/// ([`WeaverWorkspace::own_kinds`]): the library, as a read of it for the
/// panel finds it ([`crate::contents`]).
///
/// Told by the folder, since the holder is a [`MindProject`] either way:
/// what is in the library's folder is held as the library is, whichever
/// holder reached it, and a project's folder anywhere else to the
/// project's root. One at the workspace's root keeping a kind where the
/// library does holds what is there itself, by `own_kinds`, and is held to
/// its root too. With no workspace there is no library to be.
fn library(workspace: Option<&WeaverWorkspace>, kind: Kind, directory: &Path) -> bool {
    workspace.is_some_and(|workspace| {
        workspace.own().directory_for(kind).as_deref() == Some(directory)
            && workspace.own_kinds().contains(&kind)
    })
}

/// Refuse `contents` for `target` unless they are what Mindweaver would
/// write there: text, and for a skill a manifest that reads, names its
/// folder and passes the strict rules; for a rule, one that says something
/// with front matter that reads whole.
fn fit(target: &Target, contents: &[u8]) -> Result<(), LifecycleError> {
    let name = target.name.clone();
    let Ok(text) = std::str::from_utf8(contents) else {
        return Err(LifecycleError::NotText {
            name: name.to_string(),
        });
    };
    // Matched on the kind, so a third one has to say what it is held to.
    let issues = match target.kind {
        Kind::Skill => {
            let unreadable = |reason: String| LifecycleError::Unparsed {
                name: name.to_string(),
                reason,
            };
            let document =
                frontmatter::split(text).map_err(|error| unreadable(error.to_string()))?;
            let manifest = SkillManifest::parse(document.front_matter)
                .map_err(|error| unreadable(error.to_string()))?;
            // The folder is the name an agent finds it by, so it is what the
            // manifest has to say, whatever the manifest said before.
            let folder = target
                .path
                .file_name()
                .map(|folder| folder.to_string_lossy().into_owned())
                .unwrap_or_default();
            if manifest.name != folder {
                return Err(LifecycleError::NameChanged {
                    name: folder,
                    to: manifest.name,
                });
            }
            artifact::strict_issues(&manifest, &folder)
        }
        Kind::Rule => artifact::rule_text_issues(text),
        Kind::Mcp => {
            let definition =
                McpDefinition::parse(text).map_err(|error| LifecycleError::DefinitionUnparsed {
                    name: name.to_string(),
                    reason: error.to_string(),
                })?;
            // The file is the name an install keys the entry by, so it is
            // what the definition has to say, as a skill's folder is.
            if definition.name != name {
                return Err(LifecycleError::McpNameChanged {
                    name: name.to_string(),
                    to: definition.name.to_string(),
                });
            }
            let file = target
                .path
                .file_name()
                .map(|file| file.to_string_lossy().into_owned())
                .unwrap_or_default();
            let twins = target
                .path
                .parent()
                .map(|folder| twins_beside(folder, &name, &target.path))
                .unwrap_or_default();
            artifact::mcp_strict_issues(&definition, &name, &file, &twins)
        }
    };
    if issues.is_empty() {
        Ok(())
    } else {
        Err(LifecycleError::ContentsUnfit {
            name: name.to_string(),
            issues,
        })
    }
}

/// What an edit, which leaves the record as it is, did to it: kept when
/// `workspace` records a copy at `at`, and untouched when it records none
/// or there is no workspace to ask.
fn kept(
    workspace: Option<&WeaverWorkspace>,
    project: &MindProject,
    kind: Kind,
    name: &str,
    at: &Path,
) -> Result<Record, InstallError> {
    let Some(workspace) = workspace else {
        return Ok(Record::Untouched);
    };
    install::recorded_at(workspace, project, kind, name, at).map(|recorded| {
        if recorded {
            Record::Kept
        } else {
            Record::Untouched
        }
    })
}

/// How many files removing it would delete.
pub fn files(target: &Target) -> usize {
    fn count(path: &Path) -> usize {
        match fs::read_dir(path) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .map(|entry| match entry.file_type() {
                    Ok(kind) if kind.is_dir() => count(&entry.path()),
                    _ => 1,
                })
                .sum(),
            Err(_) => 1,
        }
    }
    match target.kind.layout() {
        Layout::Directory { .. } => count(&target.path),
        Layout::Files { .. } | Layout::Flat { .. } => 1,
    }
}

/// What removing an artifact did to the record of an install of it.
#[derive(Debug)]
pub struct Removed {
    /// Dropped when the copy had been installed: removing it uninstalls it,
    /// and the record would be inherited by whatever is written under its
    /// name next. Left in place, it would read as a copy missing, which the
    /// next install would put back.
    pub copy: Result<Record, InstallError>,
    /// What opening the ledger to see to that record scrubbed out of it.
    pub scrub: Scrub,
}

/// Delete an artifact: a skill's whole directory, or a rule's file and any
/// folder it leaves empty. Then drop the record of an install of it, as an
/// uninstall of that copy would. `workspace` is the one consulted, as for
/// [`add`].
pub fn remove(
    workspace: Option<&WeaverWorkspace>,
    project: &MindProject,
    target: &Target,
) -> Result<Removed, LifecycleError> {
    let directory = inside(project, target)?;
    match target.kind.layout() {
        Layout::Directory { .. } => {
            fs::remove_dir_all(&target.path).map_err(|source| LifecycleError::Io {
                path: target.path.clone(),
                source,
            })?;
        }
        Layout::Files { .. } => {
            fs::remove_file(&target.path).map_err(|source| LifecycleError::Io {
                path: target.path.clone(),
                source,
            })?;
            prune(&directory, target.path.parent());
        }
        // Directly in the kind's folder, so no folder of its own is left.
        Layout::Flat { .. } => {
            fs::remove_file(&target.path).map_err(|source| LifecycleError::Io {
                path: target.path.clone(),
                source,
            })?;
        }
    }
    let mut scrub = Scrub::default();
    let copy = disowned(
        workspace,
        project,
        target.kind,
        &target.name,
        &target.path,
        &mut scrub,
    );
    Ok(Removed { copy, scrub })
}

/// What renaming an artifact made, and what it did to the records under its
/// old name and its new one.
#[derive(Debug)]
pub struct Renamed {
    /// The artifact under its new name.
    pub target: Target,
    /// The record of the copy under its old name, carried to its new name and
    /// place, saying whether it reads as edited now ([`Record::Moved`]): it
    /// stays what was installed. Or dropped, where another installation
    /// holds the new name already.
    pub copy: Result<Record, InstallError>,
    /// A record under the new name, from an install that is no longer there:
    /// stale, and left in place it would make what was just renamed look like
    /// something Mindweaver put there.
    pub stale: Result<Record, InstallError>,
    /// What opening the ledger to see to both records scrubbed out of it,
    /// once: the second opening finds only what the first could not take out,
    /// which is the same thing to tell.
    pub scrub: Scrub,
}

/// Give an artifact a new name: a skill's folder and the `name` it declares
/// together, so it still passes `validate`; a rule's file, filed by its new
/// route.
///
/// In a skill's manifest the `name` line is the only thing rewritten. The rest
/// is somebody's writing (comments, key order, the description) and is left
/// byte for byte, and the result is parsed back to make sure the one line
/// changed what it was meant to and nothing else.
///
/// Then a stale record under the new name is dropped, and the record of the
/// copy renamed carried along with it. `workspace` is the one consulted, as
/// for [`add`].
pub fn rename(
    workspace: Option<&WeaverWorkspace>,
    project: &MindProject,
    target: &Target,
    to: &str,
) -> Result<Renamed, LifecycleError> {
    // Matched on the kind, since a server's name is held to the keys an
    // agent takes rather than to the kebab-case of the others, and only
    // once it is known to hold no folder ([`rename_mcp`]), which says so in
    // words of its own.
    let issues = match target.kind {
        Kind::Skill | Kind::Rule => artifact::name_issues(to),
        Kind::Mcp => Vec::new(),
    };
    if !issues.is_empty() {
        return Err(LifecycleError::Name {
            name: to.to_owned(),
            issues,
        });
    }
    let directory = inside(project, target)?;
    // Matched on the kind, so a third one has to say how it is renamed.
    let renamed = match target.kind {
        Kind::Skill => rename_skill(project, target, &directory, to),
        Kind::Rule => rename_rule(project, target, &directory, to),
        Kind::Mcp => rename_mcp(project, target, &directory, to),
    }?;
    let mut scrub = Scrub::default();
    // The stale one first: a record of an install gone from the new place
    // would hold the new name, and the copy could not be carried to it.
    let stale = disowned(
        workspace,
        project,
        target.kind,
        &renamed.name,
        &renamed.path,
        &mut scrub,
    );
    let copy = carried(
        workspace,
        project,
        target.kind,
        (&target.name, &target.path),
        (&renamed.name, &renamed.path),
        &mut scrub,
    );
    Ok(Renamed {
        target: renamed,
        copy,
        stale,
        scrub,
    })
}

fn rename_skill(
    project: &MindProject,
    target: &Target,
    directory: &Path,
    to: &str,
) -> Result<Target, LifecycleError> {
    if to.contains('/') {
        return Err(LifecycleError::Nested {
            name: to.to_owned(),
        });
    }
    // The name lives in the front matter too, and front matter that does not
    // parse cannot be trusted to have its name rewritten.
    let Some(artifact) = &target.artifact else {
        return Err(LifecycleError::Unreadable {
            path: target.file(),
        });
    };
    // Held to the strict rules as it will be, the rules `add` and `install`
    // hold a skill to: what is left behind is a skill Mindweaver wrote, if
    // only its name, and one an agent would skip is not left behind. Judged as
    // renamed, because its name and folder are what a rename fixes.
    if let Some(manifest) = artifact.manifest() {
        let renamed = SkillManifest {
            name: to.to_owned(),
            ..manifest.clone()
        };
        let issues = artifact::strict_issues(&renamed, to);
        if !issues.is_empty() {
            return Err(LifecycleError::Unfit {
                name: artifact.name().to_string(),
                issues,
            });
        }
    }

    let folder = directory.join(to);
    let moves = folder != target.path;
    let renames = artifact.name() != to;
    if !moves && !renames {
        return Err(LifecycleError::Unchanged {
            name: to.to_owned(),
        });
    }
    if moves && occupied(&folder) {
        return Err(LifecycleError::Exists { path: folder });
    }

    let manifest = target.file();
    let rewritten = if renames {
        let source = fs::read_to_string(&manifest).map_err(|source| LifecycleError::Io {
            path: manifest.clone(),
            source,
        })?;
        Some(
            rewrite_name(&source, to).map_err(|reason| LifecycleError::Rewrite {
                path: manifest.clone(),
                reason,
            })?,
        )
    } else {
        None
    };

    if moves {
        fs::rename(&target.path, &folder).map_err(|source| LifecycleError::Io {
            path: target.path.clone(),
            source,
        })?;
    }
    if let Some(text) = rewritten {
        if let Err(error) = replace_file(&file_of(Kind::Skill, &folder), &text) {
            // Half a rename is a skill whose folder and name disagree: put the
            // folder back rather than leave that behind.
            if moves {
                let _ = fs::rename(&folder, &target.path);
            }
            return Err(error.into());
        }
    }

    let artifact = Artifact::skill(&folder, project.root())?;
    Ok(Target {
        kind: Kind::Skill,
        name: artifact.name().to_owned(),
        path: folder,
        artifact: Some(artifact),
    })
}

fn rename_rule(
    project: &MindProject,
    target: &Target,
    directory: &Path,
    to: &str,
) -> Result<Target, LifecycleError> {
    let file = place_in(directory, Kind::Rule, to);
    if file == target.path {
        return Err(LifecycleError::Unchanged {
            name: to.to_owned(),
        });
    }
    if occupied(&file) {
        return Err(LifecycleError::Exists { path: file });
    }
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(|source| LifecycleError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    fs::rename(&target.path, &file).map_err(|source| LifecycleError::Io {
        path: target.path.clone(),
        source,
    })?;
    prune(directory, target.path.parent());

    // A rule's front matter holds no name, so there is nothing inside it to
    // rewrite, and one that did not read before a move does not read after it
    // either.
    Ok(Target {
        kind: Kind::Rule,
        name: ArtifactName::from(to),
        artifact: Artifact::rule(&file, to.to_owned(), project.root()).ok(),
        path: file,
    })
}

/// Give an MCP server's definition a new name: its file and the `name` it
/// declares together, so it still passes `validate`, as a skill's folder
/// and its manifest are renamed.
fn rename_mcp(
    project: &MindProject,
    target: &Target,
    directory: &Path,
    to: &str,
) -> Result<Target, LifecycleError> {
    // A key in an agent's config holds no folder, so neither does the name
    // an entry will be keyed by.
    if to.contains('/') {
        return Err(LifecycleError::McpNested {
            name: to.to_owned(),
        });
    }
    // Beside every definition but the one renamed, so that a name changed
    // only in case is its own and not its twin.
    let twins = twins_beside(directory, to, &target.path);
    let issues = artifact::mcp_name_strict_issues(to, &twins);
    if !issues.is_empty() {
        return Err(LifecycleError::Name {
            name: to.to_owned(),
            issues,
        });
    }
    // The name lives in the definition too, and one that does not read
    // cannot be trusted to have its name rewritten.
    let Some(definition) = target.artifact.as_ref().and_then(Artifact::definition) else {
        return Err(LifecycleError::DefinitionUnreadable {
            path: target.file(),
        });
    };
    let file = place_in(directory, Kind::Mcp, to);
    let moves = file != target.path;
    let renames = definition.name != to;
    if !moves && !renames {
        return Err(LifecycleError::Unchanged {
            name: to.to_owned(),
        });
    }
    if moves && occupied(&file) {
        return Err(LifecycleError::Exists { path: file });
    }

    let rewritten = if renames {
        let source = fs::read_to_string(&target.path).map_err(|source| LifecycleError::Io {
            path: target.path.clone(),
            source,
        })?;
        Some(crate::mcp::declaring(&source, to, None).map_err(|reason| {
            LifecycleError::Rewrite {
                path: target.path.clone(),
                reason,
            }
        })?)
    } else {
        None
    };
    // Held to what `validate` calls a definition invalid for, as it will
    // be once renamed, as a skill is held to the strict rules: what is left
    // behind is a definition Mindweaver wrote, if only its name.
    let renamed = named(definition, to);
    let file_name = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let issues = artifact::mcp_strict_issues(&renamed, to, &file_name, &twins);
    if !issues.is_empty() {
        return Err(LifecycleError::Unfit {
            name: target.name.to_string(),
            issues,
        });
    }

    if moves {
        fs::rename(&target.path, &file).map_err(|source| LifecycleError::Io {
            path: target.path.clone(),
            source,
        })?;
    }
    if let Some(text) = rewritten {
        if let Err(error) = replace_file(&file, &text) {
            // Half a rename is a definition whose file and name disagree: put
            // the file back rather than leave that behind.
            if moves {
                let _ = fs::rename(&file, &target.path);
            }
            return Err(error.into());
        }
    }

    let artifact = Artifact::mcp(&file, to.to_owned(), project.root())?;
    Ok(Target {
        kind: Kind::Mcp,
        name: ArtifactName::from(to),
        path: file,
        artifact: Some(artifact),
    })
}

/// `definition`, declaring `to` as its name.
fn named(definition: &McpDefinition, to: &str) -> McpDefinition {
    McpDefinition {
        name: ArtifactName::from(to),
        ..definition.clone()
    }
}

/// A manifest with its `name` set to `to` and every other byte as it was.
fn rewrite_name(source: &str, to: &str) -> Result<String, String> {
    let document = frontmatter::split(source).map_err(|error| error.to_string())?;
    let front_matter = document.front_matter;
    let before = SkillManifest::parse(front_matter).map_err(|error| error.to_string())?;

    // Serialized, so a name YAML would read as something else (`true`,
    // `123`) is quoted.
    let value = serde_yaml_ng::to_string(to).map_err(|error| error.to_string())?;
    let rewritten = frontmatter::with_key(front_matter, "name", value.trim_end())
        .ok_or_else(|| String::from("its `name` is not on a line of its own"))?;

    let after = SkillManifest::parse(&rewritten).map_err(|error| error.to_string())?;
    let wanted = SkillManifest {
        name: to.to_owned(),
        ..before
    };
    if after != wanted {
        return Err(String::from(
            "rewriting that line would change more than the name",
        ));
    }

    // The front matter is a slice of `source`, so where it starts in `source`
    // is where the rewritten one goes: the fences, and anything before them,
    // stay exactly as they were.
    let start = front_matter.as_ptr() as usize - source.as_ptr() as usize;
    Ok(format!(
        "{}{rewritten}{}",
        &source[..start],
        &source[start + front_matter.len()..]
    ))
}

/// The folder a target's kind lives in, having checked the target is inside
/// it: nothing here deletes or moves anything anywhere else.
fn inside(project: &MindProject, target: &Target) -> Result<PathBuf, LifecycleError> {
    let directory = project.required_directory(target.kind)?;
    if target.path == directory || !target.path.starts_with(&directory) {
        return Err(LifecycleError::Outside {
            path: target.path.clone(),
            directory,
        });
    }
    Ok(directory)
}

/// The names of the MCP servers' definitions in `folder` that are `name` in
/// another case, the one at `file` left out: what a definition written
/// there under `name` is held not to sit beside, as discovery holds the
/// ones it finds ([`crate::mcp::name::twins`]), read off the same listing.
pub(crate) fn twins_beside(folder: &Path, name: &str, file: &Path) -> Vec<ArtifactName> {
    let (files, _) = Catalog::locate(folder, Kind::Mcp);
    let stems: Vec<String> = files
        .iter()
        .filter(|other| other.as_path() != file)
        .filter_map(|other| other.file_stem()?.to_str().map(str::to_owned))
        .collect();
    crate::mcp::name::twins(name, stems.iter().map(String::as_str))
}

/// Whether anything at all is at `path`, a dangling symlink included.
fn occupied(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Remove the folders a rule leaves empty, up to but not including the folder
/// the kind lives in. Folders under it only group, so an empty one groups
/// nothing. An uninstall of a rule's copy prunes the same way
/// ([`install::uninstall`]).
pub(crate) fn prune(root: &Path, from: Option<&Path>) {
    let mut current = from;
    while let Some(folder) = current {
        if folder == root || !folder.starts_with(root) || fs::remove_dir(folder).is_err() {
            break;
        }
        current = folder.parent();
    }
}

/// The file an artifact at `path` is read from.
fn file_of(kind: Kind, path: &Path) -> PathBuf {
    match kind.layout() {
        Layout::Directory { manifest } => path.join(manifest),
        Layout::Files { .. } | Layout::Flat { .. } => path.to_path_buf(),
    }
}

/// Whether an artifact of `kind` sits at `path`.
fn holds(kind: Kind, path: &Path) -> bool {
    file_of(kind, path).is_file()
}

/// Drop the record `workspace` keeps of an install of `name` into `project`
/// that names the copy at `at`, the one the change reached, adding what
/// opening the ledger to do it scrubbed out of it to `scrub`.
fn disowned(
    workspace: Option<&WeaverWorkspace>,
    project: &MindProject,
    kind: Kind,
    name: &str,
    at: &Path,
    scrub: &mut Scrub,
) -> Result<Record, InstallError> {
    let Some(workspace) = workspace else {
        return Ok(Record::Untouched);
    };
    let disowned = install::disown(workspace, project, kind, name, at)?;
    scrub.absorb(disowned.scrub);
    if let Some(error) = disowned.still_excluded {
        return Err(error);
    }
    Ok(if disowned.dropped {
        Record::Dropped
    } else {
        Record::Untouched
    })
}

/// Carry the record `workspace` keeps of an install of `name` into `project`
/// that names the copy at `at` along to `to`, a name and the place a rename
/// moved it to, adding what opening the ledger to do it scrubbed out of it to
/// `scrub`.
fn carried(
    workspace: Option<&WeaverWorkspace>,
    project: &MindProject,
    kind: Kind,
    (name, at): (&str, &Path),
    to: (&str, &Path),
    scrub: &mut Scrub,
) -> Result<Record, InstallError> {
    let Some(workspace) = workspace else {
        return Ok(Record::Untouched);
    };
    let carry = install::carry(workspace, project, kind, name, at, to)?;
    scrub.absorb(carry.scrub);
    if let Some(error) = carry.unexcluded {
        return Err(error);
    }
    Ok(match carry.carried {
        Carried::Nothing => Record::Untouched,
        Carried::Moved => Record::Moved {
            edited: carry.edited,
        },
        Carried::Forgotten => Record::Dropped,
    })
}

/// Why an artifact could not be found, edited, renamed or removed.
#[derive(Debug, Error)]
pub enum LifecycleError {
    #[error("nothing named `{typed}` here")]
    NotFound { typed: String },
    /// Not authored in the project but received: a rule rendered into it
    /// from where it was installed from ([`find_to_change`]).
    #[error("`{typed}` is a rule {project} received, rendered from where it was installed from rather than kept in it, so it is not changed from here: uninstall it, or change it where it is offered and install it again")]
    Received { typed: String, project: String },
    /// Not authored in the project but received: an MCP server written into
    /// its agents' config files from the definition it was installed from
    /// ([`find_to_change`]). Uninstalled by `named`, however it was typed:
    /// its name bare where an uninstall finds the server by it, and
    /// qualified as a server's where the ledger records it as another kind
    /// too, which a bare name would be refused for as two.
    #[error("`{typed}` is an MCP server {project} received, an entry in its agents' config files written from the definition it was installed from rather than kept in it, so it is not changed from here: uninstall it as `{named}`, or change its definition in the library it came from and update")]
    ReceivedServer {
        typed: String,
        project: String,
        named: String,
    },
    /// Listed, from a folder the project's agents read, but in none the
    /// project authors in: installed there, or somebody's own.
    #[error("`{typed}` is at {path}, in a folder this project's agents read and it does not author in, so it is not changed from here")]
    NotAuthored { typed: String, path: PathBuf },
    #[error("`{typed}` names more than one thing ({}), so say which: by kind, as in `skill:name`, or by folder", .matches.join(", "))]
    Ambiguous { typed: String, matches: Vec<String> },
    #[error("{}", issue_list(.issues))]
    Name {
        name: String,
        issues: Vec<ValidationIssue>,
    },
    #[error("`{name}`: a skill's name cannot contain `/`; skills are flat, and only rules are filed in folders")]
    Nested { name: String },
    #[error("`{name}`: an MCP server's name cannot contain `/`; it is the key an agent's config names the server by, which holds no folder")]
    McpNested { name: String },
    #[error("`{name}` cannot be renamed until it is fixed: {}", issue_list(.issues))]
    Unfit {
        name: String,
        issues: Vec<ValidationIssue>,
    },
    #[error("it is already called `{name}`")]
    Unchanged { name: String },
    #[error("{path}: already exists, and is left alone")]
    Exists { path: PathBuf },
    #[error("{path}: its front matter cannot be read, so its name cannot be rewritten; fix it with `mind edit` first")]
    Unreadable { path: PathBuf },
    #[error("{path}: it cannot be read as an MCP server's definition, so its name cannot be rewritten; fix it with `mind edit` first")]
    DefinitionUnreadable { path: PathBuf },
    #[error("{path}: the name cannot be rewritten on its own ({reason}); change it by hand")]
    Rewrite { path: PathBuf, reason: String },
    #[error("{path}: not inside {directory}, so it is not changed from here")]
    Outside { path: PathBuf, directory: PathBuf },
    /// Contents given whole ([`replace_contents`]) that are no text, and so
    /// no manifest or rule any agent reads.
    #[error("the contents given for `{name}` are not UTF-8 text, so nothing was written")]
    NotText { name: String },
    /// A skill's contents given whole whose front matter does not read.
    #[error("the contents given for `{name}` cannot be read as a skill's, so nothing was written: {reason}")]
    Unparsed { name: String, reason: String },
    /// A skill's contents given whole that name it other than its folder.
    #[error("the contents given for `{name}` call it `{to}`: a skill's name is its folder's too, so it is changed with `mind rename`, which moves both, and nothing was written")]
    NameChanged { name: String, to: String },
    /// An MCP server's contents given whole that do not read as a
    /// definition.
    #[error("the contents given for `{name}` cannot be read as an MCP server's definition, so nothing was written: {reason}")]
    DefinitionUnparsed { name: String, reason: String },
    /// An MCP server's contents given whole that name it other than its
    /// file.
    #[error("the contents given for `{name}` call it `{to}`: an MCP server's name is its file's too, so it is changed with `mind rename`, which changes both, and nothing was written")]
    McpNameChanged { name: String, to: String },
    /// Contents given whole that break a rule Mindweaver holds a write to.
    #[error("the contents given for `{name}` break a rule, so nothing was written: {}", issue_list(.issues))]
    ContentsUnfit {
        name: String,
        issues: Vec<ValidationIssue>,
    },
    /// The hash the contents given whole were said to be made from is no
    /// SHA-256 at all, so no file could hold what it is the hash of.
    #[error("`{given}` is not a SHA-256, which is 64 hex digits and nothing else, so nothing was written")]
    HashMalformed { given: String },
    /// The file no longer holds what the contents given whole were made
    /// from: somebody else changed it since it was read.
    #[error("{path}: changed since it was read, its SHA-256 now {found} and not {expected}, so it is not written over; read it again")]
    Stale {
        path: PathBuf,
        expected: String,
        found: ContentHash,
    },
    /// The file leads, through a link, out of `directory`, one of the
    /// folders contents given whole are written inside: the project's
    /// root, the folder its kind is kept in, or a skill's own.
    #[error("{path} leads to {resolved}, outside {directory}, so it is not written")]
    LeadsOut {
        path: PathBuf,
        resolved: PathBuf,
        directory: PathBuf,
    },
    #[error("{path}: cannot be changed: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// A server's transport given for a kind no agent starts.
    #[error(
        "`--command` and `--url` say how an MCP server is started or reached, and a {} is \
         neither started nor reached",
        .kind.noun()
    )]
    TransportNotMcp { kind: Kind },
    /// A server whose definition does not read, so what is its transport
    /// and what is the rest cannot be told apart to write the one again.
    #[error("{path}: it cannot be read as an MCP server's definition ({reason}), so how it is started cannot be written again; fix it with `mind edit` first")]
    ServerUnreadable { path: PathBuf, reason: String },
    /// What `add` refuses of a transport as given, refused by its checks.
    #[error(transparent)]
    Server(CreateError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
}

codes!(LifecycleError {
    Self::NotFound { .. } => "not_found",
    Self::Received { .. } => "change_received_rule",
    Self::ReceivedServer { .. } => "change_received_mcp",
    Self::NotAuthored { .. } => "change_not_authored",
    Self::Ambiguous { .. } => "ambiguous_name",
    Self::Name { .. } => "rename_name_unfit",
    Self::Nested { .. } => "rename_skill_nested",
    Self::McpNested { .. } => "rename_mcp_nested",
    Self::Unfit { .. } => "rename_unfit",
    Self::Unchanged { .. } => "rename_unchanged",
    Self::Exists { .. } => "rename_target_exists",
    Self::Unreadable { .. } => "rename_front_matter_unreadable",
    Self::DefinitionUnreadable { .. } => "rename_definition_unreadable",
    Self::Rewrite { .. } => "rename_rewrite_failed",
    Self::Outside { .. } => "change_outside",
    Self::NotText { .. } => "edit_contents_not_text",
    Self::Unparsed { .. } => "edit_front_matter_unreadable",
    Self::NameChanged { .. } => "edit_name_changed",
    Self::DefinitionUnparsed { .. } => "edit_definition_unreadable",
    Self::McpNameChanged { .. } => "edit_mcp_name_changed",
    Self::ContentsUnfit { .. } => "edit_contents_unfit",
    Self::HashMalformed { .. } => "edit_hash_malformed",
    Self::Stale { .. } => "edit_stale",
    Self::LeadsOut { .. } => "edit_leads_outside",
    Self::Io { .. } => "change_failed",
    Self::TransportNotMcp { .. } => "edit_transport_not_mcp",
    Self::ServerUnreadable { .. } => "edit_server_unreadable",
} carried {
    Self::Server(error) => error,
    Self::Workspace(error) => error,
    Self::Artifact(error) => error,
});

#[cfg(test)]
#[path = "lifecycle_test.rs"]
mod tests;
