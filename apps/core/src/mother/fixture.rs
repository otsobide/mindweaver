//! Mothers of what lives on the disk: a workspace, a project, a git
//! repository to gather from, a workspace with a shelf already filled, and
//! what a Mindflayer build left under its own names.
//!
//! Each lives in a temporary folder the caller holds, and goes when it is
//! dropped, so no test reads or writes outside the folder it was given. The
//! git repository is written in process with `gix`, with no network and no
//! `git` binary, and asks nothing of the machine's git configuration.

use std::fs;
use std::path::Path;

use gix::objs::tree::{Entry, EntryKind};
use tempfile::TempDir;

use crate::agent::Agent;
use crate::agents_md::AgentsMd;
use crate::claude_md::ClaudeMd;
use crate::claude_rules;
use crate::exclude::ExcludeFile;
use crate::gather::{self, Request};
use crate::kind::Kind;
use crate::ledger::Ledger;
use crate::mother::artifact::{RuleMother, SkillMother};
use crate::mother::random;
use crate::mother::value::ProjectIdMother;
use crate::names::Names;
use crate::settings::Share;
use crate::workspace::{Directories, MindProject, WeaverWorkspace};

/// Makes weaver workspaces.
pub struct WorkspaceMother;

impl WorkspaceMother {
    /// A workspace with nothing linked, loaded or gathered, keeping its own
    /// in its library.
    pub fn empty() -> (TempDir, WeaverWorkspace) {
        let dir = TempDir::new().expect("make a temporary folder");
        let (workspace, _) = WeaverWorkspace::init(dir.path()).expect("make a workspace");
        (dir, workspace)
    }

    /// A workspace in `root`, made if it is not yet one.
    pub fn create(root: &Path) -> WeaverWorkspace {
        fs::create_dir_all(root).expect("make the workspace's folder");
        WeaverWorkspace::init(root).expect("make a workspace").0
    }
}

/// Makes what a Mindflayer build left on the disk, under the names it wrote
/// before #229 (D-26), for a test of reading them and of moving them.
///
/// Through core's own writers, told those names, rather than files spelled
/// here: what a test reads back is what such a build wrote, comments and
/// all, and stays so when the format moves on. No command can be told them,
/// so nothing but a mother makes a workspace under the old names.
pub struct MindflayerMother;

impl MindflayerMother {
    /// A workspace such a build made in a temporary folder, in its
    /// `.mindflayer`, with nothing linked, loaded or gathered.
    pub fn empty() -> (TempDir, WeaverWorkspace) {
        let dir = TempDir::new().expect("make a temporary folder");
        let workspace = Self::create(dir.path());
        (dir, workspace)
    }

    /// A workspace such a build made in `root`: `.mindflayer/flayer.toml`,
    /// keeping its own in `.mindflayer/library`, its ledger made as
    /// `mindflayer.db` the first time it is opened.
    pub fn create(root: &Path) -> WeaverWorkspace {
        fs::create_dir_all(root).expect("make the workspace's folder");
        WeaverWorkspace::init_under(root, &Directories::default(), Names::Mindflayer)
            .expect("make a workspace under Mindflayer's names")
            .0
    }

    /// Leave what core rendered into the project at `root` as such a build
    /// rendered it: the block of its `AGENTS.md`, the import in its
    /// `CLAUDE.md` or `.claude/CLAUDE.md` and the block of the exclude file
    /// git reads for it under that build's markers, every other byte as core
    /// wrote it.
    ///
    /// Moved back by core's own parsers rather than spelled here, so what a
    /// test reads is what such a build wrote, line endings and all.
    pub fn rendered_into(root: &Path) {
        let agents = root.join(crate::agents_md::FILE);
        if let Ok(bytes) = fs::read(&agents) {
            let mut file = AgentsMd::parse(&bytes).expect("read the block");
            if file.renamed(Names::Mindflayer) {
                fs::write(&agents, file.to_bytes()).expect("write the block back");
            }
        }
        // A `CLAUDE.md` that is the `AGENTS.md`, through a link one way or
        // the other, was moved back with it.
        let agents = fs::canonicalize(&agents).ok();
        for claude in [root.join("CLAUDE.md"), root.join(".claude/CLAUDE.md")] {
            if agents.is_some() && fs::canonicalize(&claude).ok() == agents {
                continue;
            }
            let Ok(bytes) = fs::read(&claude) else {
                continue;
            };
            let mut file = ClaudeMd::parse(&bytes).expect("read the import");
            if file.renamed(Names::Mindflayer) {
                fs::write(&claude, file.to_bytes()).expect("write the import back");
            }
        }
        let excluding = crate::exclude::locate(root).expect("find the exclude file");
        if let Some(location) = excluding {
            if let Ok(bytes) = fs::read(&location.file) {
                let mut file = ExcludeFile::parse(&bytes).expect("read the exclude block");
                if file.renamed(Names::Mindflayer) {
                    fs::write(&location.file, file.to_bytes()).expect("write the block back");
                }
            }
        }
    }

    /// [`rendered_into`](Self::rendered_into), and Claude's rule files filed
    /// where such a build filed them: each file core wrote into
    /// `.claude/rules/mindweaver` moved to `.claude/rules/mindflayer`, its
    /// record in `workspace`'s ledger and its line in the exclude block with
    /// it, as that build recorded and excluded them.
    pub fn installed_into(workspace: &WeaverWorkspace, project: &MindProject) {
        Self::rendered_into(project.root());
        let root = project.root();
        let new = root.join(claude_rules::folder_under(Names::Mindweaver));
        let old = root.join(claude_rules::folder_under(Names::Mindflayer));
        if new.exists() {
            fs::create_dir_all(old.parent().expect("a folder above")).expect("make .claude/rules");
            fs::rename(&new, &old).expect("move the rule files");
        }
        let ledger = workspace.ledger().expect("open the ledger");
        let entry = crate::install::entry_for_root(workspace, root);
        for record in ledger
            .records(&entry, Kind::Rule)
            .expect("read the records")
        {
            let targets: Vec<_> = record
                .targets
                .iter()
                .cloned()
                .map(|mut target| {
                    if crate::install::rules::claude_file_names(&target.path, &record.name)
                        == Some(Names::Mindweaver)
                    {
                        target.path = crate::install::rules::claude_file_under(
                            Names::Mindflayer,
                            &record.name,
                        );
                    }
                    target
                })
                .collect();
            ledger
                .retargeted(&entry, Kind::Rule, &record.name, &targets)
                .expect("record the files where they are");
        }
        if let Some(location) = crate::exclude::locate(root).expect("find the exclude file") {
            if let Ok(text) = fs::read_to_string(&location.file) {
                let moved =
                    text.replace("/.claude/rules/mindweaver/", "/.claude/rules/mindflayer/");
                fs::write(&location.file, moved).expect("write the exclude lines back");
            }
        }
    }
}

/// Makes mind projects.
pub struct ProjectMother;

impl ProjectMother {
    /// A project in a folder of `parent` named by a word, marked by its
    /// settings, for some agents, either share mode.
    pub fn random(parent: &Path) -> MindProject {
        let agents = random::some_of(&Agent::ALL);
        let share = random::pick(&[Share::Local, Share::Shared]);
        Self::create(&parent.join(random::word()), &agents, share)
    }

    /// The project `root` is, marked by its settings as `mind init` marks
    /// one, for `agents`.
    pub fn create(root: &Path, agents: &[Agent], share: Share) -> MindProject {
        fs::create_dir_all(root).expect("make the project's folder");
        MindProject::init_asked(root, agents, share, &Directories::default())
            .expect("make a project")
            .0
    }

    /// The project `root` is, marked by its settings as `mind init` marks
    /// one, with a folder of MCP servers its settings name beside the
    /// folders it is given by default, which name none.
    pub fn with_mcp(root: &Path, agents: &[Agent], share: Share) -> MindProject {
        fs::create_dir_all(root).expect("make the project's folder");
        let directories = Directories::default().with(crate::kind::Kind::Mcp, "mcp");
        MindProject::init_asked(root, agents, share, &directories)
            .expect("make a project")
            .0
    }

    /// The project `root` is, marked by a `mind.toml` as a version 2 `mind
    /// init` did, so a copy is installed into the folder it authors in.
    pub fn older(root: &Path) -> MindProject {
        fs::create_dir_all(root).expect("make the project's folder");
        MindProject::init(root).expect("make a project").0
    }

    /// The project `root` is, marked by the `mind.toml` a version 1 build
    /// wrote, byte for byte, which kept every kind it knew inside `.mind`.
    pub fn version_1(root: &Path) -> MindProject {
        let mind = root.join(crate::workspace::MIND_DIR);
        fs::create_dir_all(&mind).expect("make the project's .mind");
        fs::write(
            mind.join(crate::workspace::MIND_CONFIG),
            include_str!("../../tests/fixtures/mind-v1.toml"),
        )
        .expect("write its marker");
        MindProject::open(root).expect("open the project")
    }

    /// The project `root` is, marked by the `settings.json` a version 3
    /// `mind init` wrote, laid out as it laid one out: every agent,
    /// local, with the `.gitignore` that kept `.mind` out of git, and the
    /// folders `directories` names, each made. Written here rather than
    /// through core, which writes a `settings.toml` and never a
    /// `settings.json`, so such a project stays what that build made
    /// whatever `mind init` writes now. A version 3 build named `skills`
    /// and `rules`; `directories` naming a kind it did not know (`mcp`) is
    /// a key somebody wrote into the file by hand.
    pub fn json(root: &Path, directories: &Directories) -> MindProject {
        let mind = root.join(crate::workspace::MIND_DIR);
        fs::create_dir_all(&mind).expect("make the project's .mind");
        fs::write(
            mind.join(".gitignore"),
            crate::workspace::FLAYER_MIND_IGNORE,
        )
        .expect("write its .gitignore");
        let name = root
            .file_name()
            .and_then(|name| name.to_str())
            .expect("a project's folder named in UTF-8");
        // Each a JSON string as the build wrote it: a folder's name and a
        // route of them hold nothing a JSON string escapes otherwise than
        // Rust's `Debug` does.
        let named: Vec<String> = directories
            .entries()
            .into_iter()
            .map(|(key, folder)| format!("    {key:?}: {:?}", folder.to_str().expect("UTF-8")))
            .collect();
        fs::write(
            mind.join(crate::settings::SETTINGS_JSON_FILE),
            format!(
                "{{\n  \"version\": {},\n  \"id\": \"{}\",\n  \"name\": {name:?},\n  \
                 \"agents\": [\"claude\", \"codex\", \"opencode\"],\n  \"share\": \"local\",\n  \
                 \"edit_tracked\": false,\n  \"directories\": {{\n{}\n  }}\n}}\n",
                crate::workspace::SETTINGS_VERSION,
                ProjectIdMother::random(),
                named.join(",\n")
            ),
        )
        .expect("write its settings.json");
        for (_, folder) in directories.entries() {
            fs::create_dir_all(root.join(folder)).expect("make a folder it names");
        }
        MindProject::open(root).expect("open the project")
    }

    /// An older project in `workspace`'s folder named `name`, linked to it.
    pub fn linked(workspace: &mut WeaverWorkspace, name: &str) -> MindProject {
        let project = Self::older(&workspace.root().join(name));
        workspace.link(&project).expect("link the project");
        project
    }
}

/// Makes git repositories on the disk, each with one commit, to gather from.
pub struct GitRepositoryMother;

impl GitRepositoryMother {
    /// A repository holding exactly `files`, each a path and its contents.
    pub fn create(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().expect("make a temporary folder");
        Self::write(dir.path(), files);
        Self::commit(dir.path());
        dir
    }

    /// A repository holding a skill for each of `names`, under `skills/`.
    pub fn with_skills(names: &[&str]) -> TempDir {
        Self::with_skills_in("skills", names)
    }

    /// A repository holding a skill for each of `names` in `folder`, a
    /// route from its root, `.` for the root itself, where each is then a
    /// folder of its own.
    pub fn with_skills_in(folder: &str, names: &[&str]) -> TempDir {
        let files: Vec<(String, String)> = names
            .iter()
            .map(|name| {
                (
                    format!("{folder}/{name}/SKILL.md"),
                    SkillMother::create(name, &format!("What {name} does")),
                )
            })
            .collect();
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        Self::create(&borrowed)
    }

    /// Every folder a gather reads a repository's skills from when it holds
    /// no `SKILL.md` at its root (#101), each a route from that root: the
    /// root itself, whose every folder holding one is a skill, then
    /// `skills/`, a namespace of it, its two hidden ones, the folders agents
    /// read in a project, and a plugin's. The namespace and the plugin are
    /// named at random, as a repository names its own.
    pub fn layouts() -> Vec<String> {
        vec![
            String::from("."),
            String::from("skills"),
            format!("skills/{}", random::kebab()),
            String::from("skills/.curated"),
            String::from("skills/.system"),
            String::from(".agents/skills"),
            String::from(".claude/skills"),
            format!("plugins/{}/skills", random::kebab()),
        ]
    }

    /// A repository that is one skill, `name`: a `SKILL.md` at its root,
    /// beside a `README.md`, a script, and an example the skill ships of
    /// another, whose `SKILL.md` is the skill's file like any other.
    pub fn one_skill(name: &str) -> TempDir {
        Self::create(&[
            (
                "SKILL.md",
                &SkillMother::create(name, &format!("What {name} does")),
            ),
            ("README.md", "# One skill\n"),
            ("scripts/run.sh", "echo run\n"),
            (
                "examples/inner/SKILL.md",
                &SkillMother::create("inner", "An example"),
            ),
        ])
    }

    /// A repository keeping a rule at each of `routes` in `folder`, a route
    /// from its root, `.` for the root itself (#111): `<route>.md` each, a
    /// rule about the route's last segment, beside the `README.md` and the
    /// `LICENSE` a collection of rules documents itself with, at the top of
    /// `folder`, and a `CHANGELOG.md` at the repository's root, which a
    /// gather of the whole repository would take as one more.
    pub fn with_rules_in(folder: &str, routes: &[&str]) -> TempDir {
        let mut files: Vec<(String, String)> = routes
            .iter()
            .map(|route| {
                let topic = route.rsplit('/').next().unwrap_or(route);
                (format!("{folder}/{route}.md"), RuleMother::create(topic))
            })
            .collect();
        files.push((
            format!("{folder}/README.md"),
            String::from("# Rules\n\nWhat this collection is for.\n"),
        ));
        files.push((format!("{folder}/LICENSE"), Self::LICENCE.to_owned()));
        files.push((
            String::from("CHANGELOG.md"),
            String::from("# Changelog\n\n- Every rule, first.\n"),
        ));
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        Self::create(&borrowed)
    }

    /// What the `LICENSE` of [`with_rules_in`](Self::with_rules_in) holds:
    /// its first line is what a gather says every rule there is under.
    pub const LICENCE: &str = "MIT License\n\nCopyright (c) the authors\n";

    /// The URL a gather is handed for a repository on the disk: its path, for
    /// the local transport.
    pub fn url(repository: &TempDir) -> String {
        repository.path().to_string_lossy().into_owned()
    }

    /// Write `files` under `root`, making the folders they are in.
    pub fn write(root: &Path, files: &[(&str, &str)]) {
        for (path, contents) in files {
            let file = root.join(path);
            fs::create_dir_all(file.parent().expect("a file has a parent")).unwrap();
            fs::write(file, contents).unwrap();
        }
    }

    /// Commit the whole working tree, without a git identity configured.
    ///
    /// The commit object is written by hand and the branch file pointed at
    /// it, which is all a clone needs. Opened rather than initialized when it
    /// is already a repository, so a fixture can commit a second time and
    /// stand for a source that moved on.
    pub fn commit(root: &Path) {
        let repository = if root.join(".git").is_dir() {
            gix::open(root).expect("open the repository")
        } else {
            gix::init(root).expect("initialize a repository")
        };
        let parents: Vec<gix::ObjectId> = repository
            .head_id()
            .map(|id| vec![id.detach()])
            .unwrap_or_default();
        let tree = Self::tree(&repository, root);
        let who = gix::actor::Signature {
            name: "Fixture".into(),
            email: "fixture@example.com".into(),
            time: gix::date::Time::new(0, 0),
        };
        let commit = gix::objs::Commit {
            tree,
            parents: parents.into(),
            author: who.clone(),
            committer: who,
            encoding: None,
            message: "fixture".into(),
            extra_headers: Vec::new(),
        };
        let id = repository
            .write_object(&commit)
            .expect("write the commit")
            .detach();

        let head = fs::read_to_string(root.join(".git").join("HEAD")).unwrap();
        let branch = head
            .trim()
            .strip_prefix("ref: ")
            .expect("a fresh repository has a symbolic HEAD");
        let reference = root.join(".git").join(branch);
        fs::create_dir_all(reference.parent().unwrap()).unwrap();
        fs::write(reference, format!("{id}\n")).unwrap();
    }

    /// Write `dir`, all but `.git`, as a tree of `repository`'s.
    pub fn tree(repository: &gix::Repository, dir: &Path) -> gix::ObjectId {
        let mut entries = Vec::new();
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            if name == ".git" {
                continue;
            }
            let path = entry.path();
            let (kind, oid) = if path.is_dir() {
                (EntryKind::Tree, Self::tree(repository, &path))
            } else {
                (
                    EntryKind::Blob,
                    repository
                        .write_blob(fs::read(&path).unwrap())
                        .unwrap()
                        .detach(),
                )
            };
            entries.push(Entry {
                mode: kind.into(),
                filename: name.to_string_lossy().as_bytes().into(),
                oid,
            });
        }
        // Git requires a tree's entries in its own order, which `Entry`
        // sorts by: an unsorted one is an object git rejects.
        entries.sort();
        repository
            .write_object(&gix::objs::Tree { entries })
            .unwrap()
            .detach()
    }

    /// Put every file under `work_tree` in its repository's index, as `git
    /// add -A` would, so git tracks them, committed or not.
    pub fn index(work_tree: &Path) {
        let repository = gix::open_opts(work_tree, gix::open::Options::isolated()).unwrap();
        let tree = Self::tree(&repository, work_tree);
        let mut index = repository.index_from_tree(&tree).unwrap();
        index.write(Default::default()).unwrap();
    }
}

/// Makes workspaces whose shelf already holds what was gathered.
pub struct ShelfMother;

impl ShelfMother {
    /// A workspace that gathered a skill for each of `names` from one
    /// repository, and its ledger. The repository is gone once the gather
    /// took what it holds, as a clone keeps it on the shelf.
    pub fn gathered(names: &[&str]) -> (TempDir, WeaverWorkspace, Ledger) {
        let (dir, workspace) = WorkspaceMother::empty();
        let source = GitRepositoryMother::with_skills(names);
        let ledger = workspace.ledger().expect("open the ledger");
        gather::gather(
            &workspace,
            &ledger,
            &Request::git(GitRepositoryMother::url(&source)),
        )
        .expect("gather the skills");
        (dir, workspace, ledger)
    }

    /// Gather a rule for each of `routes` onto the shelf of `workspace`,
    /// from the `rules` folder of one repository
    /// ([`GitRepositoryMother::with_rules_in`]), which is gone once the
    /// gather took what it holds: what the gather said, the source's name,
    /// which `--from` takes, among it.
    pub fn rules_into(
        workspace: &WeaverWorkspace,
        ledger: &Ledger,
        routes: &[&str],
    ) -> gather::Report {
        let source = GitRepositoryMother::with_rules_in("rules", routes);
        gather::gather(
            workspace,
            ledger,
            &Request::git(GitRepositoryMother::url(&source))
                .from_subdirectory("rules")
                .taking(Kind::Rule),
        )
        .expect("gather the rules")
    }
}
