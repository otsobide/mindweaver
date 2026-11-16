//! Keeping paths out of git without touching the team's `.gitignore`.
//!
//! A project in local mode keeps what Mindweaver installs into it out of git
//! (D-02), and the file that can say so without being committed is
//! `$GIT_COMMON_DIR/info/exclude`: git reads it like a `.gitignore` but never
//! tracks it, and every linked worktree of the repository reads the same one.
//! Mindweaver writes there only between `# mindweaver:begin` and
//! `# mindweaver:end`, one group per project:
//!
//! ```text
//! # mindweaver:begin
//! # project 4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5
//! /packages/app/.agents/skills/commit-style/
//! /packages/app/.claude/skills/commit-style/
//! # project 0123456789abcdef0123456789abcdef
//! /packages/web/.agents/skills/review/
//! # mindweaver:end
//! ```
//!
//! Several projects can share one repository, so each owns a group under its
//! own header and an edit for one never rewrites another's lines. Everything
//! outside the block is somebody else's and comes back byte for byte.
//!
//! A block a Mindflayer build wrote opens with `# mindflayer:begin` (D-26,
//! [`Names`]): it is read as the block, and edited under its own markers,
//! as `AGENTS.md`'s is ([`crate::agents_md`]); only a block made anew says
//! `mindweaver:`, and only `migrate --names` moves an old one to it
//! ([`ExcludeFile::renamed`]). One block a file, whichever its markers.
//!
//! Not `.gitignore`, which is the team's: it is committed, so a line there
//! puts copies one machine has into every teammate's diff. Not
//! `core.excludesFile` either, which is the user's and spans every
//! repository they have. Mindweaver edits neither, ever.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use crate::code::codes;
use crate::lines::{self, Line};
use crate::names::Names;
use crate::paths;
use crate::workspace::replace_bytes;

/// The line that opens Mindweaver's block.
pub const BEGIN: &str = "# mindweaver:begin";
/// The line that closes it.
pub const END: &str = "# mindweaver:end";
/// [`BEGIN`] as a Mindflayer build wrote it, read and kept in a block it
/// wrote ([`Names::Mindflayer`]).
pub const FLAYER_BEGIN: &str = "# mindflayer:begin";
/// [`END`] as a Mindflayer build wrote it.
pub const FLAYER_END: &str = "# mindflayer:end";

/// The two markers of a block spelled under `names`.
const fn markers(names: Names) -> (&'static str, &'static str) {
    match names {
        Names::Mindweaver => (BEGIN, END),
        Names::Mindflayer => (FLAYER_BEGIN, FLAYER_END),
    }
}
/// What opens one project's group inside the block, before the project's key.
pub const GROUP: &str = "# project ";

/// Keep `entries` of the project at `project` out of git, in the group
/// headed `key`.
///
/// `key` is whatever names the project for good, which for an install is
/// the id its settings hold: it stays when the project is renamed or moved.
/// It is only compared, never parsed.
pub fn add(project: &Path, key: &str, entries: &[Entry]) -> Result<Excluded, ExcludeError> {
    change(project, key, entries, ExcludeFile::add)
}

/// Stop keeping `entries` of the project at `project` out of git, dropping
/// its group when that empties it.
pub fn remove(project: &Path, key: &str, entries: &[Entry]) -> Result<Excluded, ExcludeError> {
    change(project, key, entries, ExcludeFile::remove)
}

/// Drop the group headed `key` whole, whatever it holds: for a project that
/// stops keeping what is installed into it out of git, as one that turns
/// shared does (D-02).
///
/// Every line, not only those of the copies one workspace's ledger records:
/// a group is keyed by the project, and several workspaces can install into
/// one project, each line naming a copy that is now the team's to commit.
pub fn forget(project: &Path, key: &str) -> Result<Excluded, ExcludeError> {
    change(project, key, &[], |file, key, _| file.forget(key))
}

/// What the group headed `key` holds, in the exclude file git reads for the
/// work tree `project` is in: `None` outside a git work tree. A file, a
/// block or a group that is not there holds nothing.
///
/// For a front end to say what [`forget`] would take out, and which of the
/// lines an [`Entry`] would write are there already, before anything is
/// changed. A block that cannot be read is refused, as an edit refuses it.
pub fn listed(project: &Path, key: &str) -> Result<Option<Listing>, ExcludeError> {
    check(key, &[])?;
    let Some(location) = locate(project)? else {
        return Ok(None);
    };
    let file = read(&location.file)?;
    Ok(Some(Listing {
        lines: file.lines_of(key),
        file: location.file,
        route: location.route,
    }))
}

/// Move the lines of the group headed `from_key` that name paths in the
/// project at `from` into the group headed `to_key`, naming the same paths
/// in the project at `to`: for a project whose id changed
/// (`mind init --new-id`), which keeps its place and takes a new key, and
/// for one that moved (`mindweaver relink`), which keeps its key and takes a new
/// place, in the same work tree or another.
///
/// A line is the project's when it names a path below the project's own
/// directory ([`Entry::pattern`] writes every one that way), so a copy of
/// the project elsewhere in the work tree, holding the same id, keeps its
/// lines under the old key: those name its copies, not this project's.
/// Each is anchored again under `to` as it was under `from`, everything
/// after the project's own route kept as written.
///
/// Neither need be there any more: `from` as a project that moved is not,
/// and `to` as where it was, when the lines go back there because its
/// entry could not follow it ([`crate::relink`]). The exclude file of a
/// place that is gone is the one git reads for the folder nearest it that
/// still is, and its route from there what it was. Added where they go
/// before they are taken from where they were, so a copy is not offered to
/// git between the two, and nothing is written when no line is the
/// project's. Outside a work tree at `to`, the lines are taken out and none
/// is written, which [`Followed::warning`] says.
pub fn carry(
    from: &Path,
    from_key: &str,
    to: &Path,
    to_key: &str,
) -> Result<Followed, ExcludeError> {
    check(from_key, &[])?;
    check(to_key, &[])?;
    let Some(source) = locate_reaching(from)? else {
        return Ok(Followed::Nothing);
    };
    let below = format!("{}/", anchored(&source.route)?);
    let mut source_file = read(&source.file)?;
    let lines: Vec<String> = source_file
        .lines_of(from_key)
        .into_iter()
        .filter(|line| line.starts_with(&below))
        .collect();
    if lines.is_empty() {
        return Ok(Followed::Nothing);
    }

    let Some(target) = locate_reaching(to)? else {
        source_file.remove(from_key, &lines)?;
        write(&source.file, &source_file)?;
        return Ok(Followed::Outside {
            project: to.to_path_buf(),
        });
    };
    let there = format!("{}/", anchored(&target.route)?);
    if target.file == source.file && from_key == to_key && there == below {
        return Ok(Followed::Nothing);
    }
    let moved: Vec<String> = lines
        .iter()
        .map(|line| format!("{there}{}", &line[below.len()..]))
        .collect();
    if target.file == source.file {
        source_file.remove(from_key, &lines)?;
        source_file.add(to_key, &moved)?;
        write(&source.file, &source_file)?;
    } else {
        let mut target_file = read(&target.file)?;
        target_file.add(to_key, &moved)?;
        write(&target.file, &target_file)?;
        source_file.remove(from_key, &lines)?;
        write(&source.file, &source_file)?;
    }
    Ok(Followed::Written { file: target.file })
}

/// What [`carry`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Followed {
    /// No line of the group was the project's, and nothing was written.
    Nothing,
    /// The lines are in `file`, under the new key and place.
    Written { file: PathBuf },
    /// No git work tree holds the project where it is now, so its lines were
    /// taken out of the file they were in and written nowhere.
    Outside { project: PathBuf },
}

impl Followed {
    /// What a front end should say, if anything, as [`Excluded::warning`]
    /// says it.
    pub fn warning(&self) -> Option<String> {
        match self {
            Followed::Outside { project } => Excluded::Outside {
                project: project.clone(),
            }
            .warning(),
            Followed::Nothing | Followed::Written { .. } => None,
        }
    }
}

/// [`locate`], for a project that may not be there any more: from the
/// nearest folder above it that is, its route from that one's work tree
/// carried on below it. `None` when no folder above it is in a work tree.
fn locate_reaching(project: &Path) -> Result<Option<Location>, ExcludeError> {
    let project =
        paths::normalize(
            &std::path::absolute(project).map_err(|_| ExcludeError::Path {
                path: project.to_path_buf(),
            })?,
        );
    let mut at = project.as_path();
    let mut below: Vec<&std::ffi::OsStr> = Vec::new();
    while fs::symlink_metadata(at).is_err() {
        let (Some(parent), Some(name)) = (at.parent(), at.file_name()) else {
            return Ok(None);
        };
        below.push(name);
        at = parent;
    }
    let Some(mut location) = locate(at)? else {
        return Ok(None);
    };
    for name in below.into_iter().rev() {
        location.route.push(name);
    }
    Ok(Some(location))
}

/// Write `file` to `path`, its `info` folder made first when it is not
/// there, as [`change`] writes it.
fn write(path: &Path, file: &ExcludeFile) -> Result<(), ExcludeError> {
    if let Some(info) = path.parent() {
        fs::create_dir_all(info).map_err(|source| ExcludeError::Write {
            path: info.to_path_buf(),
            source,
        })?;
    }
    replace_bytes(path, &file.to_bytes())
        .map_err(|(path, source)| ExcludeError::Write { path, source })
}

/// What one project's group holds, and where: [`listed`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    /// The exclude file, which need not exist yet.
    pub file: PathBuf,
    /// The project's directory relative to the work tree root, as
    /// [`Location`] has it: what [`Entry::pattern`] is given to write a
    /// line the group would hold.
    pub route: PathBuf,
    /// Every line of the group but its header and blank ones, as written.
    pub lines: Vec<String>,
}

/// What [`add`] or [`remove`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Excluded {
    /// The exclude file was rewritten.
    Written { file: PathBuf },
    /// It already said so, and was left alone.
    Unchanged { file: PathBuf },
    /// No git work tree holds the project, so nothing was written.
    ///
    /// Not a failure, since a project need not be in git, but not silent
    /// either: what local mode installs is then kept out of nothing, and a
    /// repository made there later would offer all of it to be committed.
    /// [`Excluded::warning`] says so.
    Outside { project: PathBuf },
}

impl Excluded {
    /// What a front end should warn about, if anything.
    pub fn warning(&self) -> Option<String> {
        match self {
            Excluded::Outside { project } => Some(format!(
                "{}: not in a git work tree, so nothing keeps what is installed there out of git",
                project.display()
            )),
            Excluded::Written { .. } | Excluded::Unchanged { .. } => None,
        }
    }
}

/// Where a project's entries are kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// `info/exclude` in the repository's common dir, which need not exist
    /// yet.
    pub file: PathBuf,
    /// The project's directory relative to the work tree root: `.` at the
    /// root.
    pub route: PathBuf,
}

/// The exclude file git reads for the work tree `project` is in, and where
/// in that work tree it is. `None` when no git work tree holds it.
///
/// The common dir, not the git dir: a linked worktree has a git dir of its
/// own under the main repository's `.git/worktrees/`, but git reads
/// `info/exclude` only from the common dir, so every worktree shares it.
///
/// The repository is opened isolated, reading neither the environment nor
/// the user's git config. Core never reads the environment, and `GIT_DIR`
/// pointing somewhere else must not send one project's entries into another
/// repository's file.
pub fn locate(project: &Path) -> Result<Option<Location>, ExcludeError> {
    let Some((repository, route)) = discover(project)? else {
        return Ok(None);
    };
    // A linked worktree names its common dir from its own git dir, as
    // `../..`, and that is how gix hands it back. Normalized, so what a
    // front end reports is the file a person would go and open.
    Ok(Some(Location {
        file: paths::normalize(&repository.common_dir().join("info").join("exclude")),
        route,
    }))
}

/// Whether git tracks anything inside `folder`, a route from the project at
/// `project`: whether the index of the work tree holding the project names
/// a file there. Never, outside a git work tree.
///
/// For a front end to say what no ignore rule does: a rule keeps out of git
/// only what git does not track yet, so a `.mind` committed by design, as a
/// `mind.toml` project's was, stays in the index once a `.gitignore` asks
/// for it to be left out, until somebody runs `git rm --cached` on it. Read
/// from the index with gix, the repository opened as [`locate`] opens it,
/// and never written: Mindweaver runs no git command, and taking a team's
/// files out of the index is not its to do.
pub fn tracks(project: &Path, folder: &Path) -> Result<bool, ExcludeError> {
    in_index(project, folder, |index, path| {
        // Ending in a slash, as the index spells a folder's files, so
        // `.mind` does not take in a `.mindweaver` beside it.
        let prefix = format!("{path}/");
        index
            .prefixed_entries(prefix.as_bytes().into())
            .is_some_and(|entries| !entries.is_empty())
    })
}

/// Whether git tracks the file at `file`, a route from the project at
/// `project`: whether the index of the work tree holding the project names
/// it. Never, outside a git work tree.
///
/// For a team file Mindweaver would edit to reach an agent, which a local
/// project leaves alone while git tracks it, unless its settings say
/// `edit_tracked` (D-02), as a project's `CLAUDE.md`. Read from the index
/// as [`tracks`] reads it, so a file added and not yet committed is tracked,
/// as git says it is.
pub fn tracks_file(project: &Path, file: &Path) -> Result<bool, ExcludeError> {
    in_index(project, file, |index, path| {
        index.entry_by_path(path.as_bytes().into()).is_some()
    })
}

/// What `look` finds in the index of the work tree holding `project`, given
/// `inside`, a route from the project, as the index spells it from the work
/// tree's root: slash separated. False outside a git work tree, and in a
/// repository with no index yet, where git tracks nothing.
fn in_index(
    project: &Path,
    inside: &Path,
    look: impl FnOnce(&gix::index::File, &str) -> bool,
) -> Result<bool, ExcludeError> {
    let Some((repository, route)) = discover(project)? else {
        return Ok(false);
    };
    let unreadable = |detail: String| ExcludeError::Repository {
        project: project.to_path_buf(),
        detail,
    };
    let index = repository
        .index_or_empty()
        .map_err(|error| unreadable(error.to_string()))?;
    let inside = paths::normalize(&route.join(inside));
    let Some(path) = paths::to_config_string(&inside) else {
        return Err(unreadable(format!(
            "{} is not valid UTF-8, so the index cannot be searched for it",
            inside.display()
        )));
    };
    Ok(look(&index, &path))
}

/// The work tree holding `project`, as git names it, and the route from it
/// down to the project: `None` when no work tree holds it.
///
/// For a caller that walks the folders between the two, as Codex reads an
/// `AGENTS.md` in each ([`crate::agents_md`]). The repository is opened as
/// [`locate`] opens it.
pub(crate) fn work_tree(project: &Path) -> Result<Option<(PathBuf, PathBuf)>, ExcludeError> {
    Ok(discover(project)?.and_then(|(repository, route)| {
        repository
            .workdir()
            .map(|worktree| (worktree.to_path_buf(), route))
    }))
}

/// The repository whose work tree holds `project`, and where in that work
/// tree the project is: `None` when no work tree holds it.
///
/// Opened isolated, reading neither the environment nor the user's git
/// config, for the reason [`locate`] gives.
fn discover(project: &Path) -> Result<Option<(gix::Repository, PathBuf)>, ExcludeError> {
    let repository =
        match gix::discover_opts(project, Default::default(), gix::open::Options::isolated()) {
            Ok(repository) => repository,
            Err(gix::discover::Error::Discover(
                gix::discover::upwards::Error::NoGitRepository { .. }
                | gix::discover::upwards::Error::NoGitRepositoryWithinCeiling { .. }
                | gix::discover::upwards::Error::NoGitRepositoryWithinFs { .. },
            )) => return Ok(None),
            Err(error) => {
                return Err(ExcludeError::Repository {
                    project: project.to_path_buf(),
                    detail: error.to_string(),
                })
            }
        };
    // A bare repository has no work tree, so nothing in it is untracked.
    let Some(worktree) = repository.workdir() else {
        return Ok(None);
    };
    let route = route(worktree, project).ok_or_else(|| ExcludeError::Route {
        project: project.to_path_buf(),
        worktree: worktree.to_path_buf(),
    })?;
    Ok(Some((repository, route)))
}

/// Where `project` sits below `worktree`, or `None` when it is not below it.
///
/// From both paths resolved, not lexically as everywhere else: git matches a
/// pattern against the tree as it is on disk and never follows a symlinked
/// directory, so what is installed into a project reached through `current ->
/// packages/app-v2` is untracked under `packages/app-v2`, and a line naming
/// `current` would keep none of it out. Discovery climbs the resolved path for
/// the same reason. Resolving also spells both sides alike when discovery
/// hands back the work tree as the filesystem has it (`/private/tmp` for
/// `/tmp` on a Mac). Lexically only when a path cannot be resolved.
fn route(worktree: &Path, project: &Path) -> Option<PathBuf> {
    let project = std::path::absolute(project).ok()?;
    match (crate::paths::canonical(worktree), crate::paths::canonical(&project)) {
        (Ok(worktree), Ok(project)) => {
            let route = project.strip_prefix(&worktree).ok()?;
            Some(if route.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                route.to_path_buf()
            })
        }
        _ => paths::relative_to(&project, worktree).filter(|route| {
            route
                .components()
                .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
        }),
    }
}

/// One edit of the block, read, made and written back whole.
///
/// Nothing is written unless the edit changes something, and nothing at all
/// when the block cannot be read: a failure here is collected by the caller,
/// never a reason to leave the file half rewritten.
fn change(
    project: &Path,
    key: &str,
    entries: &[Entry],
    edit: fn(&mut ExcludeFile, &str, &[String]) -> Result<bool, ExcludeError>,
) -> Result<Excluded, ExcludeError> {
    // What the caller asked for is checked before looking for a repository,
    // so a bad key or entry is refused the same way inside git or out.
    check(key, &[])?;
    for entry in entries {
        entry.pattern(Path::new(""))?;
    }
    let Some(location) = locate(project)? else {
        return Ok(Excluded::Outside {
            project: project.to_path_buf(),
        });
    };
    let patterns = entries
        .iter()
        .map(|entry| entry.pattern(&location.route))
        .collect::<Result<Vec<_>, _>>()?;

    let mut file = read(&location.file)?;
    if !edit(&mut file, key, &patterns)? {
        return Ok(Excluded::Unchanged {
            file: location.file,
        });
    }

    write(&location.file, &file)?;
    Ok(Excluded::Written {
        file: location.file,
    })
}

/// The exclude file at `path`, read and parsed: an absent one is an empty
/// one, and a block that cannot be read is refused.
fn read(path: &Path) -> Result<ExcludeFile, ExcludeError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(source) => {
            return Err(ExcludeError::Read {
                path: path.to_path_buf(),
                source,
            })
        }
    };
    ExcludeFile::parse(&bytes).map_err(|problem| ExcludeError::Malformed {
        file: path.to_path_buf(),
        problem,
    })
}

/// One path to keep out of git, relative to the project it is in.
///
/// Which paths those are is the caller's to decide (an installed skill's
/// folders, an `AGENTS.md` Mindweaver created); this only says how a path is
/// written down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// One file.
    File(PathBuf),
    /// A directory and everything under it.
    Directory(PathBuf),
}

impl Entry {
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Entry::File(path.into())
    }

    pub fn directory(path: impl Into<PathBuf>) -> Self {
        Entry::Directory(path.into())
    }

    /// The path, relative to the project.
    pub fn path(&self) -> &Path {
        match self {
            Entry::File(path) | Entry::Directory(path) => path,
        }
    }

    /// The exclude line for this entry in a project at `route` below the work
    /// tree root (empty for a project at the root).
    ///
    /// Anchored with a leading `/`. Git already anchors a pattern with a `/`
    /// in its middle at the work tree root, but one segment matches at every
    /// level: unanchored, a root project's `AGENTS.md` would keep every
    /// `AGENTS.md` in the tree out of git, a teammate's committed one in
    /// another project included. Every line starts with `/`, so one rule
    /// covers both shapes. A directory ends in `/`, so a file that happens to
    /// take its name is not caught. Each segment is written literally, its glob
    /// characters escaped, so a route like `apps/[legacy]` names that folder
    /// rather than a class of letters.
    pub fn pattern(&self, route: &Path) -> Result<String, ExcludeError> {
        let mut line = anchored(route)?;
        let entry = anchored(self.path())?;
        // An empty route is a project at the root. An empty entry would be
        // the whole project, or the whole work tree.
        if entry.is_empty() {
            return Err(ExcludeError::Path {
                path: self.path().to_path_buf(),
            });
        }
        line.push_str(&entry);
        match self {
            Entry::Directory(_) => line.push('/'),
            // Git drops trailing spaces from a pattern unless each is
            // escaped, and a file name may end in one.
            Entry::File(_) => {
                let kept = line.trim_end_matches(' ').len();
                let spaces = line.len() - kept;
                line.truncate(kept);
                line.push_str(&"\\ ".repeat(spaces));
            }
        }
        Ok(line)
    }
}

/// `path`, a route below a work tree or a project, as an exclude line
/// spells it: each segment after a `/`, its glob characters escaped, and
/// nothing for the root itself. Refused when a segment is not one a line
/// can hold.
fn anchored(path: &Path) -> Result<String, ExcludeError> {
    let refused = || ExcludeError::Path {
        path: path.to_path_buf(),
    };
    let mut line = String::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(segment) => {
                let segment = segment.to_str().ok_or_else(refused)?;
                if segment.chars().any(char::is_control) {
                    return Err(refused());
                }
                line.push('/');
                line.push_str(&escape(segment));
            }
            // Climbing out or starting over would aim the line at
            // something outside the project, which is not this
            // module's to hide.
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(refused())
            }
        }
    }
    Ok(line)
}

/// A path segment with git's glob characters escaped, so it matches only
/// itself.
fn escape(segment: &str) -> String {
    let mut escaped = String::with_capacity(segment.len());
    for character in segment.chars() {
        if matches!(character, '*' | '?' | '[' | '\\') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// An exclude file's text, split around Mindweaver's block.
///
/// Kept as lines with their own endings ([`crate::lines`]) rather than as
/// one string to search and splice: what is outside the block is re-emitted
/// exactly as read, CRLF, a missing final newline and bytes that are not
/// UTF-8 included, and a new line is written with whichever ending the file
/// already uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludeFile {
    before: Vec<Line>,
    block: Option<Block>,
    after: Vec<Line>,
    ending: &'static [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    /// Whose markers it is spelled with: those it was found under, and
    /// Mindweaver's for one made anew.
    names: Names,
    begin: Line,
    /// Lines between the begin marker and the first group. Mindweaver never
    /// writes any, so they are somebody's, and they are kept.
    loose: Vec<Line>,
    groups: Vec<Group>,
    end: Line,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Group {
    key: Vec<u8>,
    header: Line,
    lines: Vec<Line>,
}

impl ExcludeFile {
    /// Read an exclude file's bytes. An absent file is an empty one.
    ///
    /// A block that is not exactly one well-formed block is refused rather
    /// than repaired: guessing where a missing end marker belongs could
    /// swallow somebody's patterns into Mindweaver's block, and the next edit
    /// would rewrite them.
    pub fn parse(bytes: &[u8]) -> Result<Self, Malformed> {
        let lines = lines::split(bytes);
        let ending = lines::ending(&lines);

        let mut begin: Option<(usize, Names)> = None;
        let mut end = None;
        for (index, line) in lines.iter().enumerate() {
            let number = index + 1;
            let marked = Names::ALL.into_iter().find_map(|names| {
                let (opens, closes) = markers(names);
                if is(line, opens) {
                    Some((true, names))
                } else if is(line, closes) {
                    Some((false, names))
                } else {
                    None
                }
            });
            match marked {
                None => {}
                Some((true, names)) => {
                    if begin.is_some() {
                        return Err(Malformed::Reopened {
                            line: number,
                            names,
                        });
                    }
                    begin = Some((index, names));
                }
                Some((false, names)) => {
                    let Some((_, block)) = begin.filter(|_| end.is_none()) else {
                        return Err(Malformed::Unopened {
                            line: number,
                            names,
                        });
                    };
                    if block != names {
                        return Err(Malformed::Mixed {
                            line: number,
                            found: names,
                            block,
                        });
                    }
                    end = Some(index);
                }
            }
        }

        let (begin, names, end) = match (begin, end) {
            (None, _) => {
                return Ok(ExcludeFile {
                    before: lines,
                    block: None,
                    after: Vec::new(),
                    ending,
                })
            }
            (Some((begin, names)), None) => {
                return Err(Malformed::Unclosed {
                    line: begin + 1,
                    names,
                })
            }
            (Some((begin, names)), Some(end)) => (begin, names, end),
        };

        let mut lines = lines.into_iter();
        let before: Vec<Line> = lines.by_ref().take(begin).collect();
        let begin_line = lines.next().expect("the begin marker was found");
        let mut loose = Vec::new();
        let mut groups: Vec<Group> = Vec::new();
        for (offset, line) in lines.by_ref().take(end - begin - 1).enumerate() {
            if let Some(key) = key_of(&line) {
                if groups.iter().any(|group| group.key == key) {
                    return Err(Malformed::Regrouped {
                        line: begin + offset + 2,
                        key: String::from_utf8_lossy(key).into_owned(),
                    });
                }
                groups.push(Group {
                    key: key.to_vec(),
                    header: line,
                    lines: Vec::new(),
                });
            } else if let Some(group) = groups.last_mut() {
                group.lines.push(line);
            } else {
                loose.push(line);
            }
        }
        let end_line = lines.next().expect("the end marker was found");
        Ok(ExcludeFile {
            before,
            block: Some(Block {
                names,
                begin: begin_line,
                loose,
                groups,
                end: end_line,
            }),
            after: lines.collect(),
            ending,
        })
    }

    /// Put `patterns` in `key`'s group, after what it already holds, creating
    /// the group at the end of the block and the block at the end of the file
    /// when there is none. Whether anything changed.
    pub fn add(&mut self, key: &str, patterns: &[String]) -> Result<bool, ExcludeError> {
        check(key, patterns)?;
        let ending = self.ending;
        let mut fresh: Vec<&String> = Vec::new();
        let held = self.block.as_ref().and_then(|block| {
            block
                .groups
                .iter()
                .find(|group| group.key == key.as_bytes())
        });
        for pattern in patterns {
            let present = held.is_some_and(|group| {
                group
                    .lines
                    .iter()
                    .any(|line| line.text == pattern.as_bytes())
            });
            if !present && !fresh.contains(&pattern) {
                fresh.push(pattern);
            }
        }
        if fresh.is_empty() {
            return Ok(false);
        }

        if self.block.is_none() {
            // The new block goes on lines of its own, so a last line without
            // a newline gets one, and the end marker goes without one
            // instead: the file still ends the way it did, and dropping the
            // block takes the newline back.
            let mut close = ending;
            if let Some(last) = self.before.last_mut() {
                if last.end.is_empty() {
                    last.end = ending;
                    close = b"";
                }
            }
            self.block = Some(Block {
                names: Names::Mindweaver,
                begin: Line::new(BEGIN, ending),
                loose: Vec::new(),
                groups: Vec::new(),
                end: Line::new(END, close),
            });
        }
        let block = self.block.as_mut().expect("there is a block now");
        let group = match block
            .groups
            .iter()
            .position(|group| group.key == key.as_bytes())
        {
            Some(index) => &mut block.groups[index],
            None => {
                block.groups.push(Group {
                    key: key.as_bytes().to_vec(),
                    header: Line::new(&format!("{GROUP}{key}"), ending),
                    lines: Vec::new(),
                });
                block.groups.last_mut().expect("a group was just pushed")
            }
        };
        group
            .lines
            .extend(fresh.into_iter().map(|pattern| Line::new(pattern, ending)));
        Ok(true)
    }

    /// Take `patterns` out of `key`'s group, dropping the group when nothing
    /// is left in it and the block when no group is. Whether anything
    /// changed.
    pub fn remove(&mut self, key: &str, patterns: &[String]) -> Result<bool, ExcludeError> {
        check(key, patterns)?;
        let Some(block) = self.block.as_mut() else {
            return Ok(false);
        };
        let Some(index) = block
            .groups
            .iter()
            .position(|group| group.key == key.as_bytes())
        else {
            return Ok(false);
        };
        let group = &mut block.groups[index];
        let held = group.lines.len();
        group.lines.retain(|line| {
            !patterns
                .iter()
                .any(|pattern| line.text == pattern.as_bytes())
        });
        if group.lines.len() == held {
            return Ok(false);
        }

        if group.lines.iter().all(Line::is_blank) {
            block.groups.remove(index);
        }
        self.tidy();
        Ok(true)
    }

    /// Take `key`'s group out whole, its header and every line under it,
    /// dropping the block when no group is left. Whether anything changed.
    pub fn forget(&mut self, key: &str) -> Result<bool, ExcludeError> {
        check(key, &[])?;
        let Some(block) = self.block.as_mut() else {
            return Ok(false);
        };
        let Some(index) = block
            .groups
            .iter()
            .position(|group| group.key == key.as_bytes())
        else {
            return Ok(false);
        };
        block.groups.remove(index);
        self.tidy();
        Ok(true)
    }

    /// Whose markers the block is spelled with: `None` when there is none.
    pub fn names(&self) -> Option<Names> {
        self.block.as_ref().map(|block| block.names)
    }

    /// Spell the block's two markers with `names`', and whether that
    /// changed anything: what `migrate --names` does to a block a
    /// Mindflayer build wrote, every group and every byte outside it kept,
    /// the markers' line endings among them. The block's, whichever
    /// projects' groups it holds: a file has one block, so its markers are
    /// spelled one way for all of them.
    pub fn renamed(&mut self, names: Names) -> bool {
        let Some(block) = &mut self.block else {
            return false;
        };
        if block.names == names {
            return false;
        }
        let (begin, end) = markers(names);
        block.begin = Line::new(begin, block.begin.end);
        block.end = Line::new(end, block.end.end);
        block.names = names;
        true
    }

    /// Put `to` in place of the line `from` in `key`'s group, where it
    /// stands, and whether that changed anything: for a path an entry names
    /// that moved, its group's order kept, as Claude's rule files a
    /// Mindflayer build filed are moved to the new folder.
    pub fn replace(&mut self, key: &str, from: &str, to: &str) -> Result<bool, ExcludeError> {
        check(key, &[from.to_owned(), to.to_owned()])?;
        let Some(block) = self.block.as_mut() else {
            return Ok(false);
        };
        let Some(group) = block
            .groups
            .iter_mut()
            .find(|group| group.key == key.as_bytes())
        else {
            return Ok(false);
        };
        let Some(line) = group
            .lines
            .iter_mut()
            .find(|line| line.text == from.as_bytes())
        else {
            return Ok(false);
        };
        let end = line.end;
        *line = Line::new(to, end);
        Ok(true)
    }

    /// Every line of `key`'s group but blank ones, as written: none when
    /// there is no such group. Bytes that are not UTF-8 are shown as
    /// replacement characters, being for a person to read.
    pub fn lines_of(&self, key: &str) -> Vec<String> {
        self.block
            .iter()
            .flat_map(|block| &block.groups)
            .filter(|group| group.key == key.as_bytes())
            .flat_map(|group| &group.lines)
            .filter(|line| !line.is_blank())
            .map(|line| String::from_utf8_lossy(&line.text).into_owned())
            .collect()
    }

    /// Drop the block once it holds no group and nothing somebody else put
    /// in it, giving back the file as it was before the block was written.
    fn tidy(&mut self) {
        let Some(block) = &self.block else {
            return;
        };
        if block.groups.is_empty() && block.loose.iter().all(Line::is_blank) {
            // An end marker without a newline is the file's last line, so the
            // file ended without one and still does once the block is gone.
            let unended = block.end.end.is_empty();
            self.block = None;
            let after = std::mem::take(&mut self.after);
            self.before.extend(after);
            if let Some(last) = self.before.last_mut().filter(|_| unended) {
                last.end = b"";
            }
        }
    }

    /// The file as it is to be written.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut emit = |line: &Line| {
            out.extend_from_slice(&line.text);
            out.extend_from_slice(line.end);
        };
        self.before.iter().for_each(&mut emit);
        if let Some(block) = &self.block {
            emit(&block.begin);
            block.loose.iter().for_each(&mut emit);
            for group in &block.groups {
                emit(&group.header);
                group.lines.iter().for_each(&mut emit);
            }
            emit(&block.end);
        }
        self.after.iter().for_each(&mut emit);
        out
    }
}

/// Whether a line is the marker `marker`. Trailing whitespace is forgiven,
/// as git forgives it in a pattern; leading whitespace is not, because git
/// reads `  # mindweaver:begin` as a pattern rather than a comment.
fn is(line: &Line, marker: &str) -> bool {
    line.text.trim_ascii_end() == marker.as_bytes()
}

/// The key a group header names, when the line is one.
fn key_of(line: &Line) -> Option<&[u8]> {
    line.text
        .strip_prefix(GROUP.as_bytes())
        .map(<[u8]>::trim_ascii_end)
        .filter(|key| !key.is_empty())
}

/// Refuse a key or a pattern that cannot be written as one line of the
/// block. A line break in either would put a line of its choosing into the
/// file, and a key with space around it would not read back as itself.
///
/// A pattern git reads as blank or as a comment excludes nothing, and inside
/// the block a comment is structure: `# mindweaver:end` would close the block
/// early, leaving a file the next read refuses, and `# project b` would hand
/// the lines after it to another project. [`Entry::pattern`] never writes
/// one, since its lines start with `/`; this is for patterns handed in raw.
fn check(key: &str, patterns: &[String]) -> Result<(), ExcludeError> {
    if key.is_empty() || key.trim() != key || key.chars().any(char::is_control) {
        return Err(ExcludeError::Key {
            key: key.to_owned(),
        });
    }
    if let Some(pattern) = patterns.iter().find(|pattern| {
        pattern.trim_ascii().is_empty()
            || pattern.starts_with('#')
            || pattern.chars().any(char::is_control)
    }) {
        return Err(ExcludeError::Pattern {
            pattern: pattern.clone(),
        });
    }
    Ok(())
}

/// Why the block in an exclude file is not one Mindweaver can edit. Lines are
/// counted from 1, and a marker is quoted as the file spells it, under
/// `names`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Malformed {
    #[error("line {line}: `{}` is never closed by `{}`", markers(*.names).0, markers(*.names).1)]
    Unclosed { line: usize, names: Names },
    #[error("line {line}: `{}` closes no `{}`", markers(*.names).1, markers(*.names).0)]
    Unopened { line: usize, names: Names },
    #[error("line {line}: a second `{}`, where there can be one block", markers(*.names).0)]
    Reopened { line: usize, names: Names },
    /// An end marker spelled under other names than the block's begin
    /// marker.
    #[error(
        "line {line}: an end marker spelled as {found} spells it, in a block {block}'s \
         marker opens, where both markers of a block are spelled alike"
    )]
    Mixed {
        line: usize,
        found: Names,
        block: Names,
    },
    #[error("line {line}: a second group for `{key}` in the block")]
    Regrouped { line: usize, key: String },
}

codes!(Malformed {
    Self::Unclosed { .. } => "exclude_block_unclosed",
    Self::Unopened { .. } => "exclude_block_unopened",
    Self::Reopened { .. } => "exclude_block_reopened",
    Self::Mixed { .. } => "exclude_markers_mixed",
    Self::Regrouped { .. } => "exclude_group_repeated",
});

/// Why a project's paths could not be kept out of git.
#[derive(Debug, Error)]
pub enum ExcludeError {
    #[error("{key:?} cannot name a group in the exclude file: a key is one line with no space around it")]
    Key { key: String },
    #[error("{pattern:?} cannot be written in the exclude file: a pattern is one line, neither blank nor a comment")]
    Pattern { pattern: String },
    #[error("{path}: no line of the exclude file can name it, so it cannot be kept out of git")]
    Path { path: PathBuf },
    #[error("{project}: the git repository it is in cannot be opened: {detail}")]
    Repository { project: PathBuf, detail: String },
    #[error("{project}: not below {worktree}, the work tree of the repository it is in")]
    Route { project: PathBuf, worktree: PathBuf },
    #[error("{path}: cannot be read: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: cannot be written: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{file}: {problem}, so nothing was written")]
    Malformed { file: PathBuf, problem: Malformed },
}

codes!(ExcludeError {
    Self::Key { .. } => "exclude_key_invalid",
    Self::Pattern { .. } => "exclude_pattern_invalid",
    Self::Path { .. } => "exclude_path_unnamable",
    Self::Repository { .. } => "repository_unopened",
    Self::Route { .. } => "project_outside_worktree",
    Self::Read { .. } => "exclude_read_failed",
    Self::Write { .. } => "exclude_write_failed",
} carried {
    Self::Malformed { problem, .. } => problem,
});

#[cfg(test)]
#[path = "exclude_test.rs"]
mod tests;
