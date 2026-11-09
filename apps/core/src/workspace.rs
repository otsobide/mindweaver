//! The two levels Mindweaver works at.
//!
//! A **mind project** is a directory carrying a `.mind`, the way a repository
//! carries a `.git`: its marker says which agents work in it, and it authors
//! skills and rules beside the code, in the directories the marker names
//! (`skills/<name>/SKILL.md` and `rules/` by default).
//!
//! A **weaver workspace** sits above them, carrying a `.mindweaver` that
//! references the mind projects it manages, so their artifacts can be listed
//! and moved across several projects at once, and holds artifacts of its own
//! to share with them, in a library inside that `.mindweaver` by default.
//! One a Mindflayer build made carries a `.mindflayer` instead, read and
//! written under those names until it is moved ([`Names`]).
//!
//! Both are found the way git finds a repository: by walking up from a
//! starting directory until the marker file appears. Both are described by a
//! marker file rather than by the directory alone, so an empty `.mind` left
//! behind by a failed copy is not mistaken for a project.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::agent::{Agent, AgentTarget, TargetTable};
use crate::code::{codes, Said};
use crate::gather::{self, Scrub};
use crate::kind::{Kind, Layout};
use crate::ledger::{Ledger, LedgerError};
use crate::lock::{self, LockError, WorkspaceLock};
use crate::names::Names;
use crate::paths;
use crate::settings::{ProjectId, Settings, Share, SETTINGS_FILE, SETTINGS_JSON_FILE};

/// The newest version of a project's marker this build reads: version 5, a
/// `.mind/settings.toml` ([`SETTINGS_TOML_VERSION`]).
///
/// Refusing to read a version from the future is what lets the format change
/// later without an old binary silently misreading a newer project. The
/// project marker has a version of its own rather than one shared with
/// [`WEAVER_FORMAT_VERSION`]: shared, a change to either format would be a new
/// version of both, and an older build would refuse a workspace that had not
/// changed at all.
///
/// 5 for a `settings.toml` whatever keys it sets, though it holds the keys a
/// version 4 `settings.json` does: a version names the file it is written in
/// as well as what that file holds, as [`SETTINGS_VERSION`] drew the line
/// between `mind.toml` and `settings.json`, so one found in another file is
/// refused rather than guessed at ([`MarkerFile::of_version`]). The rule
/// that writes 4 only while `targets` is set stays with the files it was
/// made for, which are read as they were written. A build from before it
/// never looks for a `settings.toml`, so it finds no project in one rather
/// than misreading it.
pub const MIND_FORMAT_VERSION: u32 = 5;

/// The newest version a `mind.toml` holds: what came before
/// [`SETTINGS_VERSION`], and what [`MindProject::init`] still writes.
///
/// Not [`MIND_FORMAT_VERSION`], which is the newest a project's marker holds
/// in either file: a new project is marked by its settings, which an older
/// build finds no project in, rather than one it misreads.
pub const MIND_TOML_VERSION: u32 = 2;

/// The layout version written into new `weaver.toml` markers, and the newest
/// one this build reads. Its own, for the reason [`MIND_FORMAT_VERSION`] is.
pub const WEAVER_FORMAT_VERSION: u32 = 2;

/// The first [`MIND_FORMAT_VERSION`] whose markers say where each kind's
/// artifacts live. Only a project marker's version is held against it: a
/// workspace marker's number says nothing about where a project keeps things.
///
/// Before it, that was not a question a project could answer: every kind lived
/// inside `.mind`. Reading such a marker as if it used today's default would
/// point a project at directories it has never had and list nothing, silently,
/// which is exactly what the version field exists to prevent.
pub const DIRECTORIES_VERSION: u32 = 2;

/// The first [`MIND_FORMAT_VERSION`] whose marker is a project's settings
/// ([`crate::settings`]) rather than `.mind/mind.toml`: `.mind/settings.json`
/// up to [`TARGETS_VERSION`], and `.mind/settings.toml` from
/// [`SETTINGS_TOML_VERSION`].
///
/// A line drawn the way [`DIRECTORIES_VERSION`] is: a marker from before it is
/// read the way it was written, as a `mind.toml`, and one from it on as the
/// settings file its version names ([`MarkerFile::of_version`]). Past it, a
/// kind the marker names no folder for has none
/// ([`MindProject::directory_for`]), where a `mind.toml` from version 2 gives
/// it the default beside the code.
///
/// What a `settings.json` stays while it says nothing of where installs land.
pub const SETTINGS_VERSION: u32 = 3;

/// The first [`MIND_FORMAT_VERSION`] whose settings may say where what is
/// installed into the project lands (`targets`), in place of the folders its
/// agents read.
///
/// A version of its own, where a key the format does not define is
/// otherwise read past: a build from before it would read past `targets`
/// too, and install into the standard folders without a word, where the
/// settings send copies elsewhere. A `settings.json` was written at it only
/// while it set them, so one that did not stayed one every build since
/// [`SETTINGS_VERSION`] reads. Nothing else in the file changes meaning, so
/// it is read as a version 3 file is, with `targets` beside.
pub const TARGETS_VERSION: u32 = 4;

/// The first [`MIND_FORMAT_VERSION`] whose marker is `.mind/settings.toml`
/// rather than `.mind/settings.json`: the same keys, in TOML written from a
/// template whose comments say what each one does, as `weaver.toml` is
/// (D-06, amended).
///
/// The one version that file holds, whatever keys it sets: every build that
/// reads one reads `targets` too, so there is no older reader for a version
/// of its own to keep out.
pub const SETTINGS_TOML_VERSION: u32 = 5;

/// The directory a mind project is identified by.
pub const MIND_DIR: &str = ".mind";
/// The marker file inside [`MIND_DIR`].
pub const MIND_CONFIG: &str = "mind.toml";
/// The directory a weaver workspace is identified by.
pub const WEAVER_DIR: &str = ".mindweaver";
/// The marker file inside [`WEAVER_DIR`].
pub const WEAVER_CONFIG: &str = "weaver.toml";
/// [`WEAVER_DIR`] as a Mindflayer build named it: the folder a workspace
/// one made is identified by, read and written under its names until
/// `migrate --names` moves it ([`Names::Mindflayer`]).
pub const FLAYER_DIR: &str = ".mindflayer";
/// [`WEAVER_CONFIG`] as a Mindflayer build named it, inside [`FLAYER_DIR`].
pub const FLAYER_CONFIG: &str = "flayer.toml";
/// Where a workspace keeps the clones it gathered from, inside [`WEAVER_DIR`].
///
/// Kept rather than discarded so a gather can be inspected after the fact, and
/// so what a source actually contained is still there when its report is not.
pub const CACHE_DIR: &str = "cache";
/// Where a workspace keeps its own artifacts when its marker does not say,
/// inside [`WEAVER_DIR`]: a folder per kind, named after it.
///
/// Inside `.mindweaver` rather than beside the projects, where a workspace
/// made before it keeps them, named in its marker: no agent looks in
/// `.mindweaver`, so the workspace in a home directory puts no `~/skills` or
/// `~/rules` above every project there, and a workspace is one folder
/// wherever it is made.
pub const LIBRARY_DIR: &str = "library";
/// Where an install builds its copy before moving it into place, inside a
/// project's [`MIND_DIR`], and a workspace's scratch folder, inside its
/// [`WEAVER_DIR`] ([`WeaverWorkspace::scratch_dir`]).
///
/// Not beside the copy, because that is a folder agents read, and not in the
/// system's temporary folder, which can be another filesystem: inside the
/// project, a finished copy is moved into place by a rename.
pub const SCRATCH_DIR: &str = "tmp";

/// Where a workspace's scratch folder is from its root, a folder at a time,
/// under `names`: the one place [`WeaverWorkspace::scratch_dir`] and
/// [`WeaverWorkspace::scratch_route`] both read it from, so the file a front
/// end writes and the route its line names cannot come apart.
const fn scratch(names: Names) -> [&'static str; 2] {
    [names.workspace_dir(), SCRATCH_DIR]
}

/// The `.gitignore` a local project's `.mind` is given: `*`, which keeps out
/// of git everything in the folder, this file included, as a Python venv
/// ignores itself.
///
/// A project turning shared loses the file only while it holds exactly what
/// Mindweaver writes, this or the one a Mindflayer build wrote
/// ([`FLAYER_MIND_IGNORE`], [`is_mind_ignore`]), so a new wording leaves no
/// file an older build wrote where it is, and none somebody changed goes.
pub const MIND_IGNORE: &str = "\
# Mindweaver keeps this project's settings here, for this checkout alone.
# Nothing in this folder is meant to be committed.
*
";

/// [`MIND_IGNORE`] as a Mindflayer build wrote it, its old name for the
/// product included (D-26): never written now, and still taken for
/// Mindweaver's own when a project turning shared drops the file.
pub const FLAYER_MIND_IGNORE: &str = "\
# Mindflayer keeps this project's settings here, for this checkout alone.
# Nothing in this folder is meant to be committed.
*
";

/// Whether `bytes`, what a `.mind/.gitignore` holds, are exactly what a
/// build wrote there, this one's or a Mindflayer build's: the one test of
/// whether the file is Mindweaver's to take away.
pub(crate) fn is_mind_ignore(bytes: &[u8]) -> bool {
    [MIND_IGNORE, FLAYER_MIND_IGNORE]
        .iter()
        .any(|ignore| bytes == ignore.as_bytes())
}

/// Which of the two marker files a version was read from.
///
/// Each is versioned on its own, so a refusal names the one that is newer: the
/// path alone leaves the reader to know which file is which level's, and a
/// front end deciding what to say about it should not have to compare file
/// names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    /// A mind project's [`MIND_CONFIG`].
    Project,
    /// A weaver workspace's [`WEAVER_CONFIG`].
    Workspace,
}

impl Marker {
    /// The newest version of this marker this build reads, and the one
    /// `mindweaver init` and `mind init` write into a new one.
    pub const fn version(self) -> u32 {
        match self {
            Marker::Project => MIND_FORMAT_VERSION,
            Marker::Workspace => WEAVER_FORMAT_VERSION,
        }
    }
}

impl fmt::Display for Marker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Marker::Project => "project marker",
            Marker::Workspace => "workspace marker",
        })
    }
}

/// Which of the files a project's marker has been a project is read from.
///
/// Each holds versions of its own, and a project is read from the newest one
/// it has ([`MindProject::open`]); the two before the newest are read the
/// way they were written, and moved to it only when somebody asks
/// (`mind migrate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerFile {
    /// `.mind/mind.toml`, versions 1 and 2: folders, and nothing else.
    MindToml,
    /// `.mind/settings.json`, versions 3 and 4.
    SettingsJson,
    /// `.mind/settings.toml`, from [`SETTINGS_TOML_VERSION`] on.
    SettingsToml,
}

impl MarkerFile {
    /// Its name, inside [`MIND_DIR`].
    pub const fn name(self) -> &'static str {
        match self {
            MarkerFile::MindToml => MIND_CONFIG,
            MarkerFile::SettingsJson => SETTINGS_JSON_FILE,
            MarkerFile::SettingsToml => SETTINGS_FILE,
        }
    }

    /// The file a marker of `version` is written in: what a version found
    /// in another file is refused as.
    pub fn of_version(version: u32) -> Self {
        if version < SETTINGS_VERSION {
            MarkerFile::MindToml
        } else if version < SETTINGS_TOML_VERSION {
            MarkerFile::SettingsJson
        } else {
            MarkerFile::SettingsToml
        }
    }
}

impl fmt::Display for MarkerFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// By its name, as a report says it (`ProjectStatus::marker`), rather than
/// derived from the variant: the spelling a reader finds in `.mind`, which a
/// renamed variant cannot change.
impl Serialize for MarkerFile {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

#[cfg(any(feature = "ts", feature = "schema"))]
crate::described::words! {
    /// The file inside `.mind` a project's marker is read from.
    MarkerFile by name { MindToml, SettingsJson, SettingsToml }
}

/// What an `init` actually did.
///
/// Initialising twice is not an error, and never overwrites the marker: the
/// caller is told which of the two happened and says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Initialization {
    /// The marker did not exist and now does.
    Created,
    /// The marker was already there and was left untouched.
    AlreadyInitialized,
}

/// What a `link` actually did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registration {
    /// The project was not registered and now is.
    Added,
    /// The project was already registered; the file was not touched.
    AlreadyRegistered,
}

// ---------------------------------------------------------------------------
// Mind projects
// ---------------------------------------------------------------------------

/// The contents of `.mind/mind.toml`, the marker of a version 1 or 2
/// project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MindConfig {
    /// The layout version this project was written with.
    pub version: u32,
    /// A human name for the project. Defaults to its directory's name.
    pub name: String,
    /// Where each kind's artifacts live, relative to the project root.
    #[serde(default)]
    pub directories: Directories,
}

/// Where a project keeps each kind of artifact.
///
/// Keyed by the kind's folder name (`skills`, `rules`), which is the plural
/// spelling the CLI already accepts, so there is no second table to keep in
/// step. A key this build does not recognise is carried along rather than
/// rejected, for the reason unknown front matter keys are: the format grows,
/// and a project configured by a newer Mindweaver is still a project.
///
/// What a kind left out means is the marker's to say, by its version. In a
/// `mind.toml` it is the default beside the code, so an empty table, what a
/// marker written by hand and never touched since will hold, means every
/// default. In a project's settings ([`SETTINGS_VERSION`]) it is no folder
/// at all.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Directories(BTreeMap<String, PathBuf>);

impl Directories {
    /// The directory configured for `kind`, if the project named one.
    pub fn get(&self, kind: Kind) -> Option<&Path> {
        self.0.get(kind.folder()).map(PathBuf::as_path)
    }

    /// Say where `kind` lives.
    pub fn with(mut self, kind: Kind, directory: impl Into<PathBuf>) -> Self {
        self.0.insert(kind.folder().to_owned(), directory.into());
        self
    }

    /// Say that `kind` lives nowhere this names: the default in a
    /// `mind.toml`, and no folder at all in a project's settings.
    pub fn without(mut self, kind: Kind) -> Self {
        self.0.remove(kind.folder());
        self
    }

    /// Whether anything was configured at all.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Say where whatever `key` names lives, a key this build may not know
    /// included: what a marker read from a newer build holds.
    pub(crate) fn with_key(mut self, key: &str, directory: impl Into<PathBuf>) -> Self {
        self.0.insert(key.to_owned(), directory.into());
        self
    }

    /// Every entry, the kinds this build knows first, in report order, then
    /// any other key in name order: the order a marker writes them in.
    pub(crate) fn entries(&self) -> Vec<(&str, &Path)> {
        let known = Kind::ALL
            .into_iter()
            .filter_map(|kind| self.0.get_key_value(kind.folder()));
        let other = self
            .0
            .iter()
            .filter(|(key, _)| Kind::ALL.iter().all(|kind| kind.folder() != key.as_str()));
        known
            .chain(other)
            .map(|(key, directory)| (key.as_str(), directory.as_path()))
            .collect()
    }

    /// What a project uses when it says nothing: a folder beside the code,
    /// named after the kind, for each kind that has one.
    ///
    /// Beside the code rather than inside `.mind`, because these are files the
    /// agents themselves read, and an agent looking for skills does not know
    /// what a `.mind` is.
    ///
    /// `None` for a kind kept nowhere unless a marker names its folder.
    /// Matched on the kind, so a kind added later has to say whether it has
    /// one: given one, every project a `mind.toml` marks, which says where
    /// the kinds it knew live and nothing of the rest, would start reading
    /// a folder of that name at its root that nobody put there for it, and
    /// every `mind init` and `mind migrate` would name it in the settings
    /// they write.
    pub fn default_for(kind: Kind) -> Option<PathBuf> {
        match kind {
            Kind::Skill | Kind::Rule => Some(PathBuf::from(kind.folder())),
            // Added once a marker could say where each kind lives, and a
            // folder called `mcp` at a repository's root is as likely to be
            // the code of a server as anything Mindweaver should read. A
            // project that authors definitions names the folder; the
            // workspace's own are in its library unless its marker names
            // one (D-05).
            Kind::Mcp => None,
        }
    }

    /// What a new workspace uses when it says nothing: a folder per kind in
    /// its library, `.mindweaver/library/<kind>` ([`LIBRARY_DIR`]).
    ///
    /// Not a project's default, which would put the workspace's own beside
    /// the repositories it manages, or in a home directory; and spelled with
    /// `/`, so the marker `init` writes reads the same on every platform.
    pub fn library_for(kind: Kind) -> PathBuf {
        Self::library_under(Names::Mindweaver, kind)
    }

    /// The same, in a workspace kept under `names`: `.mindflayer/library/<kind>`
    /// in one a Mindflayer build made, which is where its marker says nothing
    /// sends a kind.
    pub fn library_under(names: Names, kind: Kind) -> PathBuf {
        PathBuf::from(format!(
            "{}/{LIBRARY_DIR}/{}",
            names.workspace_dir(),
            kind.folder()
        ))
    }

    /// Every kind's directory spelled out, which is what `init` writes so the
    /// answer is in the file rather than in this function: each kind this
    /// names, and each it leaves out that has a default
    /// ([`default_for`](Self::default_for)).
    pub fn resolved(&self) -> Vec<(Kind, PathBuf)> {
        self.resolved_with(Directories::default_for)
    }

    /// Every kind named: one this leaves out at a project's default beside
    /// the code ([`default_for`](Self::default_for)), when it has one, and
    /// any key this build does not recognise carried along.
    ///
    /// What a new project's settings are given to author in
    /// ([`MindProject::init_asked`]). In settings a kind left out has no
    /// folder at all ([`SETTINGS_VERSION`]), so the `skills/` and `rules/` a
    /// new project still gets by default (D-06) are written down rather than
    /// left to a default. Crate-private, so that no front end spells the
    /// folders or decides when they are filled in.
    pub(crate) fn with_defaults(self) -> Self {
        self.filled(Directories::default_for)
    }

    /// The same, with `default` answering for a kind nothing names: a
    /// project's default or a workspace's. A kind neither names is left out.
    fn resolved_with(&self, default: impl Fn(Kind) -> Option<PathBuf>) -> Vec<(Kind, PathBuf)> {
        Kind::ALL
            .into_iter()
            .filter_map(|kind| {
                let directory = self
                    .get(kind)
                    .map(Path::to_path_buf)
                    .or_else(|| default(kind))?;
                Some((kind, directory))
            })
            .collect()
    }

    /// Every kind named, `default` answering for the ones that were not
    /// where it has an answer, and any key this build does not recognise
    /// carried along.
    fn filled(mut self, default: impl Fn(Kind) -> Option<PathBuf>) -> Self {
        for kind in Kind::ALL {
            if self.get(kind).is_none() {
                if let Some(directory) = default(kind) {
                    self.0.insert(kind.folder().to_owned(), directory);
                }
            }
        }
        self
    }
}

/// What a project's marker says, from whichever of the two files it is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectConfig {
    /// `.mind/mind.toml`: a project of version 1 or 2, read the way it was
    /// written.
    Toml(MindConfig),
    /// `.mind/settings.json` or `.mind/settings.toml`: a project from
    /// [`SETTINGS_VERSION`] on, whose version says which file.
    Settings(Settings),
}

impl ProjectConfig {
    /// The layout version the marker was written with.
    pub fn version(&self) -> u32 {
        match self {
            ProjectConfig::Toml(config) => config.version,
            ProjectConfig::Settings(settings) => settings.version,
        }
    }

    /// The project's declared name.
    pub fn name(&self) -> &str {
        match self {
            ProjectConfig::Toml(config) => &config.name,
            ProjectConfig::Settings(settings) => &settings.name,
        }
    }

    /// Where the marker says each kind lives. What a kind left out means
    /// depends on the version ([`MindProject::directory_for`]).
    pub fn directories(&self) -> &Directories {
        match self {
            ProjectConfig::Toml(config) => &config.directories,
            ProjectConfig::Settings(settings) => &settings.directories,
        }
    }
}

/// Something about a project's marker worth saying, that is no reason not to
/// read it.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProjectWarning {
    /// An older marker beside the one the project is read from: an older
    /// build would read it instead, and say other things about the project
    /// than this one does.
    #[error("{path}: ignored, since the project is read from the {read} beside it")]
    Leftover { path: PathBuf, read: MarkerFile },
}

codes!(ProjectWarning {
    Self::Leftover { .. } => "marker_left_over",
});

/// A folder a project's settings name as where a kind is installed that no
/// install uses: read, said, and passed over, rather than a reason to refuse
/// everything else the settings say ([`MindProject::misplaced_targets`]).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(
    "{settings}: {folder} is no folder to install {kinds} in, because {because}, \
     so nothing is installed there",
    kinds = .kind.plural(),
    because = .why.because(*.kind)
)]
pub struct MisplacedTarget {
    /// The settings file that names it.
    pub settings: PathBuf,
    pub kind: Kind,
    /// As the settings spell it, relative to the project's root.
    pub folder: PathBuf,
    pub why: Misplaced,
}

codes!(MisplacedTarget => "target_misplaced");

/// Why a folder a project's settings name is no place to install a kind in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Misplaced {
    /// It leads out of the project, or is its root: an absolute path, one
    /// climbing out with `..`, or `.`. What is installed is one project's,
    /// and a copy anywhere else is no project's, or another's.
    Outside,
    /// It is inside `.mind`, Mindweaver's own folder, which no agent reads
    /// and a local project keeps out of git.
    InsideMind,
    /// It is inside a `.mindweaver`, a workspace's own folder, or the
    /// `.mindflayer` a Mindflayer build made one, which no agent reads and
    /// whose shelf and cache gathering replaces.
    InsideWorkspace,
    /// It is inside a `.git`, a repository's own folder, which no agent
    /// reads and git owns: a copy there would sit among its refs and hooks,
    /// under a name git may mean to use.
    InsideGit,
    /// The kind is not installed into a project as a folder an artifact:
    /// rules, which D-03 gives a block of `AGENTS.md`, for M2 to say where.
    NotInstalled,
}

impl Misplaced {
    /// Why, as a clause after "because".
    pub fn because(self, kind: Kind) -> String {
        match self {
            Misplaced::Outside => String::from("it is not inside the project"),
            Misplaced::InsideMind => format!(
                "it is inside {MIND_DIR}, which holds Mindweaver's own files and no agent reads"
            ),
            Misplaced::InsideWorkspace => String::from(
                "it is inside a workspace's own folder, which holds its own files and no agent reads",
            ),
            Misplaced::InsideGit => format!(
                "it is inside {GIT_DIR}, which holds a repository's own files and no agent reads"
            ),
            Misplaced::NotInstalled => {
                format!(
                    "{} are not installed into a project as folders yet",
                    kind.plural()
                )
            }
        }
    }
}

/// A directory holding skills, identified by its `.mind`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MindProject {
    root: PathBuf,
    config: ProjectConfig,
    /// Each older marker found beside the one that was read instead.
    leftovers: Vec<PathBuf>,
}

impl MindProject {
    /// Create `.mind` and the folders its artifacts live in, under `root`,
    /// with every kind in its default place, marked by a `mind.toml` at
    /// [`MIND_TOML_VERSION`]: the project an older Mindweaver made.
    ///
    /// No front end makes one since `mind init` writes settings
    /// ([`init_asked`](Self::init_asked)). It stays for what has to
    /// start from such a project, since each one made before is still read
    /// the way it was written: the tests of how it is read, and of what a
    /// copy installed where it authors does.
    pub fn init(root: impl AsRef<Path>) -> Result<(Self, Initialization), WorkspaceError> {
        Self::init_with(root, &Directories::default())
    }

    /// The same, saying where one or more kinds should live instead.
    ///
    /// Existing files are never rewritten, so running this on a project that
    /// already exists is safe and only fills in what is missing. That also
    /// means `directories` is ignored by a second run: the marker already
    /// answered the question, and quietly moving where a project keeps its
    /// artifacts is not something `init` should do.
    pub fn init_with(
        root: impl AsRef<Path>,
        directories: &Directories,
    ) -> Result<(Self, Initialization), WorkspaceError> {
        let root = absolute(root.as_ref())?;
        let dir = root.join(MIND_DIR);
        let config_path = dir.join(MIND_CONFIG);

        for (kind, directory) in directories.resolved() {
            if !stays_inside(&directory) {
                return Err(WorkspaceError::OutsideProject {
                    path: directory,
                    kind,
                });
            }
        }
        create_dir(&dir)?;

        // Either marker, so a project read from its settings is not given a
        // `mind.toml` beside them that an older build would read instead.
        let already = Self::is_marked(&root);
        if !already {
            let name = directory_name(&root);
            write_new(&config_path, &mind_template(&name, directories))?;
        }

        // Opened before the folders are made, so a second run creates what the
        // *marker* asks for rather than what this call was passed.
        let project = Self::open(&root)?;
        // A folder per kind, so the layout is visible in a fresh project
        // rather than being something you have to read the docs to discover.
        for kind in Kind::ALL {
            if let Some(directory) = project.directory_for(kind) {
                create_dir(&directory)?;
            }
        }

        Ok((
            project,
            if already {
                Initialization::AlreadyInitialized
            } else {
                Initialization::Created
            },
        ))
    }

    /// Make `root` a project as `mind init` does:
    /// [`init_settings`](Self::init_settings), with each kind in the folder
    /// `asked` names, or else at its default beside the code.
    ///
    /// The fill is core's, not the front end's, because which folders a new
    /// project gets is a rule (D-06: `skills/` and `rules/` unless told
    /// otherwise), and a front end handing `init_settings` only what it was
    /// asked would make a project with no folder for the rest. That one
    /// takes its folders as given, for what needs a project that keeps none.
    pub fn init_asked(
        root: impl AsRef<Path>,
        agents: &[Agent],
        share: Share,
        asked: &Directories,
    ) -> Result<(Self, Initialization), WorkspaceError> {
        Self::init_settings(root, agents, share, &asked.clone().with_defaults())
    }

    /// Make `root` a project marked by its settings
    /// ([`SETTINGS_TOML_VERSION`]): `.mind/settings.toml`, written from its
    /// template ([`Settings::text`]), a `.mind/.gitignore` holding `*` unless the
    /// project is [shared](Share::Shared), and the folder of each kind
    /// `directories` names. A kind it leaves out has no folder, and none is
    /// made for it.
    ///
    /// The `.gitignore` is the venv's way of keeping a folder out of git
    /// (D-01): inside the folder it is about, so it needs no repository yet,
    /// goes with a copy of the project and goes when `.mind` does, and
    /// changes no file the team owns. A shared project's settings are the
    /// team's, to commit.
    ///
    /// As [`init_with`](Self::init_with), never a rewrite: a project already
    /// marked, by settings or by a `mind.toml`, keeps its marker, and only
    /// what its marker asks for and is missing is made, a folder or the
    /// `.gitignore` of a local project. Moving a project from a `mind.toml`
    /// or a `settings.json` to a `settings.toml` is a migration's (#38), not
    /// this.
    pub fn init_settings(
        root: impl AsRef<Path>,
        agents: &[Agent],
        share: Share,
        directories: &Directories,
    ) -> Result<(Self, Initialization), WorkspaceError> {
        let root = absolute(root.as_ref())?;
        let dir = root.join(MIND_DIR);

        for kind in Kind::ALL {
            if let Some(error) = directories
                .get(kind)
                .and_then(|directory| misplaced_authored(kind, directory))
            {
                return Err(error);
            }
        }
        let settings = Settings::new(
            directory_name(&root),
            agents.to_vec(),
            share,
            directories.clone(),
        );
        let text = settings.text()?;
        create_dir(&dir)?;

        let already = Self::is_marked(&root);
        if !already {
            // Before the marker, so a local project's settings never show
            // in `git status`, not even for a moment.
            if share == Share::Local {
                ignore_itself(&dir)?;
            }
            write_new(&dir.join(SETTINGS_FILE), &text)?;
        }

        // What the marker asks for, as `init_with` makes the folders: a
        // `.gitignore` deleted from a local project is made again, and none
        // is made for a shared one, or for one still marked by a
        // `mind.toml`, which was meant to be committed.
        let project = Self::open(&root)?;
        if project
            .settings()
            .is_some_and(|settings| settings.share == Share::Local)
        {
            ignore_itself(&dir)?;
        }
        for kind in Kind::ALL {
            if let Some(directory) = project.directory_for(kind) {
                create_dir(&directory)?;
            }
        }

        Ok((
            project,
            if already {
                Initialization::AlreadyInitialized
            } else {
                Initialization::Created
            },
        ))
    }

    /// Open the mind project rooted at `root`, from the newest marker it
    /// has: its `settings.toml`, else its `settings.json`, else its
    /// `mind.toml`.
    ///
    /// The newest wins when more than one is there: an older one beside it is
    /// one an upgrade left, or a copy brought back. Still a project rather
    /// than a refusal, since either file says what it is, but a
    /// [warning](Self::warnings) names each one ignored.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let root = absolute(root.as_ref())?;
        let mind = root.join(MIND_DIR);
        let mut found = [
            MarkerFile::SettingsToml,
            MarkerFile::SettingsJson,
            MarkerFile::MindToml,
        ]
        .into_iter()
        .map(|file| (file, mind.join(file.name())))
        .filter(|(_, path)| path.is_file());
        let Some((file, path)) = found.next() else {
            return Err(WorkspaceError::NotAProject { path: root });
        };
        let leftovers = found.map(|(_, path)| path).collect();
        if file != MarkerFile::MindToml {
            return Ok(Self {
                root,
                config: ProjectConfig::Settings(Settings::read(&path)?),
                leftovers,
            });
        }
        let config: MindConfig = read_config(&path)?;
        check_version(&path, Marker::Project, config.version)?;
        // Not newer than this build, and not a version a `mind.toml` holds
        // either: from `SETTINGS_VERSION` on, the marker is another file, and
        // what a `mind.toml` claiming that version means is nobody's to say.
        if config.version >= SETTINGS_VERSION {
            return Err(WorkspaceError::Malformed {
                path,
                detail: format!(
                    "version {} is written as {}, not as {MIND_CONFIG}, which holds versions up to {MIND_TOML_VERSION}",
                    config.version,
                    MarkerFile::of_version(config.version)
                ),
            });
        }
        Ok(Self {
            root,
            config: ProjectConfig::Toml(config),
            leftovers,
        })
    }

    /// Walk up from `start` looking for a mind project.
    pub fn locate(start: impl AsRef<Path>) -> Result<Option<Self>, WorkspaceError> {
        match locate_marker(start.as_ref(), Self::is_marked)? {
            Some(root) => Self::open(root).map(Some),
            None => Ok(None),
        }
    }

    /// Whether `root` carries a project's marker, in either file: what makes
    /// a directory a project, to everything that looks for one.
    ///
    /// The file, not the `.mind` alone, so an empty `.mind` left behind by a
    /// failed copy is not taken for a project; and any of them, so a project
    /// made by this build and one made by an older one are found alike.
    pub fn is_marked(root: &Path) -> bool {
        let mind = root.join(MIND_DIR);
        [
            MarkerFile::SettingsToml,
            MarkerFile::SettingsJson,
            MarkerFile::MindToml,
        ]
        .into_iter()
        .any(|file| mind.join(file.name()).is_file())
    }

    /// Whether nothing at `root` says a project is there: each marker file
    /// looked for and found missing, as where a project moved from is left.
    ///
    /// Not merely the opposite of [`is_marked`](Self::is_marked), which a
    /// marker that cannot be looked at, in a `.mind` nobody may read, fails
    /// too. A directory still holding a marker, or one that may, holds its
    /// project, even when the marker would not open (a typo, or a newer
    /// version): that project's id is what nothing read, so a copy of it
    /// made with its `.mind` would pass for it moved ([`crate::relink`]).
    pub fn is_vacant(root: &Path) -> bool {
        let mind = root.join(MIND_DIR);
        [
            MarkerFile::SettingsToml,
            MarkerFile::SettingsJson,
            MarkerFile::MindToml,
        ]
        .into_iter()
        .all(|file| match fs::symlink_metadata(mind.join(file.name())) {
            Ok(_) => false,
            Err(error) => matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ),
        })
    }

    /// Which file the project is read from.
    pub fn marker_file(&self) -> MarkerFile {
        match &self.config {
            ProjectConfig::Toml(_) => MarkerFile::MindToml,
            ProjectConfig::Settings(settings) => MarkerFile::of_version(settings.version),
        }
    }

    /// The file the project is read from, whether or not it is there still.
    pub fn marker_path(&self) -> PathBuf {
        self.mind_dir().join(self.marker_file().name())
    }

    /// The directory holding `.mind`.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The `.mind` directory itself.
    pub fn mind_dir(&self) -> PathBuf {
        self.root.join(MIND_DIR)
    }

    /// Where an install into this project builds its copy ([`SCRATCH_DIR`]),
    /// whether or not one has made it yet.
    pub fn scratch_dir(&self) -> PathBuf {
        self.mind_dir().join(SCRATCH_DIR)
    }

    /// The folder holding one kind, whether or not it exists yet, or `None`
    /// when the project keeps none of that kind of its own.
    ///
    /// What the marker says, or, when it says nothing, what its version
    /// meant by that. A project's settings ([`SETTINGS_VERSION`]) mean no
    /// folder at all. A `mind.toml` means the default beside the code, except
    /// one written before [`DIRECTORIES_VERSION`], which described a layout
    /// where every kind lived inside `.mind`; and either means no folder for
    /// a kind with no default ([`Directories::default_for`]), which it was
    /// not written knowing. Each is read the way it was written.
    ///
    /// An `Option` rather than a folder always, so that a project can say it
    /// authors no artifacts of a kind: a reader then finds none there, which
    /// is an absence and not a failure, and a change that needs the folder
    /// asks [`required_directory`](Self::required_directory), which refuses.
    /// Answering with a default instead would put a kind added later in a
    /// folder at the project's root that nobody asked for.
    pub fn directory_for(&self, kind: Kind) -> Option<PathBuf> {
        if let Some(directory) = self.config.directories().get(kind) {
            return Some(self.root.join(directory));
        }
        match &self.config {
            ProjectConfig::Settings(_) => None,
            // Every kind it knew, which is every kind with a default: one
            // added since is kept nowhere it does not name.
            ProjectConfig::Toml(config) if config.version < DIRECTORIES_VERSION => {
                Directories::default_for(kind).map(|_| self.mind_dir().join(kind.folder()))
            }
            ProjectConfig::Toml(_) => {
                Directories::default_for(kind).map(|directory| self.root.join(directory))
            }
        }
    }

    /// [`directory_for`](Self::directory_for), for what has to have the
    /// folder to do anything at all: an artifact made there, a copy put
    /// there. A project that keeps no artifacts of `kind` is refused,
    /// rather than given a folder its marker never named.
    pub fn required_directory(&self, kind: Kind) -> Result<PathBuf, WorkspaceError> {
        self.directory_for(kind)
            .ok_or_else(|| WorkspaceError::NoDirectory {
                path: self.root.clone(),
                kind,
                marker: self.marker_file(),
            })
    }

    /// Where `kind` goes in the project for each agent its settings name
    /// ([`TargetTable::targets`]), whether or not a folder is there yet;
    /// none for a project a `mind.toml` marks, which names no agents.
    ///
    /// The project hands its table on, rather than a caller reaching for
    /// [`TargetTable::standard`]: which table applies is the settings' to
    /// say. The standard one, overridden for each kind the settings name
    /// `targets` for ([`TargetTable::overriding`]) with every folder an
    /// install may use, and only those: a folder written there by hand that
    /// leads out of the project, or into `.mind`, is passed over and said
    /// ([`misplaced_targets`](Self::misplaced_targets)). A kind all of whose
    /// folders are passed over goes nowhere, rather than back to the
    /// standard folders, which the settings said not to use.
    pub fn targets(&self, kind: Kind) -> Vec<AgentTarget> {
        self.settings().map_or_else(Vec::new, |settings| {
            let mut table = TargetTable::standard();
            if let Some(folders) = settings.targets.get(kind) {
                let placed: Vec<PathBuf> = folders
                    .iter()
                    .filter(|folder| misplaced_target(kind, folder).is_none())
                    .map(|folder| paths::normalize(folder))
                    .collect();
                table = table.overriding(kind, &placed);
            }
            table.targets(kind, &settings.agents)
        })
    }

    /// Every folder the project's settings name in `targets` that no install
    /// uses, kind by kind in report order, each folder in the order written:
    /// a failure collected rather than raised, for a front end to say where
    /// it installs or reports, since the rest of the settings still stand.
    pub fn misplaced_targets(&self) -> Vec<MisplacedTarget> {
        let Some(settings) = self.settings() else {
            return Vec::new();
        };
        let path = self.marker_path();
        Kind::ALL
            .into_iter()
            .flat_map(|kind| {
                settings
                    .targets
                    .get(kind)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(move |folder| Some((kind, folder, misplaced_target(kind, folder)?)))
            })
            .map(|(kind, folder, why)| MisplacedTarget {
                settings: path.clone(),
                kind,
                folder: folder.clone(),
                why,
            })
            .collect()
    }

    /// The project's declared name.
    pub fn name(&self) -> &str {
        self.config.name()
    }

    /// The parsed marker file, whichever it is.
    pub fn config(&self) -> &ProjectConfig {
        &self.config
    }

    /// The project's settings, when its marker is a settings file; `None`
    /// for a project still marked by a `mind.toml`, which has none of what
    /// they add.
    pub fn settings(&self) -> Option<&Settings> {
        match &self.config {
            ProjectConfig::Settings(settings) => Some(settings),
            ProjectConfig::Toml(_) => None,
        }
    }

    /// This project as it would be read with `settings` for its marker,
    /// nothing written: for asking where it would install, and what it would
    /// read, before a change to its settings is made
    /// ([`crate::reapply`]).
    pub(crate) fn with_settings(&self, settings: Settings) -> Self {
        Self {
            root: self.root.clone(),
            config: ProjectConfig::Settings(settings),
            leftovers: self.leftovers.clone(),
        }
    }

    /// Change the project's settings as `change` changes a copy of them, and
    /// write back only what it changed.
    ///
    /// The file is read here, not taken from what was read when the project
    /// was opened, so a change is made to the file it is about to write and
    /// not to a copy that may be minutes old, as a workspace's marker is
    /// edited ([`WeaverWorkspace`]'s `edit_array`), through the same
    /// `rewrite`. Each key whose value changed is edited where it stands,
    /// through toml_edit as `weaver.toml` is (`settings::edited`), so a key
    /// this build does not know, the order of the keys, the comments and the
    /// file's layout all stay as somebody left them. A changed file is
    /// written through a temporary and a rename (`replace_file`), so a crash
    /// leaves the old settings or the new ones; an unchanged one is not
    /// written at all, its modification time included. What is in memory
    /// afterwards is read back from the file.
    ///
    /// Refused for a project an older marker marks
    /// ([`changeable`](Self::changeable)), when the change touches the
    /// version or the id, and when it moves a kind's folder out of the
    /// project or into its `.mind`, as `init` refuses one, or sends a kind's
    /// installs to a folder no install would use: strict when writing, while
    /// a folder somebody wrote there by hand is still read, and left alone by
    /// a change to something else.
    /// A settings file this build refuses to read is refused here too, and
    /// left as it is.
    ///
    /// Takes no lock, as `config_entry::ConfigFile` takes none: its caller
    /// holds the one that keeps its change apart. A change to the agents or
    /// the share mode carries what is installed along, which is the writing
    /// of a workspace's copies, ledger and exclude entries, so
    /// [`crate::reapply::apply`] holds the lock of the workspace that links
    /// the project from before it reads the settings until all of it is
    /// written. A lock of the project's own, in `.mind`, would keep out the
    /// writers of a second workspace linking it too, but not their installs,
    /// which go by that workspace's lock (an open question).
    pub fn change_settings<T>(
        &mut self,
        change: impl FnOnce(&mut Settings) -> T,
    ) -> Result<T, WorkspaceError> {
        self.changeable()?;
        let path = self.marker_path();
        let value = rewrite(&path, |text| {
            let before = crate::settings::parse(&path, text)?;
            let mut wanted = before.clone();
            let value = change(&mut wanted);
            Self::check_change(&path, &before, &wanted)?;
            let edited = crate::settings::edited(&path, text, &before, &wanted)?;
            Ok((edited, value))
        })?;
        self.config = ProjectConfig::Settings(Settings::read(&path)?);
        Ok(value)
    }

    /// Give the project `id` in place of the one its settings hold, and hand
    /// back that one: the one change [`change_settings`](Self::change_settings)
    /// refuses, made the way it makes the others, through the same rewrite and
    /// toml_edit, with every other byte as it was (`settings::renewed`).
    ///
    /// Refused as a change is ([`changeable`](Self::changeable)), and taking
    /// no lock, as a change takes none: [`crate::relink::new_id`] holds the
    /// one of the workspace linking the project, and keeps its exclude group
    /// and ledger with it.
    pub(crate) fn change_id(&mut self, id: &ProjectId) -> Result<ProjectId, WorkspaceError> {
        self.changeable()?;
        let path = self.marker_path();
        let was = rewrite(&path, |text| {
            let before = crate::settings::parse(&path, text)?;
            Ok((crate::settings::renewed(&path, text, id)?, before.id))
        })?;
        self.config = ProjectConfig::Settings(Settings::read(&path)?);
        Ok(was)
    }

    /// The project's settings, when a change may be made to them: those of
    /// a project its `settings.toml` marks.
    ///
    /// Refused for a project an older marker marks, pointing at the
    /// migration: a `mind.toml` has no settings to change, and a
    /// `settings.json` is read as it was written and changed by nothing but
    /// its move to a `settings.toml`, so the settings are changed one way,
    /// as `weaver.toml` is, and never in a grammar this build no longer
    /// writes.
    pub fn changeable(&self) -> Result<&Settings, WorkspaceError> {
        match &self.config {
            ProjectConfig::Settings(settings) if self.marker_file() == MarkerFile::SettingsToml => {
                Ok(settings)
            }
            _ => Err(WorkspaceError::NoSettings {
                path: self.root.clone(),
                marker: self.marker_file(),
            }),
        }
    }

    /// What [`change_settings`](Self::change_settings) holds `wanted`, the
    /// settings as a change leaves them, to before it writes them.
    fn check_change(
        path: &Path,
        before: &Settings,
        wanted: &Settings,
    ) -> Result<(), WorkspaceError> {
        for kind in Kind::ALL {
            if let Some(error) = wanted
                .directories
                .get(kind)
                .filter(|directory| before.directories.get(kind) != Some(*directory))
                .and_then(|directory| misplaced_authored(kind, directory))
            {
                return Err(error);
            }
            // Strict when writing, as for a folder authored in: a kind whose
            // targets change is refused any folder no install would use, and
            // a list with none, which the settings read as malformed.
            let targets = wanted.targets.get(kind);
            if targets == before.targets.get(kind) {
                continue;
            }
            let folders = targets.unwrap_or_default();
            if targets.is_some() && folders.is_empty() {
                return Err(WorkspaceError::Edit {
                    path: path.to_path_buf(),
                    detail: format!(
                        "`targets.{}` would name no folder, which is no place to install anything",
                        kind.folder()
                    ),
                });
            }
            if let Some((folder, why)) = folders
                .iter()
                .find_map(|folder| Some((folder, misplaced_target(kind, folder)?)))
            {
                return Err(WorkspaceError::Target {
                    path: folder.clone(),
                    kind,
                    why,
                });
            }
        }
        Ok(())
    }

    /// What opening the project found worth saying, for a front end to say:
    /// collected rather than raised, since none of it stops the project
    /// being read.
    pub fn warnings(&self) -> Vec<ProjectWarning> {
        let read = self.marker_file();
        self.leftovers
            .iter()
            .map(|path| ProjectWarning::Leftover {
                path: path.clone(),
                read,
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Weaver workspaces
// ---------------------------------------------------------------------------

/// The contents of `.mindweaver/weaver.toml`, or of the `.mindflayer/flayer.toml`
/// a Mindflayer build wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeaverConfig {
    /// The layout version this workspace was written with.
    pub version: u32,
    /// A human name for the workspace. Defaults to its directory's name.
    pub name: String,
    /// The mind projects this workspace manages, relative to its root.
    #[serde(default)]
    pub projects: Vec<PathBuf>,
    /// Other mindweaver workspaces whose own artifacts this one offers to its
    /// projects, read live from where they are; relative to its root.
    #[serde(default)]
    pub loaded: Vec<PathBuf>,
    /// The other weaver workspaces this machine knows, relative to its root:
    /// read only in the default workspace, the one in home, which
    /// [`crate::known`] keeps it in.
    ///
    /// No new [`WEAVER_FORMAT_VERSION`]: a build from before it reads past
    /// the key, which says nothing about this workspace, and keeps it when
    /// it edits the marker, since every edit goes through `toml_edit`.
    #[serde(default)]
    pub known: Vec<PathBuf>,
    /// Where the workspace keeps its own artifacts (the ones it shares with
    /// every project it manages), relative to its root. A kind it does not
    /// name is kept in its library ([`Directories::library_for`]).
    #[serde(default)]
    pub directories: Directories,
}

/// A directory orchestrating several mind projects, identified by its
/// `.mindweaver`, or by the `.mindflayer` of one a Mindflayer build made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeaverWorkspace {
    root: PathBuf,
    config: WeaverConfig,
    /// Whether its marker is on disk: false only for an
    /// [`unmade`](Self::unmade) one.
    made: bool,
    /// Whose names its folder, marker and ledger go by: Mindweaver's for
    /// every one made now, and kept as they are found ([`Names`]).
    names: Names,
    /// The folder of a workspace under older names beside the one read
    /// ([`leftover`](Self::leftover)).
    leftover: Option<PathBuf>,
}

impl WeaverWorkspace {
    /// Create `.mindweaver` and its marker file under `root`.
    ///
    /// The registry starts empty: a workspace manages the projects it is told
    /// about, and guessing which neighbouring directories were meant would be
    /// the kind of surprise that is hard to undo.
    ///
    /// A `root` that holds a workspace already, under either names, is that
    /// one, and nothing is made: one a Mindflayer build made is opened as it
    /// is ([`Initialization::AlreadyInitialized`]), never given a
    /// `.mindweaver` beside it, which would leave two workspaces in one
    /// folder, each read by one build and missed by the other.
    pub fn init(root: impl AsRef<Path>) -> Result<(Self, Initialization), WorkspaceError> {
        Self::init_with(root, &Directories::default())
    }

    /// The same, saying where the workspace keeps its own artifacts.
    ///
    /// A kind `directories` does not name is kept in the workspace's library
    /// ([`Directories::library_for`]), and the marker says so.
    ///
    /// Unlike a project's, those folders are not made here, wherever they
    /// are. One told to keep them beside the repositories it manages puts
    /// nothing among other people's work until there is something to put,
    /// and the library follows the same rule rather than a second one: the
    /// first artifact added makes its folder. As with a project, a second run
    /// changes nothing, `directories` included.
    ///
    /// A directory outside the workspace, or one overlapping the shelf or the
    /// cache, which are reserved for gathering, is refused before anything is
    /// made.
    pub fn init_with(
        root: impl AsRef<Path>,
        directories: &Directories,
    ) -> Result<(Self, Initialization), WorkspaceError> {
        Self::init_under(root.as_ref(), directories, Names::Mindweaver)
    }

    /// [`init_with`](Self::init_with), under `names`: Mindweaver's for every
    /// workspace a command makes, and Mindflayer's only for a test's mother
    /// of what such a build made ([`crate::mother::MindflayerMother`]), so
    /// no build makes a workspace under the old names.
    pub(crate) fn init_under(
        root: &Path,
        directories: &Directories,
        names: Names,
    ) -> Result<(Self, Initialization), WorkspaceError> {
        let root = absolute(root)?;
        let dir = root.join(names.workspace_dir());
        let config_path = dir.join(names.workspace_config());

        for (kind, directory) in
            directories.resolved_with(|kind| Some(Directories::library_under(names, kind)))
        {
            if let Some(error) = misplaced_own(kind, &directory) {
                return Err(error);
            }
        }
        if Self::is_marked(&root) {
            return Ok((Self::open(&root)?, Initialization::AlreadyInitialized));
        }
        create_dir(&dir)?;

        let name = directory_name(&root);
        write_new(&config_path, &weaver_template(&name, directories, names))?;
        Ok((Self::open(&root)?, Initialization::Created))
    }

    /// Open the workspace rooted at `root`, under whichever names its marker
    /// is found by: Mindweaver's first, then Mindflayer's.
    ///
    /// The new wins when both are there, as a project's newest marker does:
    /// an old one beside it is one a Mindflayer build made since, or a copy
    /// brought back. Still a workspace rather than a refusal, since the new
    /// marker says what it is, but [`leftover`](Self::leftover) names the old
    /// folder, which an older build reads instead.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let root = absolute(root.as_ref())?;
        let mut found = Names::ALL
            .into_iter()
            .filter(|names| marker_under(&root, *names).is_file());
        let Some(names) = found.next() else {
            return Err(WorkspaceError::NotAWorkspace { path: root });
        };
        let leftover = found.next().map(|older| root.join(older.workspace_dir()));
        let config_path = marker_under(&root, names);
        let config: WeaverConfig = read_config(&config_path)?;
        check_version(&config_path, Marker::Workspace, config.version)?;
        Ok(Self {
            root,
            config,
            made: true,
            names,
            leftover,
        })
    }

    /// Whether `root` holds a workspace's marker, under either names: a
    /// name looked for, never a file read, so a marker that would not open
    /// still says a workspace is there, as [`MindProject::is_marked`] says
    /// a project is.
    pub fn is_marked(root: &Path) -> bool {
        Self::names_at(root).is_some()
    }

    /// Whose names the workspace at `root` is kept under, the new looked
    /// for first ([`Names::ALL`]): `None` when no marker is there.
    pub fn names_at(root: &Path) -> Option<Names> {
        Names::ALL
            .into_iter()
            .find(|names| marker_under(root, *names).is_file())
    }

    /// The workspace [`init`](Self::init) would make at `root`, held in memory
    /// rather than made: what a command that only reads is given where the
    /// default workspace does not exist yet, since reading is no reason to
    /// make it.
    ///
    /// Its marker is the one `init` would write, parsed instead of written, so
    /// a read answers as it would of a workspace made a moment before (no
    /// projects, nothing loaded, an empty ledger), and coming first changes
    /// nothing about what it says. Nothing is written through it: an edit of
    /// its marker is refused as it is for any directory that is not a
    /// workspace, and so is its ledger. What writes makes the real one:
    /// [`resolve`](crate::resolve::resolve) does, for a command that means to
    /// write.
    pub fn unmade(root: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let root = absolute(root.as_ref())?;
        let names = Names::Mindweaver;
        let marker = weaver_template(&directory_name(&root), &Directories::default(), names);
        let config = toml::from_str(&marker).map_err(|source| WorkspaceError::Parse {
            path: marker_under(&root, names),
            source,
        })?;
        Ok(Self {
            root,
            config,
            made: false,
            names,
            leftover: None,
        })
    }

    /// Whether it is on disk, which every workspace is but an
    /// [`unmade`](Self::unmade) one.
    pub fn is_made(&self) -> bool {
        self.made
    }

    /// Walk up from `start` looking for a weaver workspace, under either
    /// names.
    pub fn locate(start: impl AsRef<Path>) -> Result<Option<Self>, WorkspaceError> {
        match locate_marker(start.as_ref(), Self::is_marked)? {
            Some(root) => Self::open(root).map(Some),
            None => Ok(None),
        }
    }

    /// Whether this is the default workspace at `home` rather than one found
    /// up the tree.
    pub fn is_default(&self, home: Option<&Path>) -> bool {
        home.and_then(|home| absolute(home).ok())
            .is_some_and(|home| home == self.root)
    }

    /// The directory holding `.mindweaver`.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The `.mindweaver` directory itself, or the `.mindflayer` of one a
    /// Mindflayer build made.
    pub fn weaver_dir(&self) -> PathBuf {
        self.root.join(self.names.workspace_dir())
    }

    /// Whose names its folder, marker and ledger go by.
    pub fn names(&self) -> Names {
        self.names
    }

    /// The folder of a workspace kept under older names beside this one,
    /// when there is one: a `.mindflayer` beside the `.mindweaver` read,
    /// which an older build reads instead, and so says other things about
    /// the workspace than this one does. Collected rather than raised, for
    /// a front end to say, as [`MindProject::warnings`] names an older
    /// marker beside a project's.
    pub fn leftover(&self) -> Option<&Path> {
        self.leftover.as_deref()
    }

    /// The workspace's declared name.
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// The parsed marker file.
    pub fn config(&self) -> &WeaverConfig {
        &self.config
    }

    /// The marker file itself.
    pub fn config_path(&self) -> PathBuf {
        marker_under(&self.root, self.names)
    }

    /// Where clones are kept, whether or not it exists yet.
    pub fn cache_dir(&self) -> PathBuf {
        self.weaver_dir().join(CACHE_DIR)
    }

    /// The workspace's scratch folder ([`SCRATCH_DIR`] in `.mindweaver`),
    /// whether or not anything has made it yet: where a front end puts what
    /// it hands a command as a file and takes away after, the panel's text
    /// for `edit --contents` among it, and where a gather's dry run fetches
    /// and copies what it says of, in a folder of its own it takes away
    /// after ([`crate::gather::dry_run`]).
    ///
    /// The workspace's own, as `.mind/tmp` is a project's, and named alike:
    /// inside `.mindweaver`, where no agent looks and a workspace keeps what
    /// is Mindweaver's, never in the system's temporary folder, where
    /// another user could read what is about to be written. Not a project's
    /// `.mind/tmp`, even for a project's artifact: a cloned repository can
    /// bring that folder, or a link in its place, with it.
    pub fn scratch_dir(&self) -> PathBuf {
        self.root.join(PathBuf::from_iter(scratch(self.names)))
    }

    /// The workspace's scratch folder ([`scratch_dir`](Self::scratch_dir))
    /// as a route from its root, `/` separated: how a command line run from
    /// the root names a file there, as the panel's save does its line.
    pub fn scratch_route(&self) -> String {
        Self::scratch_route_under(self.names)
    }

    /// The same, for a workspace under `names`: what a front end spells
    /// with no workspace at hand, under Mindweaver's for one made now.
    pub fn scratch_route_under(names: Names) -> String {
        scratch(names).join("/")
    }

    /// Where artifacts gathered from elsewhere live, one folder per kind.
    ///
    /// The same shape a mind project uses, one level up, so installing one
    /// into a project is a copy rather than a translation.
    pub fn gathered_dir(&self, kind: Kind) -> PathBuf {
        self.weaver_dir().join(kind.folder())
    }

    /// The database recording what was gathered and what was done:
    /// `mindweaver.db`, or the `mindflayer.db` of one a Mindflayer build
    /// made.
    pub fn ledger_path(&self) -> PathBuf {
        self.weaver_dir().join(self.names.ledger_file())
    }

    /// The file a writer locks ([`lock::LOCK_FILE`]), whether or not one has
    /// made it yet.
    pub fn lock_path(&self) -> PathBuf {
        self.weaver_dir().join(lock::LOCK_FILE)
    }

    /// Hold this workspace for a change, waiting up to [`lock::WAIT`] for a
    /// writer that holds it, then failing with [`LockError::Busy`], which
    /// names that writer's process.
    ///
    /// Core's writers take it themselves (an edit of the marker, an install
    /// or an uninstall, a gather, the scrub that opening the ledger to write
    /// runs when it has clones to rewrite), so a front end
    /// has no need to, and must not while it asks core to write: the lock is
    /// not re-entrant, and a writer that asks for it twice waits for itself.
    /// Moving the workspace's own into its library
    /// ([`crate::migrate::library`]) takes it too, from its plan to its
    /// rewrite of the marker. Readers never take it.
    pub fn lock(&self) -> Result<WorkspaceLock, LockError> {
        self.lock_waiting(lock::WAIT)
    }

    /// [`lock`](Self::lock), waiting up to `wait` rather than [`lock::WAIT`]:
    /// [`Duration::ZERO`] tries once, and a wait too long to count from now,
    /// [`Duration::MAX`], never gives up.
    pub fn lock_waiting(&self, wait: Duration) -> Result<WorkspaceLock, LockError> {
        WorkspaceLock::acquire(&self.weaver_dir(), wait)
    }

    /// Open that database, creating it if this workspace has never had one.
    ///
    /// Refused for an [`unmade`](Self::unmade) workspace, which has nowhere to
    /// keep one: creating it would leave half a workspace behind, a
    /// `.mindweaver` holding a database and no marker.
    ///
    /// What a command opens it through, rather than how: one that writes to it
    /// opens it with [`WeaverWorkspace::ledger_to_write`], which scrubs it
    /// first, and one that only reads with
    /// [`WeaverWorkspace::ledger_for_reading`], which neither makes it nor
    /// scrubs it.
    ///
    /// Opened without the workspace's [`lock`](Self::lock), though opening is
    /// when a ledger is made or migrated: a migration is one transaction that
    /// reads the version again once it holds SQLite's write lock
    /// ([`Ledger::open`]), so two processes opening one an older build left
    /// take turns there, and the second finds nothing to do. That keeps a
    /// read's migration apart from a writer's too, which the workspace's lock
    /// could not, since readers never take it. Taking it here as well would
    /// only keep every change to an artifact waiting behind a gather to drop
    /// its record, after the change was made, and failing as busy when the
    /// gather ran long, with the record left behind.
    ///
    /// A ledger made or migrated here is told the id of each project the
    /// workspace links (`WeaverWorkspace::known`), which the step to schema 5
    /// names each one's records by. What the step to schema 7 made one is
    /// left for a command that writes to say and clear ([`gather::scrub`]).
    pub fn ledger(&self) -> Result<Ledger, LedgerError> {
        if !self.made {
            return Err(LedgerError::Unmade {
                path: self.ledger_path(),
            });
        }
        Ledger::open_knowing(self.ledger_path(), || self.known().ids)
    }

    /// Open that database for a command that writes to it: scrubbed first of
    /// what an older build stored with a credential ([`gather::scrub`]), with
    /// what the scrub took out handed back for the front end to say.
    ///
    /// The one way to open it for writing, rather than a scrub each front end
    /// remembers to run: which commands write is the front end's to know, as
    /// whether one may create a workspace is with the
    /// [`Intent`](crate::resolve::Intent) it hands
    /// [`resolve`](crate::resolve::resolve), but what opening to write
    /// involves is core's, so no front end can write first and scrub after,
    /// or not at all.
    ///
    /// The scrub holds the workspace while it rewrites clones, which is only
    /// when an older build left a credential in one, and like any writer gives
    /// up as busy after [`lock::WAIT`].
    ///
    /// And told which id each project the workspace links holds now
    /// ([`Ledger::knew`](crate::port::LedgerStore::knew)), so it follows them: one linked since it was made,
    /// one given a new id, one whose route held another project before. A
    /// stale entry keeps the id the ledger last saw there, which is what
    /// finds its project once it has moved ([`crate::relink`]). Every time,
    /// as the scrub runs every time, rather than by each command that
    /// changes what the workspace links: an entry written by hand is linked
    /// too. Rows only, which SQLite keeps apart, so it waits for no lock.
    pub fn ledger_to_write(&self) -> Result<(Ledger, Scrub), LedgerError> {
        self.ledger_to_write_waiting(lock::WAIT)
    }

    /// [`ledger_to_write`](Self::ledger_to_write), with the scrub waiting up to
    /// `wait` for the workspace rather than [`lock::WAIT`], for a writer that
    /// must not give up ([`crate::install::disown`]).
    pub(crate) fn ledger_to_write_waiting(
        &self,
        wait: Duration,
    ) -> Result<(Ledger, Scrub), LedgerError> {
        let ledger = self.ledger()?;
        // Through gix whatever a gather is handed: what is scrubbed is what
        // an older build's gix clones kept, and only gix knows those files.
        let scrub = gather::scrub(self, &ledger, wait, &crate::adapter::gix::Gix)?;
        let known = self.known();
        ledger.knew(&known.ids, &known.stale)?;
        Ok((ledger, scrub))
    }

    /// The ledger, for a command that only reads it: the one on disk when
    /// there is one, and otherwise an empty one in memory, which answers every
    /// question the way a ledger nothing was ever written to would, without
    /// making a file to say so.
    ///
    /// Never for a write. What is recorded in the stand-in is gone when it is
    /// dropped, and a copy installed without its record is one Mindweaver
    /// would afterwards refuse to touch; a write opens
    /// [`ledger_to_write`](Self::ledger_to_write).
    ///
    /// Nor is the one on disk scrubbed of what an older build stored with a
    /// credential: everything the ledger hands back is redacted on the way out
    /// anyway, and reading is no reason to write. It is left for the next
    /// command that writes.
    ///
    /// Never waits for the workspace's lock either, though an older ledger is
    /// migrated here as it is there: the migration's own transaction keeps it
    /// apart from a writer's ([`ledger`](Self::ledger)), so the most a read
    /// waits for is another process's migration, once after an upgrade.
    pub fn ledger_for_reading(&self) -> Result<Ledger, LedgerError> {
        // An unmade workspace has no ledger, whatever a stray file says.
        if !self.made {
            return Ledger::in_memory();
        }
        match Ledger::open_existing_knowing(self.ledger_path(), || self.known().ids)? {
            Some(ledger) => Ok(ledger),
            None => Ledger::in_memory(),
        }
    }

    /// Which id each project the workspace links holds, by the route the
    /// ledger names it by, and which entries open no project: what the
    /// ledger is told of them ([`Ledger::knew`](crate::port::LedgerStore::knew)).
    ///
    /// A project a `mind.toml` marks has no id, and is in neither. Nor is an
    /// id two linked directories hold: a project copied with its `.mind`,
    /// which a report says ([`crate::status`]) until one of them is given an
    /// id of its own, and which the ledger is not to take for either.
    ///
    /// An entry whose marker is there and would not open is stale here too,
    /// though it is no place a project moved from
    /// ([`MindProject::is_vacant`]): the ledger keeps the id it last saw
    /// there, rather than handing it to a linked copy of a project nothing
    /// could read.
    pub(crate) fn known(&self) -> Known {
        let mut held: Vec<(ProjectId, String, PathBuf)> = Vec::new();
        let mut stale = Vec::new();
        for member in self.members() {
            let route = crate::install::entry_for_root(self, &member.root);
            match member.project {
                Ok(project) => {
                    if let Some(settings) = project.settings() {
                        held.push((settings.id.clone(), route, member.root));
                    }
                }
                Err(_) => stale.push(route),
            }
        }
        let ids = held
            .iter()
            .filter(|(id, _, root)| {
                held.iter()
                    .all(|(other, _, at)| other != id || same_directory(at, root))
            })
            .map(|(id, route, _)| (id.clone(), route.clone()))
            .collect::<Vec<_>>();
        let mut unique: Vec<(ProjectId, String)> = Vec::new();
        for (id, route) in ids {
            if !unique.iter().any(|(seen, _)| *seen == id) {
                unique.push((id, route));
            }
        }
        Known { ids: unique, stale }
    }

    /// The entry this workspace would store for a project directory.
    ///
    /// A route relative to the workspace when one genuinely resolves, so the
    /// pair can be moved together; the absolute path otherwise.
    pub fn entry_for(&self, project_root: &Path) -> PathBuf {
        let absolute = paths::normalize(project_root);
        match paths::relative_to(&absolute, &self.root) {
            Some(route) if self.route_reaches(&route, &absolute) => route,
            _ => absolute,
        }
    }

    /// Whether following `route` from this workspace really lands on `target`.
    ///
    /// The route is arithmetic, and arithmetic on paths is only true when no
    /// component is a symlink: if the workspace root is spelled through one,
    /// a `..` climbs out of the link's target rather than out of the directory
    /// the name suggests, and the stored entry points somewhere that does not
    /// exist. `/tmp` is a symlink to `/private/tmp` on every Mac, so this is
    /// not an exotic case.
    ///
    /// The filesystem is consulted here and only here. The answer decides
    /// which spelling to store; it is never stored itself, so entries stay
    /// portable rather than being frozen to one machine's symlink layout.
    fn route_reaches(&self, route: &Path, target: &Path) -> bool {
        match (
            fs::canonicalize(self.root.join(route)),
            fs::canonicalize(target),
        ) {
            (Ok(followed), Ok(wanted)) => followed == wanted,
            // Nothing to compare against: a route that only descends is safe
            // whatever the symlinks do, and one that climbs is not worth a
            // guess.
            _ => !climbs(route),
        }
    }

    /// Register a mind project with this workspace.
    ///
    /// Idempotent, like `init`: registering a project that is already there
    /// changes nothing, says so, and reports the spelling the file actually
    /// uses rather than the one this call would have written.
    pub fn link(
        &mut self,
        project: &MindProject,
    ) -> Result<(PathBuf, Registration), WorkspaceError> {
        let entry = self.entry_for(project.root());
        let target = paths::normalize(project.root());
        let written =
            paths::to_config_string(&entry).ok_or_else(|| WorkspaceError::NonUtf8Path {
                path: entry.clone(),
            })?;
        let root = self.root.clone();

        // Matched against the array being edited, not against the copy parsed
        // when this workspace was opened: the file is meant to be editable by
        // hand, so the copy can be stale by the time we get here.
        self.edit_projects(move |array| {
            if let Some(existing) = entries(array)
                .into_iter()
                .find(|entry| points_at(&root, entry, &target))
            {
                return Ok((existing, Registration::AlreadyRegistered));
            }
            array.push(written.as_str());
            Ok((PathBuf::from(&written), Registration::Added))
        })
    }

    /// Drop every entry pointing at a project, returning what was removed.
    ///
    /// Every entry, not the first: two spellings of one directory are one
    /// project, and removing half of them while reporting success would leave
    /// it registered and the user believing otherwise.
    ///
    /// Takes a path rather than a `MindProject` on purpose: the entry worth
    /// removing most often is one whose directory has moved away, and that
    /// cannot be opened as a project any more.
    ///
    /// Unlike `link` this is not idempotent. Removing something that was never
    /// there is a typo far more often than it is a no-op, and saying so is
    /// what turns a silent success into a fixable mistake.
    pub fn unlink(&mut self, project_root: &Path) -> Result<Vec<PathBuf>, WorkspaceError> {
        self.unlink_all(&[project_root.to_path_buf()])
    }

    /// Point the workspace's entry for the directory `old` at `new` instead,
    /// where it stands in the marker, and hand back both spellings: what the
    /// entry was, and what it is now, the route [`link`](Self::link) would
    /// have written for `new`. Every other entry pointing at `old` goes, as
    /// [`unlink`](Self::unlink) takes every spelling of one directory.
    ///
    /// In place rather than an unlink and a link, so the entry keeps its
    /// place in the list and the comments around it, which a person put
    /// there about the project, not about where it was. For
    /// [`crate::relink::relink`], which holds the workspace's lock and has
    /// said `new` is `old`'s project, moved. Refused when no entry points at
    /// `old`, matched against the array being edited, as `link` matches.
    pub(crate) fn relink_held(
        &mut self,
        old: &Path,
        new: &Path,
    ) -> Result<(PathBuf, PathBuf), WorkspaceError> {
        let entry = self.entry_for(new);
        let written =
            paths::to_config_string(&entry).ok_or_else(|| WorkspaceError::NonUtf8Path {
                path: entry.clone(),
            })?;
        let target = paths::normalize(old);
        let moved = paths::normalize(new);
        let root = self.root.clone();
        let edited = self.edit_marker_held(|document| {
            let Some(array) = document
                .get_mut(PROJECTS_KEY)
                .and_then(toml_edit::Item::as_array_mut)
            else {
                return Ok(None);
            };
            // Looked at again here, under the lock: a link made since the
            // caller looked would leave two entries for one project.
            if let Some(linked) = entries(array)
                .into_iter()
                .find(|entry| points_at(&root, entry, &moved))
            {
                return Err(format!(
                    "{} is linked already, as {}",
                    new.display(),
                    linked.display()
                ));
            }
            let Some(index) = array.iter().position(|value| {
                value
                    .as_str()
                    .is_some_and(|entry| points_at(&root, Path::new(entry), &target))
            }) else {
                return Ok(None);
            };
            let was = array
                .get(index)
                .and_then(|value| value.as_str())
                .map(PathBuf::from);
            array.replace(index, written.as_str());
            remove_pointing_at(array, &root, &target);
            Ok(was)
        })?;
        match edited {
            Some(was) => Ok((was, PathBuf::from(&written))),
            None => Err(WorkspaceError::NotRegistered {
                path: old.to_path_buf(),
                workspace: self.root.clone(),
            }),
        }
    }

    /// [`unlink`](Self::unlink) several at once, in one rewrite of the file:
    /// all of them or, if any is not registered, none. Two spellings of one
    /// directory are one project, not a second one to fail on.
    pub fn unlink_all(&mut self, roots: &[PathBuf]) -> Result<Vec<PathBuf>, WorkspaceError> {
        let workspace = self.root.clone();
        self.drop_entries(PROJECTS_KEY, roots)?
            .map_err(|path| WorkspaceError::NotRegistered { path, workspace })
    }

    /// Remove every entry of the array `key` pointing at any of `roots`, in
    /// one edit. When one of them has no entry, remove nothing and return
    /// that one.
    fn drop_entries(
        &mut self,
        key: &str,
        roots: &[PathBuf],
    ) -> Result<Result<Vec<PathBuf>, PathBuf>, WorkspaceError> {
        let targets: Vec<(PathBuf, PathBuf)> = roots
            .iter()
            .map(|root| (root.clone(), paths::normalize(root)))
            .collect();
        let root = self.root.clone();
        self.edit_array(key, move |array| {
            let stored = entries(array);
            if let Some((missing, _)) = targets
                .iter()
                .find(|(_, target)| !stored.iter().any(|entry| points_at(&root, entry, target)))
            {
                return Ok(Err(missing.clone()));
            }
            let mut removed = Vec::new();
            for (_, target) in &targets {
                removed.extend(remove_pointing_at(array, &root, target));
            }
            Ok(Ok(removed))
        })
    }

    /// Offer another workspace's own artifacts to this one's projects.
    ///
    /// Recorded, not copied: the entry is where to read them from, so an edit
    /// made there is what installs here next, without loading it again.
    /// Idempotent, like `link`, and a workspace cannot load itself.
    pub fn load(
        &mut self,
        source: &WeaverWorkspace,
    ) -> Result<(PathBuf, Registration), WorkspaceError> {
        let mut loaded = self.load_all(std::slice::from_ref(source))?;
        Ok(loaded.remove(0))
    }

    /// [`load`](Self::load) several at once, in one rewrite of the file: all
    /// of them or, if one is this workspace, none. A workspace named with
    /// `-W` is not passed over for being among what it is told to load, so
    /// the refusal has to come before the first entry is written.
    pub fn load_all(
        &mut self,
        sources: &[WeaverWorkspace],
    ) -> Result<Vec<(PathBuf, Registration)>, WorkspaceError> {
        let mut wanted = Vec::new();
        for source in sources {
            if same_directory(source.root(), &self.root) {
                return Err(WorkspaceError::LoadsItself {
                    path: self.root.clone(),
                });
            }
            let entry = self.entry_for(source.root());
            let written =
                paths::to_config_string(&entry).ok_or_else(|| WorkspaceError::NonUtf8Path {
                    path: entry.clone(),
                })?;
            wanted.push((written, paths::normalize(source.root())));
        }
        let root = self.root.clone();

        self.edit_array(LOADED_KEY, move |array| {
            Ok(wanted
                .iter()
                .map(|(written, target)| {
                    // Against the array as this edit leaves it, so two
                    // spellings of one source in one call are one load.
                    if let Some(existing) = entries(array)
                        .into_iter()
                        .find(|entry| points_at(&root, entry, target))
                    {
                        return (existing, Registration::AlreadyRegistered);
                    }
                    array.push(written.as_str());
                    (PathBuf::from(written), Registration::Added)
                })
                .collect())
        })
    }

    /// Stop offering a loaded workspace's artifacts, returning every entry
    /// that pointed at it. A path rather than a workspace, as for `unlink`: a
    /// source that has moved away is the one most worth dropping.
    pub fn unload(&mut self, source_root: &Path) -> Result<Vec<PathBuf>, WorkspaceError> {
        self.unload_all(&[source_root.to_path_buf()])
    }

    /// [`unload`](Self::unload) several at once, all or none, in one rewrite.
    pub fn unload_all(&mut self, roots: &[PathBuf]) -> Result<Vec<PathBuf>, WorkspaceError> {
        let workspace = self.root.clone();
        self.drop_entries(LOADED_KEY, roots)?
            .map_err(|path| WorkspaceError::NotLoaded { path, workspace })
    }

    /// Whether some `loaded` entry points at the directory `root`.
    pub fn is_loaded(&self, root: &Path) -> bool {
        let target = paths::normalize(root);
        self.config
            .loaded
            .iter()
            .any(|entry| points_at(&self.root, entry, &target))
    }

    /// Every loaded entry, in marker order, with the workspace it opens or the
    /// reason it does not: the same shape as [`members`](Self::members), so
    /// a source that has gone away is shown where it can be unloaded.
    ///
    /// Only one level deep: what a loaded workspace loads itself is not
    /// followed, so two workspaces loading each other is not a loop.
    pub fn loads(&self) -> Vec<Loaded> {
        self.opened(&self.config.loaded)
    }

    /// Every `known` entry, in marker order, with the workspace it opens or
    /// the reason it does not, as [`loads`](Self::loads) has them: what
    /// [`crate::known::known`] lists, a workspace that has gone included.
    pub(crate) fn remembered(&self) -> Vec<Loaded> {
        self.opened(&self.config.known)
    }

    /// Each of `entries`, resolved against the root, with the workspace there.
    fn opened(&self, entries: &[PathBuf]) -> Vec<Loaded> {
        entries
            .iter()
            .map(|entry| {
                let root = paths::normalize(&self.root.join(entry));
                let workspace = WeaverWorkspace::open(&root);
                Loaded {
                    entry: entry.clone(),
                    root,
                    workspace,
                }
            })
            .collect()
    }

    /// Remember another workspace in this one's `known`, stored the way
    /// `link` stores a project and matched the same way, so remembering one
    /// already known by another spelling changes nothing and reports the
    /// spelling the file uses.
    ///
    /// Crate-private: which workspace remembers, the default one only once
    /// it is made, and never itself, is [`crate::known`]'s to decide, so no
    /// front end writes the list into some other workspace.
    pub(crate) fn remember(
        &mut self,
        workspace: &WeaverWorkspace,
    ) -> Result<(PathBuf, Registration), WorkspaceError> {
        let entry = self.entry_for(workspace.root());
        let target = paths::normalize(workspace.root());
        let written =
            paths::to_config_string(&entry).ok_or_else(|| WorkspaceError::NonUtf8Path {
                path: entry.clone(),
            })?;
        let root = self.root.clone();
        self.edit_array(KNOWN_KEY, move |array| {
            if let Some(existing) = entries(array)
                .into_iter()
                .find(|entry| points_at(&root, entry, &target))
            {
                return Ok((existing, Registration::AlreadyRegistered));
            }
            array.push(written.as_str());
            Ok((PathBuf::from(&written), Registration::Added))
        })
    }

    /// Drop every `known` entry pointing at `root`, returning what was
    /// removed: every spelling, as `unlink` drops every one of a project's,
    /// and a path rather than a workspace, since the one most worth
    /// forgetting is one that has gone. Refused when none points at it.
    pub(crate) fn forget(&mut self, root: &Path) -> Result<Vec<PathBuf>, WorkspaceError> {
        let workspace = self.root.clone();
        self.drop_entries(KNOWN_KEY, &[root.to_path_buf()])?
            .map_err(|path| WorkspaceError::NotKnown { path, workspace })
    }

    /// Rewrite the `projects` array in place. See [`edit_array`](Self::edit_array).
    fn edit_projects<F, T>(&mut self, edit: F) -> Result<T, WorkspaceError>
    where
        F: FnOnce(&mut toml_edit::Array) -> Result<T, String>,
    {
        self.edit_array(PROJECTS_KEY, edit)
    }

    /// Rewrite one array of the marker in place, leaving every other byte of
    /// the file alone: comments, key order, spacing.
    ///
    /// This is why `toml_edit` is a dependency. Reading is serde's job, but
    /// re-serializing to write would throw away the comment that explains what
    /// the file is, which is the first documentation anyone opening it reads.
    ///
    /// The array is read from disk here rather than taken from `self.config`,
    /// so an edit decides what to do from the file it is about to write, not
    /// from a copy that may be minutes old.
    fn edit_array<F, T>(&mut self, key: &str, edit: F) -> Result<T, WorkspaceError>
    where
        F: FnOnce(&mut toml_edit::Array) -> Result<T, String>,
    {
        // Not made, so there is no marker to edit, and writing one here would
        // make the workspace behind the back of whatever asked only to read
        // it. Asked before the lock, whose file would be the first thing made.
        self.refuse_unmade()?;
        // Held from before the file is read until what is written has been
        // read back. Reading from disk is not enough on its own: two writers
        // at once would both read the file before either wrote, and the
        // second rename would put back a marker without the first one's entry.
        let _lock = self.lock()?;
        self.edit_marker_held(|document| {
            // A key deleted by hand, or a marker from before `loaded`
            // existed, is made again rather than refused.
            let made = !document.as_table().contains_key(key);
            if made {
                document[key] = toml_edit::value(toml_edit::Array::new());
                if let (Some(comment), Some(mut made)) = (explained(key), document.key_mut(key)) {
                    made.leaf_decor_mut().set_prefix(comment);
                }
            }
            let array = document[key]
                .as_array_mut()
                .ok_or_else(|| format!("`{key}` is not an array"))?;
            let value = edit(array)?;
            // Made for an edit that put nothing in it, such as a `forget`
            // of a workspace nothing remembers: taken out again, so an edit
            // that changes nothing leaves the file as it found it.
            if made
                && document[key]
                    .as_array()
                    .is_some_and(toml_edit::Array::is_empty)
            {
                document.remove(key);
            }
            Ok(value)
        })
    }

    /// Rewrite the marker in place, as [`edit_array`](Self::edit_array) does,
    /// for a writer that already holds the workspace's [`lock`](Self::lock):
    /// one whose change is more than the marker, and has to keep a second
    /// writer out from before its first step until the marker is written.
    ///
    /// The caller's to hold rather than taken here, because the lock is not
    /// re-entrant: a writer asking for it twice waits for itself.
    pub(crate) fn edit_marker_held<F, T>(&mut self, edit: F) -> Result<T, WorkspaceError>
    where
        F: FnOnce(&mut toml_edit::DocumentMut) -> Result<T, String>,
    {
        self.refuse_unmade()?;
        let path = self.config_path();
        let refused = |detail: String| WorkspaceError::Edit {
            path: path.clone(),
            detail,
        };
        // An already-registered link and a failed unlink both leave the file
        // untouched, down to its modification time.
        let value = rewrite(&path, |text| {
            let mut document: toml_edit::DocumentMut = text
                .parse()
                .map_err(|error: toml_edit::TomlError| refused(error.to_string()))?;
            let value = edit(&mut document).map_err(refused)?;
            Ok((document.to_string(), value))
        })?;

        // Re-read, so what is in memory is what is on disk rather than what we
        // believe we wrote.
        let config: WeaverConfig = read_config(&path)?;
        check_version(&path, Marker::Workspace, config.version)?;
        self.config = config;
        Ok(value)
    }

    /// Refuse an [`unmade`](Self::unmade) workspace's marker to a writer, as
    /// any directory that is not a workspace is refused.
    fn refuse_unmade(&self) -> Result<(), WorkspaceError> {
        if self.made {
            Ok(())
        } else {
            Err(WorkspaceError::NotAWorkspace {
                path: self.root.clone(),
            })
        }
    }

    /// Open every registered mind project.
    ///
    /// A registered path that has gone missing is reported rather than raised:
    /// one stale entry must not stop a workspace from managing the rest.
    pub fn projects(&self) -> (Vec<MindProject>, Vec<WorkspaceError>) {
        let mut projects = Vec::new();
        let mut failures = Vec::new();
        for member in self.members() {
            match member.project {
                Ok(project) => projects.push(project),
                Err(error) => failures.push(error),
            }
        }
        (projects, failures)
    }

    /// Every registered entry, in the order the marker lists them, with the
    /// project each one opens or the reason it does not.
    ///
    /// What [`projects`](Self::projects) is built on, for a front end that
    /// shows a stale entry beside the ones that work: the entry is what
    /// `unlink` is given to remove it, and an error alone does not say which
    /// entry it came from.
    pub fn members(&self) -> Vec<Member> {
        self.config
            .projects
            .iter()
            .map(|entry| {
                // Relative to the workspace root, so a workspace can be moved
                // with its projects and keep working.
                let root = paths::normalize(&self.root.join(entry));
                let project = MindProject::open(&root);
                Member {
                    entry: entry.clone(),
                    root,
                    project,
                }
            })
            .collect()
    }

    /// The workspace's own artifacts, held the way a project holds its own: in
    /// the directories its marker names.
    ///
    /// A `MindProject` rather than a type of its own, because everything that
    /// reads, creates, renames or removes an artifact asks its holder the same
    /// three things (where it is, what it is called, where each kind lives),
    /// and a workspace answers them exactly as a project does. So the rules
    /// for the workspace's artifacts are the rules for everybody's, with no
    /// second copy of them to drift.
    ///
    /// Every kind is named in it, the ones the marker leaves out in the
    /// library ([`Directories::library_for`]): asked about a kind it was not
    /// told of, a `MindProject` answers with a folder beside the code, which
    /// for a workspace is beside other people's repositories, or in a home
    /// directory.
    pub fn own(&self) -> MindProject {
        MindProject {
            root: self.root.clone(),
            leftovers: Vec::new(),
            config: ProjectConfig::Toml(MindConfig {
                // The project marker's version rather than the workspace's:
                // `directory_for` holds it against `DIRECTORIES_VERSION`, a
                // line drawn in the project format, and below it sends every
                // kind its marker does not name into `.mind`, where a
                // workspace has never kept its artifacts and a v1
                // marker would send them. The newest a `mind.toml`
                // holds, since what this is read as is one: every kind is
                // named below, so no version gives one a different folder.
                version: MIND_TOML_VERSION,
                name: self.config.name.clone(),
                directories: self
                    .config
                    .directories
                    .clone()
                    .filled(|kind| Some(Directories::library_under(self.names, kind))),
            }),
        }
    }

    /// The kinds the workspace keeps artifacts of its own for.
    ///
    /// Every kind, except where the workspace's root is itself a project it
    /// manages and both keep that kind in the same folder: what sits there is
    /// the project's, and counting it twice would list every one of them
    /// twice.
    pub fn own_kinds(&self) -> Vec<Kind> {
        let own = self.own();
        let here = self
            .members()
            .into_iter()
            .filter_map(|member| member.project.ok())
            .find(|project| project.root() == self.root);
        Kind::ALL
            .into_iter()
            .filter(|kind| {
                here.as_ref()
                    .is_none_or(|project| project.directory_for(*kind) != own.directory_for(*kind))
            })
            .collect()
    }

    /// Whether some registered entry points at the directory `root`.
    ///
    /// By where entries point, not how they are spelled (the same matching
    /// `link` uses), so a project registered as `./collapse` is linked when
    /// asked about as `collapse`.
    pub fn is_linked(&self, root: &Path) -> bool {
        self.linked_as(root).is_some()
    }

    /// The first registered entry that points at the directory `root`, as
    /// the marker spells it, matched as [`is_linked`](Self::is_linked)
    /// matches: what a report names the project by, and what `-p` and
    /// `unlink` are given.
    pub(crate) fn linked_as(&self, root: &Path) -> Option<&Path> {
        let target = paths::normalize(root);
        self.config
            .projects
            .iter()
            .find(|entry| points_at(&self.root, entry, &target))
            .map(PathBuf::as_path)
    }

    /// The directory `root` names, spelled the way this workspace's entry
    /// for it leads there; `root` itself when no entry points at it.
    ///
    /// For the ledger, which names a project by its route from the
    /// workspace, so one directory spelled two ways would be two projects to
    /// it. A `mind` command spells its project the way the operating system
    /// reports the directory it runs in, through no symlink, while a
    /// workspace named with `-W` keeps the spelling it was given, `/tmp` on
    /// a Mac say; the route between those two is not the one `mindweaver -p`
    /// takes from the entry, and a record written under one would not be
    /// found under the other. A spelling that already matches as written is
    /// kept, so nothing that matched before is named differently now.
    pub(crate) fn spelling_of(&self, root: &Path) -> PathBuf {
        let target = paths::normalize(root);
        let led: Vec<PathBuf> = self
            .config
            .projects
            .iter()
            .map(|entry| paths::normalize(&self.root.join(entry)))
            .collect();
        if led.contains(&target) {
            return target;
        }
        led.into_iter()
            .find(|candidate| same_directory(candidate, &target))
            .unwrap_or(target)
    }
}

/// What the projects a workspace links say of their ids
/// ([`WeaverWorkspace::known`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Known {
    /// Each id one linked directory holds, by its route from the workspace,
    /// in the order the marker lists them.
    pub ids: Vec<(ProjectId, String)>,
    /// The route of each entry that opens no project, whether nothing is
    /// there any more or its marker would not open.
    pub stale: Vec<String>,
}

/// One entry in a workspace's registry, and what it opens.
#[derive(Debug)]
pub struct Member {
    /// The entry as the marker spells it.
    pub entry: PathBuf,
    /// Where it points, resolved against the workspace root.
    pub root: PathBuf,
    /// The project there, or why there is none.
    pub project: Result<MindProject, WorkspaceError>,
}

/// As the entry and what it opens: the entry as the marker spells it, where
/// it points, then the project's name, marker, id, agents and share mode,
/// none of them for an entry that opens no project, and why it opens none
/// as `error`, by its code and message.
///
/// By hand: a project holds the whole of its marker, and the error what JSON
/// has no shape for, so a derive could not write either. Spread over fields
/// of the member's own, rather than nested, so it reads as a status report's
/// project does, whose `name`, `marker` and `agents` say the same thing by
/// the same names; `error` says what that report's `stale` does, with the
/// code beside the message.
impl Serialize for Member {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let project = self.project.as_ref().ok();
        let settings = project.and_then(MindProject::settings);
        let error = self.project.as_ref().err();
        let mut member = serializer.serialize_struct("Member", 8)?;
        member.serialize_field("entry", &paths::as_route(&self.entry))?;
        member.serialize_field("root", &self.root.to_string_lossy())?;
        member.serialize_field("name", &project.map(MindProject::name))?;
        member.serialize_field("marker", &project.map(MindProject::marker_file))?;
        member.serialize_field("id", &settings.map(|settings| &settings.id))?;
        member.serialize_field(
            "agents",
            settings.map_or(&[][..], |settings| &settings.agents),
        )?;
        member.serialize_field("share", &settings.map(|settings| settings.share))?;
        member.serialize_field("error", &error.map(|error| Said::of(error.code(), error)))?;
        member.end()
    }
}

/// One entry in a workspace's `loaded` list, or in the default workspace's
/// `known`, and what it opens.
#[derive(Debug)]
pub struct Loaded {
    /// The entry as the marker spells it.
    pub entry: PathBuf,
    /// Where it points, resolved against the workspace root.
    pub root: PathBuf,
    /// The workspace there, or why there is none.
    pub workspace: Result<WeaverWorkspace, WorkspaceError>,
}

/// As the entry and what it opens, for the reason a [`Member`] is: the entry
/// as the marker spells it, where it points, the workspace's name, none for
/// an entry that opens none, and why it opens none as `error`.
impl Serialize for Loaded {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let workspace = self.workspace.as_ref().ok();
        let error = self.workspace.as_ref().err();
        let mut loaded = serializer.serialize_struct("Loaded", 4)?;
        loaded.serialize_field("entry", &paths::as_route(&self.entry))?;
        loaded.serialize_field("root", &self.root.to_string_lossy())?;
        loaded.serialize_field("name", &workspace.map(WeaverWorkspace::name))?;
        loaded.serialize_field("error", &error.map(|error| Said::of(error.code(), error)))?;
        loaded.end()
    }
}

const PROJECTS_KEY: &str = "projects";
const LOADED_KEY: &str = "loaded";
const KNOWN_KEY: &str = "known";

/// The comment a key an edit makes is given in the marker: `known`'s, the
/// one key the template leaves out, since only the default workspace has a
/// use for it and every other marker would carry a line about nothing.
fn explained(key: &str) -> Option<&'static str> {
    (key == KNOWN_KEY).then_some(
        "\n\
         # `known` lists the other weaver workspaces on this machine, which\n\
         # `mindweaver workspaces` shows. While this is the default workspace,\n\
         # `mindweaver init` adds each one it makes; `mindweaver workspaces forget <path>`\n\
         # takes one out.\n",
    )
}

/// Remove every entry of `array` pointing at `target`, returning them.
fn remove_pointing_at(array: &mut toml_edit::Array, root: &Path, target: &Path) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    let mut index = 0;
    while index < array.len() {
        match array.get(index).and_then(|value| value.as_str()) {
            Some(entry) if points_at(root, Path::new(entry), target) => {
                removed.push(PathBuf::from(entry));
                let gone = array.remove(index);
                // The entry after it moves into its place. A prefix toml_edit
                // has not been given is the one it would write there: none
                // before the first entry, a space before any other.
                if let Some(next) = array.get_mut(index) {
                    let place = layout(&gone, if index == 0 { "" } else { " " });
                    let moved = moved_in(place, layout(next, " "));
                    next.decor_mut().set_prefix(moved);
                }
            }
            _ => index += 1,
        }
    }
    removed
}

/// What comes before `value` in its array: the space and comments since
/// the bracket or comma ahead of it, or `default` when it has none of its own.
fn layout<'v>(value: &'v toml_edit::Value, default: &'v str) -> &'v str {
    value
        .decor()
        .prefix()
        .and_then(|raw| raw.as_str())
        .unwrap_or(default)
}

/// The layout an entry laid out as `own` is left with when the one before
/// it, laid out as `place`, is dropped and it moves into that place.
///
/// Each layout starts on the line of whatever comes before its entry: up to
/// its first line break is the rest of that line, and a comment there is
/// that line's; after the break come the comments above the entry and its
/// indent. So the entry keeps the rest of the line above the dropped one and
/// its own comments, and loses the dropped entry's: the ones above it, and
/// one after it on its line (the start of `own`). Taking `place` whole would
/// put the dropped entry's comment above this one in place of its own, and
/// keeping `own` whole would leave `[ "b"]` from `["a", "b"]` and lose a
/// comment at the end of the line above.
fn moved_in(place: &str, own: &str) -> String {
    let (above, dropped) = first_line(place);
    let (line, kept) = first_line(own);
    if line.ends_with('\n') {
        // The entry had a line of its own, and keeps one. A plain break
        // even where the file has `\r\n`: toml_edit writes layout without
        // its `\r`s.
        let break_ = if above.ends_with('\n') { "" } else { "\n" };
        format!("{above}{break_}{kept}")
    } else {
        // It shared the dropped entry's line, so it takes that one's indent.
        let indent = dropped.rsplit_once('\n').map_or(dropped, |(_, last)| last);
        format!("{above}{indent}")
    }
}

/// `raw` split after its first line break, or all of it and nothing when
/// it has none.
fn first_line(raw: &str) -> (&str, &str) {
    match raw.find('\n') {
        Some(at) => raw.split_at(at + 1),
        None => (raw, ""),
    }
}

/// Whether two spellings name one directory: lexically first, then through
/// the filesystem, so a symlinked spelling of a root is still that root.
pub(crate) fn same_directory(a: &Path, b: &Path) -> bool {
    let (a, b) = (paths::normalize(a), paths::normalize(b));
    a == b
        || matches!(
            (fs::canonicalize(&a), fs::canonicalize(&b)),
            (Ok(a), Ok(b)) if a == b
        )
}

// ---------------------------------------------------------------------------
// Shared plumbing
// ---------------------------------------------------------------------------

/// Why a project or a workspace could not be created, found or read.
#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("{path}: cannot be created: {source}")]
    Create {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: cannot be read: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: not valid Mindweaver configuration: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    /// A marker that is not what its format says, for a reason no parser
    /// gives: a project's settings, which are read a key at a time, or a
    /// version the file it is in does not hold.
    #[error("{path}: not valid Mindweaver configuration: {detail}")]
    Malformed { path: PathBuf, detail: String },
    #[error(
        "{path}: not a mind project (no {MIND_DIR}/{SETTINGS_FILE}, {MIND_DIR}/{SETTINGS_JSON_FILE} \
         or {MIND_DIR}/{MIND_CONFIG})"
    )]
    NotAProject { path: PathBuf },
    #[error("{path}: not a weaver workspace (no {WEAVER_DIR}/{WEAVER_CONFIG} or {FLAYER_DIR}/{FLAYER_CONFIG})")]
    NotAWorkspace { path: PathBuf },
    #[error("{path}: written by a newer Mindweaver ({marker} version {found}, this one reads {supported})", supported = .marker.version())]
    Version {
        path: PathBuf,
        marker: Marker,
        found: u32,
    },
    #[error("{path}: cannot be edited: {detail}")]
    Edit { path: PathBuf, detail: String },
    #[error("{path}: not registered in the workspace at {workspace}")]
    NotRegistered { path: PathBuf, workspace: PathBuf },
    #[error("{path}: a workspace cannot load itself")]
    LoadsItself { path: PathBuf },
    #[error("{path}: not loaded by the workspace at {workspace}")]
    NotLoaded { path: PathBuf, workspace: PathBuf },
    #[error("{path}: not known to the default workspace at {workspace}")]
    NotKnown { path: PathBuf, workspace: PathBuf },
    #[error("{path}: not valid UTF-8, so it cannot be written into a marker")]
    NonUtf8Path { path: PathBuf },
    #[error("{path}: cannot hold a project's {kinds}, because it is not inside the project", kinds = .kind.plural())]
    OutsideProject { path: PathBuf, kind: Kind },
    #[error(
        "{path}: cannot hold a project's {kinds}, \
         because it is inside {MIND_DIR}, which holds Mindweaver's own files \
         and is kept out of git unless the project is shared",
        kinds = .kind.plural()
    )]
    InsideMind { path: PathBuf, kind: Kind },
    /// Saying how the project comes to keep one, by the file it is marked
    /// by: only a `settings.toml` is changed, so a project marked by anything
    /// older is sent to the migration first, as [`NoSettings`] sends it.
    ///
    /// [`NoSettings`]: WorkspaceError::NoSettings
    #[error(
        "{path}: keeps no {} of its own, since its {marker} names no folder for them; {}",
        .kind.plural(),
        named_by(*.marker)
    )]
    NoDirectory {
        path: PathBuf,
        kind: Kind,
        marker: MarkerFile,
    },
    /// A folder a change to a project's settings would send a kind's
    /// installs to that no install would use ([`Misplaced`]).
    #[error(
        "{path}: cannot be where a project's {kinds} are installed, because {because}",
        kinds = .kind.plural(),
        because = .why.because(*.kind)
    )]
    Target {
        path: PathBuf,
        kind: Kind,
        why: Misplaced,
    },
    /// Pointing at the migration rather than at `mind init`, which never
    /// rewrites a marker: what such a project lacks is what moving it to a
    /// `settings.toml` gives it, as it was committed by design or written
    /// before.
    #[error(
        "{path}: has no {SETTINGS_FILE} to change, since a {marker} marks it; \
         `mind migrate` moves it to one"
    )]
    NoSettings { path: PathBuf, marker: MarkerFile },
    #[error("{path}: cannot hold the workspace's {kinds}, because it is not inside the workspace", kinds = .kind.plural())]
    OutsideWorkspace { path: PathBuf, kind: Kind },
    #[error(
        "{path}: cannot hold the workspace's {kinds}, \
         because it overlaps {gathered}, which is reserved for gathering",
        kinds = .kind.plural()
    )]
    OverlapsGathered {
        path: PathBuf,
        kind: Kind,
        gathered: PathBuf,
    },
    #[error(transparent)]
    Lock(#[from] LockError),
}

codes!(WorkspaceError {
    Self::Create { .. } => "create_failed",
    Self::Read { .. } => "read_failed",
    Self::Parse { .. } => "marker_unparsed",
    Self::Malformed { .. } => "marker_malformed",
    Self::NotAProject { .. } => "not_a_project",
    Self::NotAWorkspace { .. } => "not_a_workspace",
    Self::Version { .. } => "marker_too_new",
    Self::Edit { .. } => "marker_edit_failed",
    Self::NotRegistered { .. } => "not_registered",
    Self::LoadsItself { .. } => "loads_itself",
    Self::NotLoaded { .. } => "not_loaded",
    Self::NotKnown { .. } => "not_known",
    Self::NonUtf8Path { .. } => "path_not_utf8",
    Self::OutsideProject { .. } => "folder_outside_project",
    Self::InsideMind { .. } => "folder_inside_mind",
    Self::NoDirectory { .. } => "no_folder",
    Self::Target { .. } => "target_unusable",
    Self::NoSettings { .. } => "no_settings",
    Self::OutsideWorkspace { .. } => "folder_outside_workspace",
    Self::OverlapsGathered { .. } => "folder_overlaps_gathered",
} carried {
    Self::Lock(error) => error,
});

impl WorkspaceError {
    /// The path the failure is about.
    pub fn path(&self) -> &Path {
        match self {
            WorkspaceError::Create { path, .. }
            | WorkspaceError::Read { path, .. }
            | WorkspaceError::Parse { path, .. }
            | WorkspaceError::Malformed { path, .. }
            | WorkspaceError::NotAProject { path }
            | WorkspaceError::NotAWorkspace { path }
            | WorkspaceError::Version { path, .. }
            | WorkspaceError::Edit { path, .. }
            | WorkspaceError::NotRegistered { path, .. }
            | WorkspaceError::LoadsItself { path }
            | WorkspaceError::NotLoaded { path, .. }
            | WorkspaceError::NotKnown { path, .. }
            | WorkspaceError::NonUtf8Path { path }
            | WorkspaceError::OutsideProject { path, .. }
            | WorkspaceError::InsideMind { path, .. }
            | WorkspaceError::NoDirectory { path, .. }
            | WorkspaceError::Target { path, .. }
            | WorkspaceError::NoSettings { path, .. }
            | WorkspaceError::OutsideWorkspace { path, .. }
            | WorkspaceError::OverlapsGathered { path, .. } => path,
            WorkspaceError::Lock(error) => error.path(),
        }
    }
}

/// How a project marked by `marker` comes to keep a kind it names no folder
/// for, said after [`WorkspaceError::NoDirectory`]: in its `[directories]`,
/// once it is marked by a `settings.toml`, which `mind migrate` moves the
/// older markers to.
fn named_by(marker: MarkerFile) -> String {
    match marker {
        MarkerFile::MindToml | MarkerFile::SettingsJson => format!(
            "`mind migrate` moves it to a {SETTINGS_FILE}, whose `[directories]` can name one"
        ),
        MarkerFile::SettingsToml => String::from("its `[directories]` can name one"),
    }
}

/// Walk `start` and its ancestors looking for one `marked` says carries a
/// marker.
fn locate_marker(
    start: &Path,
    marked: impl Fn(&Path) -> bool,
) -> Result<Option<PathBuf>, WorkspaceError> {
    // Absolute first: `ancestors()` on a relative path stops at the empty
    // component, so `mind list` run from a subdirectory would find nothing.
    let start = absolute(start)?;
    Ok(start
        .ancestors()
        .find(|candidate| marked(candidate))
        .map(Path::to_path_buf))
}

/// Read and parse a marker file.
fn read_config<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, WorkspaceError> {
    let text = fs::read_to_string(path).map_err(|source| WorkspaceError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| WorkspaceError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

/// Refuse a marker written by a version that knows more than this one.
pub(crate) fn check_version(path: &Path, marker: Marker, found: u32) -> Result<(), WorkspaceError> {
    if found > marker.version() {
        return Err(WorkspaceError::Version {
            path: path.to_path_buf(),
            marker,
            found,
        });
    }
    Ok(())
}

fn create_dir(path: &Path) -> Result<(), WorkspaceError> {
    fs::create_dir_all(path).map_err(|source| WorkspaceError::Create {
        path: path.to_path_buf(),
        source,
    })
}

/// Give the `.mind` at `dir` the `.gitignore` that keeps it out of git,
/// unless it has one. `create_new`, so one somebody changed is left as they
/// left it.
pub(crate) fn ignore_itself(dir: &Path) -> Result<(), WorkspaceError> {
    match write_new(&dir.join(".gitignore"), MIND_IGNORE) {
        Err(WorkspaceError::Create { source, .. })
            if source.kind() == io::ErrorKind::AlreadyExists =>
        {
            Ok(())
        }
        written => written,
    }
}

/// Write a file that must not already exist.
///
/// `create_new` rather than a prior `exists()` check: the check and the write
/// are one operation, so nothing that appears in between is overwritten.
pub(crate) fn write_new(path: &Path, contents: &str) -> Result<(), WorkspaceError> {
    use std::io::Write as _;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| WorkspaceError::Create {
            path: path.to_path_buf(),
            source,
        })?;
    file.write_all(contents.as_bytes())
        .map_err(|source| WorkspaceError::Create {
            path: path.to_path_buf(),
            source,
        })
}

/// The entries currently written in a `projects` array.
fn entries(array: &toml_edit::Array) -> Vec<PathBuf> {
    array
        .iter()
        .filter_map(|value| value.as_str())
        .map(PathBuf::from)
        .collect()
}

/// Whether a stored entry names the same directory as `target`.
///
/// By where they point, not how they are spelled, so `collapse` and
/// `./collapse` are one entry. Arithmetic settles most of it; two spellings
/// that only the filesystem can equate (a symlinked parent, `/tmp` on a Mac)
/// need canonicalising, and a path that does not exist cannot be equated that
/// way at all.
fn points_at(root: &Path, entry: &Path, target: &Path) -> bool {
    let resolved = paths::normalize(&root.join(entry));
    if resolved == target {
        return true;
    }
    match (fs::canonicalize(&resolved), fs::canonicalize(target)) {
        (Ok(followed), Ok(wanted)) => followed == wanted,
        _ => false,
    }
}

/// Whether a route has to climb out of its base to get where it is going.
fn climbs(route: &Path) -> bool {
    route
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
}

/// Change the marker at `path` in place: read it, hand its text to `edit`,
/// which gives back the text it is to hold, and write that only when it
/// differs, through [`replace_file`].
///
/// One way for both markers, a workspace's and a project's settings, so
/// both are changed alike: read here rather than taken from what was read
/// when their holder was opened, so an edit decides what to do from the
/// file it is about to write and not from a copy that may be minutes old;
/// and left alone, its modification time included, when nothing changed.
/// Parsing is the caller's, since what a marker that does not parse is
/// refused as is its own to say.
///
/// Takes no lock: whoever calls it holds the one that keeps the change apart.
pub(crate) fn rewrite<T>(
    path: &Path,
    edit: impl FnOnce(&str) -> Result<(String, T), WorkspaceError>,
) -> Result<T, WorkspaceError> {
    let text = fs::read_to_string(path).map_err(|source| WorkspaceError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let (rewritten, value) = edit(&text)?;
    if rewritten != text {
        replace_file(path, &rewritten)?;
    }
    Ok(value)
}

/// Replace a file's contents through a temporary file and a rename.
///
/// The rename is the point: a crash mid-write leaves either the old config or
/// the new one, never half of each. The temporary sits beside the original so
/// the rename stays inside one filesystem, which is where it is atomic.
pub(crate) fn replace_file(path: &Path, contents: &str) -> Result<(), WorkspaceError> {
    replace_bytes(path, contents.as_bytes())
        .map_err(|(path, source)| WorkspaceError::Create { path, source })
}

/// [`replace_file`] for bytes, failing with the path the error is about (the
/// temporary or the file) rather than a `WorkspaceError`.
///
/// Split out for files that are not markers and need not be UTF-8: git's
/// local exclude file is bytes, and its failures belong to its own module's
/// error, not to a workspace's.
pub(crate) fn replace_bytes(path: &Path, contents: &[u8]) -> Result<(), (PathBuf, io::Error)> {
    // Follow a symlinked config to the file it names. Replacing the link
    // itself would silently detach a workspace from a config someone chose to
    // share, and leave the original holding stale contents.
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    replace_bytes_unresolved(&resolved, contents)
}

/// [`replace_bytes`] at `path` as it is spelled, never resolved: the
/// temporary is made beside it and renamed onto it.
///
/// For a file whose route its caller refuses to follow a link on
/// (`config_entry::ConfigFile`), where resolving one would carry the write
/// out of the folder it was meant for.
pub(crate) fn replace_bytes_unresolved(
    path: &Path,
    contents: &[u8],
) -> Result<(), (PathBuf, io::Error)> {
    use std::io::Write as _;

    let (temporary, mut file) = temporary_beside(path)?;
    let written = file.write_all(contents);
    drop(file);
    // A temporary left behind by a write or a rename that failed is litter
    // nothing owns, so every way out after it is made takes it away.
    if let Err(source) = written {
        let _ = fs::remove_file(&temporary);
        return Err((temporary, source));
    }

    // A rename carries the temporary file's permissions with it, so the
    // original's are copied across first: a config chmodded to 600 must not
    // come back world readable. Best effort: a filesystem that cannot say is
    // not a reason to refuse the edit.
    if let Ok(metadata) = fs::metadata(path) {
        let _ = fs::set_permissions(&temporary, metadata.permissions());
    }
    fs::rename(&temporary, path).map_err(|source| {
        let _ = fs::remove_file(&temporary);
        (path.to_path_buf(), source)
    })
}

/// A new, empty file beside `path` to build its replacement in, named after
/// it and this run (`weaver.toml.<pid>-<suffix>.tmp`).
///
/// Not `weaver.toml.tmp`, the one name every writer used to share: two
/// writers at once wrote into one temporary, so either could rename the
/// other's half-written bytes into place, and the second rename found
/// nothing to move. Made with `create_new`, so a file somebody else has
/// under that name is never written into.
fn temporary_beside(path: &Path) -> Result<(PathBuf, fs::File), (PathBuf, io::Error)> {
    let mut attempts = 0;
    loop {
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(format!(".{}.tmp", crate::copy::unique_name()));
        let temporary = path.with_file_name(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && attempts < 8 => {
                attempts += 1;
            }
            Err(error) => return Err((temporary, error)),
        }
    }
}

/// An absolute, `..`-free path, without requiring the path to exist yet.
///
/// The normalising half is not cosmetic. A workspace resolves an entry by
/// joining it onto its own root, so a project registered as `../collapse`
/// arrives here as `<workspace>/../collapse`; `std::path::absolute` leaves
/// that `..` in place. Every root would then carry it, and two paths naming
/// one directory would stop comparing equal, which is what `in_project` and
/// the link deduplication are built on.
pub(crate) fn absolute(path: &Path) -> Result<PathBuf, WorkspaceError> {
    let absolute = std::path::absolute(path).map_err(|source| WorkspaceError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(paths::normalize(&absolute))
}

/// The name to give a project or workspace created in `root`.
fn directory_name(root: &Path) -> String {
    root.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("mindweaver")
        .to_owned()
}

/// The marker files are written from templates rather than serialized, so the
/// comments explaining the layout are in the file the user opens first.
///
/// Every kind's directory is spelled out even when it is the default, so the
/// answer to "where do this project's skills go" is in the file rather than in
/// a function somebody has to go and read.
///
/// The `mind.toml` a version 2 build wrote, byte for byte, its old name for
/// the product included: only tests of such a project write one now, and
/// what they hold is what those builds left on the disk.
fn mind_template(name: &str, directories: &Directories) -> String {
    let configured = directory_lines(directories.resolved());

    format!(
        "# Mindflayer mind project.\n\
         #\n\
         # Skills are a directory each, holding a SKILL.md, so a skill can carry\n\
         # scripts and references beside its instructions. Rules are one markdown\n\
         # file each, giving context and declaring nothing; folders under the rules\n\
         # directory group and mean nothing else, so git/no-force-push.md is the\n\
         # rule `git/no-force-push`.\n\
         #\n\
         # `directories` says where each kind lives, relative to the directory\n\
         # holding this {MIND_DIR}. Point them wherever the agents that read them\n\
         # already look (.claude/skills, docs/rules) and Mindflayer follows.\n\
         # They are meant to be committed: they travel with the project.\n\
         version = {MIND_TOML_VERSION}\n\
         name = {}\n\
         \n\
         [directories]\n\
         {configured}",
        toml_string(name),
    )
}

/// The marker a new workspace is written with, its directories and comments
/// spelled under `names`: Mindweaver's for every one a command makes.
fn weaver_template(name: &str, directories: &Directories, names: Names) -> String {
    let dir = names.workspace_dir();
    format!(
        "# Mindweaver workspace.\n\
         #\n\
         # It orchestrates the mind projects listed below, so their skills can be\n\
         # managed together. Paths are relative to this file's grandparent, the\n\
         # directory holding {dir}.\n\
         #\n\
         # `directories` says where the workspace keeps its own skills and rules,\n\
         # the ones every project it manages can install. By default that is its\n\
         # library, inside {dir} where no agent looks, and a kind not named\n\
         # here is kept there too. Each folder is made the first time something\n\
         # is added to it.\n\
         #\n\
         # `loaded` lists other weaver workspaces whose own skills its projects\n\
         # can install too, read from there each time: `mindweaver load <path>`.\n\
         version = {WEAVER_FORMAT_VERSION}\n\
         name = {}\n\
         projects = []\n\
         loaded = []\n\
         \n\
         [directories]\n\
         {}",
        toml_string(name),
        // Spelled out for the kinds every marker has named since markers
        // said where a kind lives, the ones with a project's default too:
        // one added since (MCP) is kept in the library whatever a marker
        // says (D-05), as the comment above tells, so a line for it would
        // tell a reader nothing, and make the marker a new workspace gets
        // differ from the one an older build wrote for no reader of either.
        // One `directories` names is written all the same.
        directory_lines(directories.resolved_with(|kind| {
            Directories::default_for(kind).map(|_| Directories::library_under(names, kind))
        })),
    )
}

/// Where the marker of a workspace at `root` kept under `names` is.
fn marker_under(root: &Path, names: Names) -> PathBuf {
    root.join(names.workspace_dir())
        .join(names.workspace_config())
}

/// `skills = "skills"`, a line per kind, spelled out even for the defaults.
fn directory_lines(resolved: Vec<(Kind, PathBuf)>) -> String {
    resolved
        .into_iter()
        .map(|(kind, directory)| {
            format!(
                "{} = {}\n",
                kind.folder(),
                toml_string(&directory.to_string_lossy())
            )
        })
        .collect()
}

/// Whether a kind's directory stays inside what it belongs to.
///
/// What a mind project authors is the project's, written beside its code, so
/// a kind's directory has to be somewhere inside it; a workspace's own
/// artifacts belong to it the same way. Anything else describes artifacts
/// that are not the holder's, and `mind init --skills /etc` should say so
/// rather than making the folder.
///
/// The test is that the first component, once `.` and `..` are resolved, is a
/// plain name. Not `is_absolute()`, which is **false on Windows** for
/// `/etc/skills`: a path with a root but no drive letter is relative to the
/// current drive's root, which is still not inside anything. Asking for a
/// plain name instead covers absolute paths, rooted ones, `C:` prefixes and
/// `..` at once, on both platforms, with one rule and no platform branch.
fn stays_inside(directory: &Path) -> bool {
    matches!(
        paths::normalize(directory).components().next(),
        Some(std::path::Component::Normal(_))
    )
}

/// Why `directory`, a route from a project's root, cannot be where its
/// settings say it authors `kind`s: it leads out of the project
/// ([`stays_inside`]), or into its `.mind`.
///
/// `.mind` is Mindweaver's: the marker, the templates, the stage an install
/// builds in. A local project's keeps itself out of git, so a folder there
/// would hide what the project wrote from the team without a word, where the
/// point of authoring beside the code (D-06) is that it is committed with it.
/// Refused in a shared project too, whose share a change may later make
/// local. Only when writing settings: a `mind.toml` of version 1 kept every
/// kind inside `.mind`, and is still read that way.
pub(crate) fn misplaced_authored(kind: Kind, directory: &Path) -> Option<WorkspaceError> {
    let path = directory.to_path_buf();
    if !stays_inside(directory) {
        return Some(WorkspaceError::OutsideProject { path, kind });
    }
    starts_in(directory, MIND_DIR).then_some(WorkspaceError::InsideMind { path, kind })
}

/// Whether `directory`, a route from a project's root, starts in the folder
/// named `folder`, without regard to case, as `gathered_overlap` compares,
/// since `.MIND` is the same folder as `.mind` where the file system ignores
/// case.
fn starts_in(directory: &Path, folder: &str) -> bool {
    matches!(
        paths::normalize(directory).components().next(),
        Some(std::path::Component::Normal(first)) if first.eq_ignore_ascii_case(folder)
    )
}

/// Why `folder`, a route from a project's root that its settings name in
/// `targets`, is no place to install `kind`s in, if it is not.
///
/// Inside the project, as a folder it authors in is, so a copy is that
/// project's and not some other's, or nobody's: the copies go where the
/// project's agents are pointed, which only the project can promise. Not in
/// `.mind`, which no agent reads and a local project keeps out of git, nor
/// in a `.mindweaver` or the `.mindflayer` a Mindflayer build made, which no
/// agent reads either, and whose shelf and
/// cache gathering replaces and deletes by names of its own: a workspace at
/// the project's root keeps both there. Nor in a `.git`, the project's or
/// that of a repository nested in it, which no agent reads and git owns: a
/// skill named `logs` or `worktrees` would be written where git keeps its
/// own, and a shared project's settings would send it there in every clone.
/// Only a kind installed as a folder an artifact has targets at all; a rule
/// reaches agents through `AGENTS.md` (D-03), which M2 puts in the target
/// table.
///
/// What install refuses of any target, an agent's own folder and one a
/// loaded workspace keeps as its own, it refuses of these too, when it
/// knows the homes and the loads, which settings do not.
pub(crate) fn misplaced_target(kind: Kind, folder: &Path) -> Option<Misplaced> {
    if !matches!(kind.layout(), Layout::Directory { .. }) {
        return Some(Misplaced::NotInstalled);
    }
    if !stays_inside(folder) {
        return Some(Misplaced::Outside);
    }
    if starts_in(folder, MIND_DIR) {
        return Some(Misplaced::InsideMind);
    }
    let parts = paths::normalize(folder);
    let named = |name: &str| {
        parts
            .components()
            .any(|part| part.as_os_str().eq_ignore_ascii_case(name))
    };
    if Names::ALL
        .into_iter()
        .any(|names| named(names.workspace_dir()))
    {
        return Some(Misplaced::InsideWorkspace);
    }
    named(GIT_DIR).then_some(Misplaced::InsideGit)
}

/// A repository's own folder, which no install may write in.
const GIT_DIR: &str = ".git";

/// Why `directory`, a route from a workspace's root, cannot hold the
/// workspace's own `kind`s: it leads out of the workspace, or it overlaps
/// what is reserved for gathering ([`gathered_overlap`]).
///
/// What [`WeaverWorkspace::init_with`] refuses before it makes anything, and
/// what a migration refuses to move out of
/// ([`crate::migrate::library_plan`]). `open` still reads a marker edited by
/// hand to name such a folder, since reading one changes nothing; moving out
/// of it would empty another directory, or gathering's, into this one.
pub(crate) fn misplaced_own(kind: Kind, directory: &Path) -> Option<WorkspaceError> {
    if !stays_inside(directory) {
        return Some(WorkspaceError::OutsideWorkspace {
            path: directory.to_path_buf(),
            kind,
        });
    }
    gathered_overlap(directory).map(|gathered| WorkspaceError::OverlapsGathered {
        path: directory.to_path_buf(),
        kind,
        gathered,
    })
}

/// The folder reserved for gathering that a workspace's own `directory` would
/// overlap, if any: the shelf, a folder per kind
/// ([`WeaverWorkspace::gathered_dir`]), or the cache of clones
/// ([`WeaverWorkspace::cache_dir`]), as a route from the workspace's root.
///
/// Those folders are gathering's alone. It replaces and deletes what is in
/// them by names of its own, so an artifact somebody wrote there could go with
/// the next gather, and a listing could no longer tell what was gathered from
/// what the workspace keeps. Overlapping counts either way round:
/// `.mindweaver` holds both, and a rules folder there would read every
/// markdown file in every clone as one of the workspace's rules. The rest of
/// `.mindweaver` is not gathering's, so a directory there is allowed.
///
/// Under both names, whichever the workspace is kept under: a `.mindflayer`
/// becomes a `.mindweaver` when it is moved, its shelf and cache with it, so
/// a folder of its own in the one would be gathering's in the other.
///
/// Every kind's shelf is reserved, not only a gatherable kind's, rather than
/// matched on the kind as `gather` is: scrubbing a source already deletes its
/// folder from each one (`gather::remove_folders_of`), and a kind made
/// gatherable later has to find its shelf free, not holding a workspace's
/// own.
///
/// Compared a component at a time, ignoring ASCII case: where the filesystem
/// does, as macOS's and Windows' do by default, `.Mindweaver/Skills` is the
/// shelf, and refusing it on Linux as well costs nobody anything.
fn gathered_overlap(directory: &Path) -> Option<PathBuf> {
    let directory = paths::normalize(directory);
    // Spelled with `/`, so a refusal reads the same on every platform.
    Names::ALL
        .into_iter()
        .flat_map(|names| {
            Kind::ALL
                .into_iter()
                .map(Kind::folder)
                .chain(std::iter::once(CACHE_DIR))
                .map(move |folder| PathBuf::from(format!("{}/{folder}", names.workspace_dir())))
        })
        .find(|gathered| {
            directory
                .components()
                .zip(gathered.components())
                .all(|(one, other)| one.as_os_str().eq_ignore_ascii_case(other.as_os_str()))
        })
}

/// A TOML basic string. Directory names are arbitrary, so the quoting is not
/// optional even though the common case never needs it.
///
/// A basic string takes no control character as it is but a tab, so every
/// other one is escaped: a name holding an ESC or a CR, which `mindweaver init
/// PATH` takes as typed, would otherwise be written into a marker that no
/// command, `init` again included, could read. The C1 controls are allowed
/// as they are, and escaped all the same, since `char::is_control` is the
/// one test for the lot and the escape reads the same back.
pub(crate) fn toml_string(value: &str) -> String {
    let mut quoted = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\t' => quoted.push_str("\\t"),
            c if c.is_control() => quoted.push_str(&format!("\\u{:04X}", u32::from(c))),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
#[path = "workspace_test.rs"]
mod tests;
