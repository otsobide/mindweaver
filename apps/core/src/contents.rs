//! One file of an artifact, read for a front end that shows it where a file
//! must never run: the web panel, which asks for it over the local server
//! (#67).
//!
//! What a skill holds is somebody else's: gathered from a repository, or put
//! in an agent's folder by another tool. A `SKILL.md` can carry a `<script>`,
//! an asset can be an HTML page or an SVG with a script inside, and a link
//! can lead anywhere on the disk. So a file reaches a front end only as
//! this module reads it:
//!
//! - **As text inside JSON, or not at all.** [`Contents`] holds the text as
//!   a string, which a front end puts on a page as text; nothing here says
//!   what type of file it is, so nothing downstream can serve it as one, and
//!   no file is ever handed over as its own bytes. A file that is not UTF-8,
//!   or holds a NUL, is said to be binary, with its size, and its bytes stay
//!   where they are.
//! - **Held to its holder once its links are followed**, by the check a copy
//!   makes of every link it meets ([`copy::followed`]): a file, or a folder
//!   on the way to it, that leads out of the project it belongs to, or for
//!   the workspace's own out of the folder it keeps that kind in, is
//!   refused, never read. Before that, the file is named by a route of plain
//!   names from the artifact, so a `..` never gets as far as the disk.
//! - **No bigger than [`CAP`]**: one larger is said to be too large, with its
//!   size, and never read whole, so a browser cannot make the server read a
//!   disk image into memory by naming one.
//!
//! What an artifact holds is listed the same way ([`Files`]): every file and
//! folder by its route, a link judged by the same check before it is
//! followed, one leading out of the holder or back into a folder above it
//! listed with why and never followed, as is one whose name no route
//! spells, and no more than [`FILES_CAP`] of them, so a front end can show
//! a skill's tree and ask for each file in it by the route it lists.
//!
//! Rules in core, as every rule about a path is: the server carries what this
//! answers and the panel draws it, and neither could disagree with the other
//! about what may be read.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};
use std::slice;

use serde::{Serialize, Serializer};
use serde_yaml_ng::Value;
use thiserror::Error;

use crate::catalog::{Catalog, Reference};
use crate::code::{codes, Said};
use crate::copy;
use crate::frontmatter;
use crate::hash;
use crate::install;
use crate::kind::{Kind, Layout};
use crate::lifecycle::{self, LifecycleError};
use crate::paths;
use crate::rule::RuleManifest;
use crate::value::{ArtifactName, ContentHash};
use crate::workspace::{MindProject, WeaverWorkspace};

/// The most a file may hold, in bytes, to have its text read: a mebibyte.
///
/// Room for any `SKILL.md` and the references beside it, which the Agent
/// Skills format asks to keep short, and little enough that every answer
/// is quick to build, send and draw. A file larger than this is no file a
/// person reads in a browser.
pub const CAP: u64 = 1 << 20;

/// One file of an artifact, as a front end may show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Contents {
    /// The artifact it belongs to.
    pub kind: Kind,
    pub name: ArtifactName,
    /// The file, as it was asked for: a route from the artifact's folder,
    /// `/` separated, or a rule's own file name.
    pub file: String,
    /// How many bytes it holds.
    pub size: u64,
    /// The SHA-256 of what it holds ([`hash::bytes`]), when it was read
    /// whole: none for one larger than [`CAP`], which never is.
    ///
    /// What `edit --if-unchanged` takes, so a front end that changes the
    /// file holds the change to what it read, and is refused when somebody
    /// changed it since (`edit_stale`). Hashed here, of the bytes the text
    /// was read from, rather than by the front end from the text: one
    /// recipe for every hash of a file, and a text a browser hashed could
    /// be told apart from the file by nothing but a line ending.
    pub hash: Option<ContentHash>,
    /// What it says, when it is UTF-8 text no larger than [`CAP`]; none
    /// otherwise, and [`withheld`](Self::withheld) says why.
    pub text: Option<String>,
    /// Why there is no text, when there is none.
    pub withheld: Option<Withheld>,
    /// What it declares and says, cut as core reads it, when it is the file
    /// the artifact is read from (a skill's `SKILL.md`, a rule's own file)
    /// and its text was read; none for any other file, whose front matter
    /// no agent reads as such.
    pub manifest: Option<Manifest>,
}

/// The file an artifact is read from, cut at its front matter as core cuts
/// it: what a front end shows as a table of what it declares above the
/// markdown it renders.
///
/// Cut here rather than by a front end from [`Contents::text`], since where
/// front matter ends, and whether a rule's fences hold any, are rules
/// ([`frontmatter::split`], [`RuleManifest::read`]): a panel cutting it
/// itself could show as declared what every agent reads as content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Manifest {
    /// Each key at the top of its front matter, in the order written, every
    /// one the format does not define among them; none when it has none
    /// core reads: no fences, fences that never close, or a block that is
    /// no YAML mapping.
    pub front_matter: Option<Vec<Declaration>>,
    /// What follows the front matter, as an agent reads it: the whole file
    /// when no front matter is read.
    pub body: String,
}

/// One key of an artifact's front matter, and what it is given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Declaration {
    pub key: String,
    /// A string as it is; anything else, a list or a mapping, a number or
    /// nothing, as YAML writes it, so a value is shown whatever its shape
    /// rather than only those the format defines.
    pub value: String,
}

/// The most entries a listing of an artifact's files holds: a thousand.
///
/// More than any skill the format describes holds (its `SKILL.md`, and the
/// scripts, references and assets beside it), and few enough that an
/// answer stays quick to build and to draw, whatever a folder somebody else
/// wrote holds: a skill that vendors a dependency tree is said to be cut
/// short rather than walked to its end.
pub const FILES_CAP: usize = 1000;

/// Every file and folder of an artifact, as a front end may list them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Files {
    /// The artifact they belong to.
    pub kind: Kind,
    pub name: ArtifactName,
    /// The file it is read from, as [`Contents`] takes it: a skill's
    /// `SKILL.md`, or a rule's own file name. Said rather than left to a
    /// front end, which would otherwise spell where each kind keeps what
    /// it says.
    pub manifest: String,
    /// Each file and folder it holds, a folder before what it holds and
    /// each folder's in the order of their names: a rule's one file alone.
    pub entries: Vec<Listed>,
    /// Whether it holds more than [`FILES_CAP`], which `entries` stops at.
    pub truncated: bool,
}

/// One file or folder of an artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Listed {
    /// Its route from the artifact's folder, `/` separated, as [`Contents`]
    /// takes it; a rule's own file name. One whose name no route spells is
    /// refused (`contents_name_unfit`) and listed by that name as near as
    /// UTF-8 writes it, which another entry's can match: a refused entry is
    /// shown, never asked for.
    pub path: String,
    /// Whether it is a folder, whose files are listed after it.
    pub folder: bool,
    /// How many bytes a file holds; none for a folder, and for one not
    /// followed.
    pub size: Option<u64>,
    /// Why it is not followed, when it is not: a link leading out of the
    /// holder, or back into a folder it is in, or something that does not
    /// read. Listed all the same, since somebody looking for why a skill
    /// misbehaves is looking for exactly that.
    pub refused: Option<Said>,
}

/// Why a file's text is not handed over, though the file was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Withheld {
    /// It is not UTF-8, or it holds a NUL: an image, an archive, a program.
    /// Text a browser could show would be a guess at what the bytes mean.
    Binary,
    /// It holds more than [`CAP`] bytes.
    TooLarge,
}

impl Withheld {
    /// How JSON says it: one word, which once printed is a contract.
    pub const fn slug(self) -> &'static str {
        match self {
            Withheld::Binary => "binary",
            Withheld::TooLarge => "too_large",
        }
    }
}

/// By its slug, for the reason a kind is: a renamed variant cannot change
/// what a front end compares.
impl Serialize for Withheld {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.slug())
    }
}

#[cfg(any(feature = "ts", feature = "schema"))]
crate::described::words! {
    /// Why a file's text is not handed over, though the file was found.
    Withheld by slug { Binary, TooLarge }
}

/// Why a file was not read at all.
#[derive(Debug, Error)]
pub enum ContentsError {
    /// The reference names nothing the holder holds, or more than one thing.
    #[error(transparent)]
    Find(#[from] LifecycleError),
    #[error("`{file}` names no file of {artifact}: a file is named by its route from the artifact's folder, each part a plain name, and a rule kept in one file by that file's name")]
    Route { file: String, artifact: String },
    #[error("{path} leads to {resolved}, outside {holder}, so it is not read")]
    Outside {
        path: PathBuf,
        resolved: PathBuf,
        holder: PathBuf,
    },
    #[error("{path}: no such file")]
    Missing { path: PathBuf },
    #[error("{path} has a name no route spells, one that is not UTF-8 or holds a `\\`, so it is not opened")]
    NameUnfit { path: PathBuf },
    #[error("{path}: not a file")]
    NotAFile { path: PathBuf },
    #[error("{path} leads back to {resolved}, a folder it is in, so it is not followed")]
    Cycle { path: PathBuf, resolved: PathBuf },
    #[error("{path}: cannot be read: {source}")]
    Unreadable {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

codes!(ContentsError {
    Self::Route { .. } => "contents_route_unfit",
    Self::Outside { .. } => "contents_outside",
    Self::Missing { .. } => "contents_missing",
    Self::NameUnfit { .. } => "contents_name_unfit",
    Self::NotAFile { .. } => "contents_not_a_file",
    Self::Cycle { .. } => "contents_cycle",
    Self::Unreadable { .. } => "contents_unreadable",
} carried {
    Self::Find(error) => error,
});

impl Contents {
    /// `file` of the artifact `reference` names among `workspace`'s own: the
    /// library a snapshot lists, of the kinds it keeps apart from a project
    /// at its root ([`WeaverWorkspace::own_kinds`]).
    ///
    /// Held to the folder the workspace keeps the artifact's kind in, not to
    /// its root as an install from it is: the default workspace's root is a
    /// home, where every secret its user keeps would count as inside it.
    pub fn of_library(
        workspace: &WeaverWorkspace,
        reference: &str,
        file: &str,
    ) -> Result<Contents, ContentsError> {
        read(
            &workspace.own(),
            &workspace.own_kinds(),
            Within::Kept,
            reference,
            file,
        )
    }

    /// `file` of the artifact `reference` names in `project`, wherever a
    /// snapshot lists it from: authored, an install's copy, or somebody's in
    /// one of its agents' folders.
    ///
    /// Held to the project's root, as an install from it holds a copy: a
    /// project is a repository, and a skill in it may link to a file
    /// elsewhere in it.
    pub fn of_project(
        project: &MindProject,
        reference: &str,
        file: &str,
    ) -> Result<Contents, ContentsError> {
        read(project, &Kind::ALL, Within::Root, reference, file)
    }
}

impl Files {
    /// What the artifact `reference` names among `workspace`'s own holds,
    /// held as [`Contents::of_library`] holds a file of it.
    pub fn of_library(
        workspace: &WeaverWorkspace,
        reference: &str,
    ) -> Result<Files, ContentsError> {
        list(
            &workspace.own(),
            &workspace.own_kinds(),
            Within::Kept,
            reference,
        )
    }

    /// What the artifact `reference` names in `project` holds, held as
    /// [`Contents::of_project`] holds a file of it.
    pub fn of_project(project: &MindProject, reference: &str) -> Result<Files, ContentsError> {
        list(project, &Kind::ALL, Within::Root, reference)
    }
}

/// What a file is held to once its links are followed.
#[derive(Clone, Copy)]
enum Within {
    /// The holder's root.
    Root,
    /// The folder the holder keeps the artifact's kind in.
    Kept,
}

/// The one artifact a reference names in a holder, and what it is held to.
struct Located {
    kind: Kind,
    name: String,
    /// The folder a skill is, or the file a rule is, as discovery found it.
    at: PathBuf,
    /// What every file of it must lie in once its links are followed, with
    /// its own links followed already.
    bound: PathBuf,
}

/// The one artifact of `kinds` that `reference` names in `holder`, held
/// `within` it.
fn locate(
    holder: &MindProject,
    kinds: &[Kind],
    within: Within,
    reference: &str,
) -> Result<Located, ContentsError> {
    let reference = Reference::parse(reference);
    let (kind, name, at) = artifact(holder, kinds, &reference)?;
    // A `Kept` read never falls back to the root in fact: every kind has a
    // folder in the workspace's own (`WeaverWorkspace::own`), and the
    // artifact was found in its kind's.
    let bound = match within {
        Within::Root => None,
        Within::Kept => holder.directory_for(kind),
    }
    .unwrap_or_else(|| holder.root().to_path_buf());
    let bound = crate::paths::canonical(&bound).map_err(|source| ContentsError::Unreadable {
        path: bound,
        source,
    })?;
    Ok(Located {
        kind,
        name,
        at,
        bound,
    })
}

/// Where `path` leads once its links are followed, when that is inside
/// `bound`: the check [`copy::tree`] makes of each link it meets.
fn held(path: &Path, bound: &Path) -> Result<PathBuf, ContentsError> {
    let (resolved, inside) =
        copy::followed(path, &[bound.to_path_buf()]).map_err(|source| match source.kind() {
            io::ErrorKind::NotFound => ContentsError::Missing {
                path: path.to_path_buf(),
            },
            _ => ContentsError::Unreadable {
                path: path.to_path_buf(),
                source,
            },
        })?;
    if !inside {
        return Err(ContentsError::Outside {
            path: path.to_path_buf(),
            resolved,
            holder: bound.to_path_buf(),
        });
    }
    Ok(resolved)
}

/// `file` of the one artifact of `kinds` that `reference` names in `holder`,
/// held `within` it.
fn read(
    holder: &MindProject,
    kinds: &[Kind],
    within: Within,
    reference: &str,
    file: &str,
) -> Result<Contents, ContentsError> {
    let Located {
        kind,
        name,
        at,
        bound,
    } = locate(holder, kinds, within, reference)?;
    let path = place(kind, &at, file).ok_or_else(|| ContentsError::Route {
        file: file.to_owned(),
        artifact: format!("{}{}{name}", kind.slug(), crate::catalog::QUALIFIER),
    })?;
    let resolved = held(&path, &bound)?;

    // The resolved path from here on, never the one asked for: a link put
    // in its place after the check would otherwise be followed by the read.
    let unreadable = |source| ContentsError::Unreadable {
        path: path.clone(),
        source,
    };
    let metadata = fs::metadata(&resolved).map_err(unreadable)?;
    if !metadata.is_file() {
        return Err(ContentsError::NotAFile { path });
    }
    let withheld = |size, hash, withheld| Contents {
        kind,
        name: ArtifactName::from(&name),
        file: file.to_owned(),
        size,
        hash,
        text: None,
        withheld: Some(withheld),
        manifest: None,
    };
    if metadata.len() > CAP {
        return Ok(withheld(metadata.len(), None, Withheld::TooLarge));
    }
    // One byte past the cap, so a file that grew since it was measured is
    // caught rather than cut short and passed off as whole.
    let mut bytes = Vec::new();
    File::open(&resolved)
        .and_then(|opened| opened.take(CAP + 1).read_to_end(&mut bytes))
        .map_err(unreadable)?;
    let size = bytes.len() as u64;
    if size > CAP {
        return Ok(withheld(size, None, Withheld::TooLarge));
    }
    let hash = Some(hash::bytes(&bytes));
    match String::from_utf8(bytes) {
        Ok(text) if !text.contains('\0') => Ok(Contents {
            kind,
            name: ArtifactName::from(name),
            file: file.to_owned(),
            size,
            hash,
            manifest: is_manifest(kind, file).then(|| Manifest::of(kind, &text)),
            text: Some(text),
            withheld: None,
        }),
        _ => Ok(withheld(size, hash, Withheld::Binary)),
    }
}

/// Whether `file`, a route of an artifact of `kind`, is the one it is read
/// from: a skill's manifest, or a rule's or a definition's one file, the
/// only route it has.
fn is_manifest(kind: Kind, file: &str) -> bool {
    match kind.layout() {
        Layout::Directory { manifest } => file == manifest,
        Layout::Files { .. } | Layout::Flat { .. } => true,
    }
}

impl Manifest {
    /// `text`, the file an artifact of `kind` is read from, cut as its
    /// kind's reader cuts it: a skill wherever its fences close, since its
    /// front matter is required and what does not read is `validate`'s to
    /// say; a rule only where they hold a mapping, as [`RuleManifest::read`]
    /// decides, since its front matter is optional and anything else is
    /// content; and a definition never, since it has no front matter: its
    /// file is what it declares, and is shown whole.
    fn of(kind: Kind, text: &str) -> Manifest {
        let cut = match kind {
            Kind::Skill => frontmatter::split(text).ok(),
            Kind::Mcp => None,
            Kind::Rule => {
                let (_, body) = RuleManifest::read(text);
                // Its fences were taken off only when they hold a mapping.
                frontmatter::split(text)
                    .ok()
                    .filter(|document| document.body.len() == body.len())
            }
        };
        match cut {
            Some(document) => Manifest {
                front_matter: declarations(document.front_matter),
                body: document.body.to_owned(),
            },
            None => Manifest {
                front_matter: None,
                body: text.to_owned(),
            },
        }
    }
}

/// Each key at the top of `front_matter`, with its value, when it is a YAML
/// mapping.
fn declarations(front_matter: &str) -> Option<Vec<Declaration>> {
    let Ok(Value::Mapping(mapping)) = serde_yaml_ng::from_str::<Value>(front_matter) else {
        return None;
    };
    Some(
        mapping
            .iter()
            .map(|(key, value)| Declaration {
                key: written(key),
                value: written(value),
            })
            .collect(),
    )
}

/// A YAML value as somebody reads it: a string as it is, anything else as
/// YAML writes it, without the line YAML ends it with.
fn written(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => serde_yaml_ng::to_string(other)
            .map(|text| text.trim_end().to_owned())
            .unwrap_or_default(),
    }
}

/// Every file and folder of the one artifact of `kinds` that `reference`
/// names in `holder`, held `within` it.
///
/// The artifact itself leading out of its holder is refused, as a file of
/// it is; anything further down that does is listed with why, and never
/// followed, so one bad link hides nothing else the folder holds.
fn list(
    holder: &MindProject,
    kinds: &[Kind],
    within: Within,
    reference: &str,
) -> Result<Files, ContentsError> {
    let Located {
        kind,
        name,
        at,
        bound,
    } = locate(holder, kinds, within, reference)?;
    let resolved = held(&at, &bound)?;
    let unreadable = |source| ContentsError::Unreadable {
        path: at.clone(),
        source,
    };
    let metadata = fs::metadata(&resolved).map_err(unreadable)?;
    let mut walk = Walk {
        bound,
        entries: Vec::new(),
        truncated: false,
    };
    let manifest = match kind.layout() {
        Layout::Directory { manifest } => {
            if !metadata.is_dir() {
                return Err(ContentsError::NotAFile { path: at });
            }
            let names = names(&resolved).map_err(unreadable)?;
            walk.folder(&at, &resolved, names, "", &mut vec![resolved.clone()]);
            manifest.to_owned()
        }
        Layout::Files { .. } | Layout::Flat { .. } => {
            // A rule or a definition was found as a file, so it has a name.
            let file = at
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !metadata.is_file() {
                return Err(ContentsError::NotAFile { path: at });
            }
            walk.entries.push(Listed {
                path: file.clone(),
                folder: false,
                size: Some(metadata.len()),
                refused: None,
            });
            file
        }
    };
    Ok(Files {
        kind,
        name: ArtifactName::from(name),
        manifest,
        entries: walk.entries,
        truncated: walk.truncated,
    })
}

/// A listing of an artifact's folder, as it goes.
struct Walk {
    bound: PathBuf,
    entries: Vec<Listed>,
    truncated: bool,
}

impl Walk {
    /// Each of `holds`, what the folder at `path` holds (read where it
    /// leads, `resolved`), and what each folder among them holds after it,
    /// each listed at its route below `route`. `inside` is every folder the
    /// walk is in, with its links followed, so a link back into one is
    /// listed rather than walked until the routes are too long.
    fn folder(
        &mut self,
        path: &Path,
        resolved: &Path,
        holds: Vec<OsString>,
        route: &str,
        inside: &mut Vec<PathBuf>,
    ) {
        for name in holds {
            if self.entries.len() == FILES_CAP {
                self.truncated = true;
                return;
            }
            let shown = name.to_string_lossy();
            let route = if route.is_empty() {
                shown.into_owned()
            } else {
                format!("{route}/{shown}")
            };
            let named = path.join(&name);
            // Listed only by a route a file is then asked for by, which
            // `place` takes a part at a time: a name that is not UTF-8
            // would be listed by a spelling no file has, and one holding a
            // `\` by one `place` refuses, so either is a button that can
            // only fail. Refused before it is followed, and a folder of
            // either never walked, since every route below it would be one.
            if !name.to_str().is_some_and(install::is_folder_name) {
                self.refuse(route, ContentsError::NameUnfit { path: named });
                continue;
            }
            // The resolved folder from here on, never the one named: a link
            // put in its place after it was checked is not followed.
            let target = match held(&resolved.join(&name), &self.bound) {
                Ok(target) => target,
                Err(error) => {
                    self.refuse(route, error);
                    continue;
                }
            };
            let metadata = match fs::metadata(&target) {
                Ok(metadata) => metadata,
                Err(source) => {
                    self.refuse(
                        route,
                        ContentsError::Unreadable {
                            path: named.clone(),
                            source,
                        },
                    );
                    continue;
                }
            };
            if metadata.is_dir() {
                if inside.iter().any(|walked| walked.starts_with(&target)) {
                    self.refuse(
                        route,
                        ContentsError::Cycle {
                            path: named.clone(),
                            resolved: target,
                        },
                    );
                    continue;
                }
                let names = match names(&target) {
                    Ok(names) => names,
                    Err(source) => {
                        self.refuse(
                            route,
                            ContentsError::Unreadable {
                                path: named.clone(),
                                source,
                            },
                        );
                        continue;
                    }
                };
                self.entries.push(Listed {
                    path: route.clone(),
                    folder: true,
                    size: None,
                    refused: None,
                });
                inside.push(target.clone());
                self.folder(&named, &target, names, &route, inside);
                inside.pop();
            } else if metadata.is_file() {
                self.entries.push(Listed {
                    path: route,
                    folder: false,
                    size: Some(metadata.len()),
                    refused: None,
                });
            } else {
                self.refuse(
                    route,
                    ContentsError::NotAFile {
                        path: named.clone(),
                    },
                );
            }
        }
    }

    /// `route`, listed with why it is not followed.
    fn refuse(&mut self, route: String, error: ContentsError) {
        self.entries.push(Listed {
            path: route,
            folder: false,
            size: None,
            refused: Some(Said::of(error.code(), &error)),
        });
    }
}

/// What the folder at `path` holds, by name, in the order of the names: the
/// order a listing is read in, the same on every platform.
fn names(path: &Path) -> io::Result<Vec<OsString>> {
    let mut names = fs::read_dir(path)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<io::Result<Vec<_>>>()?;
    names.sort();
    Ok(names)
}

/// The kind and name of the one artifact `reference` names in `holder`, and
/// where it is: the folder a skill is, or the file a rule is.
///
/// As a snapshot lists it, which takes in an artifact whose file does not
/// read under the name its place gives it ([`lifecycle::identify`]): that
/// file is the one most in need of being looked at.
fn artifact(
    holder: &MindProject,
    kinds: &[Kind],
    reference: &Reference,
) -> Result<(Kind, String, PathBuf), LifecycleError> {
    let kinds: Vec<Kind> = kinds
        .iter()
        .copied()
        .filter(|kind| reference.kind().is_none_or(|asked| asked == *kind))
        .collect();
    let catalog = Catalog::discover_kinds(slice::from_ref(holder), &kinds);
    let mut found: Vec<(Kind, String, PathBuf)> = catalog
        .find(reference)
        .into_iter()
        .map(|artifact| {
            (
                artifact.kind(),
                artifact.name().to_string(),
                artifact.path().to_path_buf(),
            )
        })
        .collect();
    for failure in catalog.failures() {
        let Some((kind, name)) = lifecycle::identify(holder, failure.path()) else {
            continue;
        };
        if name != reference.name() || !kinds.contains(&kind) {
            continue;
        }
        let Some(at) = lifecycle::place(holder, kind, &name) else {
            continue;
        };
        if !found.iter().any(|(_, _, path)| *path == at) {
            found.push((kind, name, at));
        }
    }

    match found.len() {
        0 => Err(LifecycleError::NotFound {
            typed: reference.typed().to_owned(),
        }),
        1 => Ok(found.remove(0)),
        _ => Err(LifecycleError::Ambiguous {
            typed: reference.typed().to_owned(),
            matches: found
                .iter()
                .map(|(kind, name, path)| {
                    let at =
                        paths::relative_to(path, holder.root()).unwrap_or_else(|| path.clone());
                    format!(
                        "{}{}{name} at {}",
                        kind.slug(),
                        crate::catalog::QUALIFIER,
                        at.display()
                    )
                })
                .collect(),
        }),
    }
}

/// Where `file` is in an artifact of `kind` at `at`; none when it names no
/// file of it.
///
/// A route of plain names for a skill, each one folder or file name, so a
/// `..`, a root or an empty part never reaches the disk; a rule and a
/// definition are one file, so its own file name is the one route it has.
/// Joined a part at a time, so a `/` means the same on every platform.
fn place(kind: Kind, at: &Path, file: &str) -> Option<PathBuf> {
    match kind.layout() {
        Layout::Directory { .. } => file.split('/').all(install::is_folder_name).then(|| {
            file.split('/')
                .fold(at.to_path_buf(), |path, part| path.join(part))
        }),
        Layout::Files { .. } | Layout::Flat { .. } => {
            (at.file_name() == Some(OsStr::new(file))).then(|| at.to_path_buf())
        }
    }
}

#[cfg(test)]
#[path = "contents_test.rs"]
mod tests;
