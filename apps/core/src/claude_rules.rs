//! A project's rules, rendered as Claude Code's rule files.
//!
//! Claude reads every markdown file under a project's `.claude/rules`, at any
//! depth, and one whose front matter states `paths` only once it reads a
//! file those patterns match (docs/agents.md, verified on 2.1.309). No other
//! agent reads that folder, and none honours `paths`. So a rule about some
//! files only reaches its project as a file there, and never as a section of
//! the block in `AGENTS.md` ([`crate::agents_md`]), where every agent would
//! read it for every file (D-03):
//!
//! ```text
//! .claude/rules/mindweaver/rust/errors.md
//! ```
//!
//! Such a rule reaches Claude alone, which is said ([`Warning::ClaudeOnly`]).
//! The same files give Claude the rules about every file when it reads no
//! block, in [`Mode::Fallback`]. Which mode applies is the caller's to say,
//! from how Claude reaches the block ([`crate::claude_md::Reach::mode`]),
//! never from whether a block was written alone, so Claude reads each rule
//! once and misses none.
//!
//! A file holds the rule as written, front matter and all: Claude takes
//! `paths` from it and strips the rest before the rule reaches the model
//! (docs/agents.md, docs-only), and a file that is the rule byte for byte
//! is one somebody can open and recognise. Below `mindweaver`, a folder of
//! Mindweaver's own beside whatever somebody keeps in `.claude/rules`, a
//! file is filed by the rule's route, folders and all, as in the library,
//! so two rules never share one.
//!
//! A Mindflayer build filed them below `.claude/rules/mindflayer` (D-26,
//! [`Names`]). A file the ledger records there stays there, written over and
//! taken away where it is, until `migrate --names` carries it and its record
//! to the new folder; only a rule recorded nowhere is given a file in the
//! new one, and somebody's file at either place keeps it out, so no rule is
//! read twice by Claude, once from each folder.
//!
//! Which files are Mindweaver's is told by what the caller hands back of an
//! earlier render ([`Recorded`]): each file's route, and the hash of what it
//! held ([`hash::bytes`]). A file is Mindweaver's while it holds exactly
//! that, and anything else at a rule's place is somebody's, written over
//! and taken away by nothing. Keeping the record is the caller's
//! ([`crate::install::rules`], in the ledger), as is deciding which rules a
//! project is given and when.
//!
//! A local project keeps the files out of git (D-02), one entry each in its
//! group of the exclude block ([`crate::exclude`]), written before they land
//! and taken out after they go, as an install's copies are. And none is
//! written where the block of `AGENTS.md` would be refused
//! ([`crate::agents_md::misplaced`]), nor through a link out of the project.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::agent::{Agent, AgentHomes};
use crate::agents_md::{self, Misplaced};
use crate::artifact::{issue_list, name_issues, Artifact, ArtifactError, Declared};
use crate::catalog::{report_order, spelled};
use crate::code::codes;
use crate::exclude::{self, ExcludeError, Excluded};
use crate::hash;
use crate::install;
use crate::names::Names;
use crate::value::ContentHash;
use crate::workspace::{replace_bytes_unresolved, MindProject, WeaverWorkspace};

/// The folders, from a project's root, the files go in.
const FOLDER: [&str; 3] = [".claude", "rules", "mindweaver"];
/// [`FOLDER`] as a Mindflayer build named it, where the files it recorded
/// stay until they are moved ([`Names::Mindflayer`]).
const FLAYER_FOLDER: [&str; 3] = [".claude", "rules", "mindflayer"];

/// What a rule's file is called after the last word of its route: the
/// extension Claude reads in `.claude/rules`.
const EXTENSION: &str = ".md";

/// Where the files go, relative to a project's root:
/// `.claude/rules/mindweaver`.
///
/// A folder of Mindweaver's own rather than `.claude/rules` itself, where
/// somebody may keep rules by hand: a route can never name one of theirs,
/// and the folder goes once the last file in it does.
pub fn folder() -> PathBuf {
    folder_under(Names::Mindweaver)
}

/// The folder the files are filed in under `names`:
/// `.claude/rules/mindflayer` for those a Mindflayer build wrote.
pub fn folder_under(names: Names) -> PathBuf {
    match names {
        Names::Mindweaver => FOLDER.iter().collect(),
        Names::Mindflayer => FLAYER_FOLDER.iter().collect(),
    }
}

/// The file the rule at `route` goes to, relative to a project's root.
///
/// Joined a folder at a time, so it is the same path on every platform.
pub fn file_for(route: &str) -> PathBuf {
    file_under(Names::Mindweaver, route)
}

/// The file the rule at `route` is filed as under `names`.
pub fn file_under(names: Names, route: &str) -> PathBuf {
    let mut path = folder_under(names);
    path.extend(route.split('/'));
    let name = route.rsplit('/').next().unwrap_or(route);
    path.set_file_name(format!("{name}{EXTENSION}"));
    path
}

/// Which rules a render writes, as its caller decides from whether Claude
/// reads the block in `AGENTS.md` ([`crate::claude_md::Reach::mode`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Only the rules about some files: Claude reads the others in the block
    /// of `AGENTS.md`, natively or through an import, and a file of theirs
    /// here would have it read them twice.
    Scoped,
    /// Every rule: Claude reads no block, so the rules about every file come
    /// here too, or it would miss them.
    Fallback,
}

/// A rule file an earlier render left Mindweaver's: its route, the hash of
/// what it held ([`hash::bytes`]), and whose folder it is in.
///
/// What the caller keeps ([`crate::install::rules`]), from
/// [`Rendered::recorded`], and hands back to the next render, which takes a
/// file as Mindweaver's only while it holds what was recorded, and looks
/// for it where it was recorded: in the folder a Mindflayer build filed it
/// in, until it is moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub route: String,
    pub hash: ContentHash,
    pub names: Names,
}

/// What [`render`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// Each rule's file the render wrote or found, and each route recorded,
    /// in the order of their routes.
    pub files: Vec<RuleFile>,
    /// What there is to say, file by file in the same order.
    pub warnings: Vec<Warning>,
    /// What keeping the files out of git did, for a local project (D-02):
    /// `None` for a shared one, one a `mind.toml` marks, and a render that
    /// leaves no file of Mindweaver's. [`Excluded::warning`] says when
    /// nothing keeps them out, outside a git work tree.
    pub excluded: Option<Excluded>,
    /// Why the exclude block still names a file the render took away, or
    /// failed to make, when it could not be edited: a line left there keeps
    /// whatever somebody writes in that place later out of git.
    pub still_excluded: Option<String>,
}

impl Rendered {
    /// What the caller keeps and hands back to the next render: every file
    /// left Mindweaver's, with what it holds.
    pub fn recorded(&self) -> Vec<Recorded> {
        self.files
            .iter()
            .filter_map(|file| {
                Some(Recorded {
                    route: file.route.clone(),
                    hash: file.hash.clone()?,
                    names: file.names,
                })
            })
            .collect()
    }
}

/// One rule's file, and what a render did to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFile {
    /// The rule's route, which names the file.
    pub route: String,
    /// Where the file is, or would be.
    pub path: PathBuf,
    /// Whose folder `path` is in: where the file was recorded, and
    /// Mindweaver's for one recorded nowhere.
    pub names: Names,
    pub written: Written,
    /// The hash of what the file holds while it is Mindweaver's, for the
    /// caller to record; none once the render leaves nothing of Mindweaver's
    /// there.
    pub hash: Option<ContentHash>,
}

/// What [`render`] did to one rule's file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    /// It was not there, and now holds the rule: made with `create_new`.
    Created,
    /// It held what was recorded, and was replaced whole by the rule as it
    /// is now.
    Changed,
    /// It already held the rule as it is now, and was left as it was.
    Unchanged,
    /// It held what was recorded, and its rule is no longer asked for, so it
    /// was taken away.
    Removed,
    /// It was recorded, and was not there any more, nor asked for: nothing
    /// is left to record.
    Gone,
    /// It was recorded, and holds something else now: somebody changed it,
    /// so it is left as it is, and its record kept (D-04).
    Edited,
    /// It was not recorded, and something is there: somebody's, left alone,
    /// and the rule is not written.
    Foreign,
    /// It could not be looked at, written or taken away; [`Warning::Failed`]
    /// says why.
    Failed,
}

/// Something a render has to say about a rule's file, which keeps neither
/// it nor any other file from being rendered.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Warning {
    /// The rule is about some files only, so it reaches Claude Code alone:
    /// `missed` are the project's agents that read no `.claude/rules`, in
    /// report order. Never empty.
    #[error("{}", claude_only(.route, .missed))]
    ClaudeOnly { route: String, missed: Vec<Agent> },
    /// Somebody's file is where the rule's would go.
    #[error(
        "{path}: not a file Mindweaver wrote, so rule `{route}` is not written there and \
         does not reach Claude Code"
    )]
    Foreign { route: String, path: PathBuf },
    /// A file Mindweaver wrote has been changed since.
    #[error("{path}: changed since Mindweaver wrote rule `{route}` there, so it is left as it is")]
    Edited { route: String, path: PathBuf },
    /// The file could not be looked at, written or taken away.
    #[error("{path}: rule `{route}`: {error}")]
    Failed {
        route: String,
        path: PathBuf,
        error: String,
    },
}

codes!(Warning {
    Self::ClaudeOnly { .. } => "claude_only_rule",
    Self::Foreign { .. } => "claude_rule_foreign",
    Self::Edited { .. } => "claude_rule_edited",
    Self::Failed { .. } => "claude_rule_failed",
});

/// Render `rules` into `project`'s `.claude/rules/mindweaver`, each `mode`
/// takes as one file holding the rule as written, and take away each file
/// recorded whose rule is no longer asked for.
///
/// `recorded` is what the caller kept of earlier renders
/// ([`Rendered::recorded`]), and decides what is Mindweaver's: a file that
/// holds what was recorded for its route is replaced when its rule has
/// changed, and taken away when its rule is no longer asked for, with every
/// folder that leaves empty up to and including `mindweaver`, each in the
/// folder it was recorded in, a Mindflayer build's `mindflayer` among them.
/// A rule recorded nowhere is written into `mindweaver`, unless something is
/// at its place in either folder, which is somebody's. Anything else
/// at a rule's place is somebody's: a file nothing recorded, or one changed
/// since it was written, is left exactly as it is, and said
/// ([`Written::Foreign`], [`Written::Edited`]). A file that is not there is
/// made with `create_new`, so one somebody made meanwhile is never written
/// over; one that is there is replaced through a temporary and a rename,
/// never followed through a link. A render that changes nothing writes
/// nothing.
///
/// A local project's exclude entries are written first, one for each file
/// left Mindweaver's, so none shows in `git status`, not even for a moment;
/// a block that cannot be edited refuses the render with nothing written,
/// rather than leave rules one machine has in every teammate's way. The
/// entries of files taken away, gone or not made after all are taken out
/// last, and a block that cannot be edited then is said
/// ([`Rendered::still_excluded`]), not raised: the files are gone already.
///
/// A rule that is not a rule, cannot be read or is named no name, two rules
/// of one name among all those handed in, whichever `mode` takes, and a
/// route recorded that is no name are refused before anything is written,
/// as the block of `AGENTS.md` refuses them. So is a
/// rule file a link on its way would put outside the project ([`inside`]),
/// and any file at all where the agents of other projects would read it too
/// ([`agents_md::misplaced`]): above a project the workspace links, at the
/// workspace's root where it links none, or inside one of the agents' own
/// folders, which `homes` names. `workspace` is the one whose links say
/// where those are. A render asked for no rule goes ahead there, since
/// taking rules away from those agents is what the refusal is for.
///
/// A file that cannot be looked at, written or taken away is the one rule's
/// failure, collected ([`Warning::Failed`]) while every other file is
/// rendered: each file stands on its own, and the caller has to be told of
/// each one that was written, to record it.
///
/// Takes no lock: the caller holds the workspace's, as one calling
/// [`crate::agents_md::render`] does, and the lock is not re-entrant.
pub fn render<'a>(
    workspace: &WeaverWorkspace,
    project: &MindProject,
    rules: impl IntoIterator<Item = &'a Artifact>,
    mode: Mode,
    recorded: &[Recorded],
    homes: &AgentHomes,
) -> Result<Rendered, ClaudeRulesError> {
    let root = project.root();
    let mut rules: Vec<&Artifact> = rules.into_iter().collect();
    rules.sort_by(|a, b| report_order(a, b));
    let mut asked: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut scoped = BTreeSet::new();
    let mut names = BTreeSet::new();
    for rule in rules {
        let Declared::Rule(manifest) = rule.declared() else {
            return Err(ClaudeRulesError::NotARule {
                name: rule.qualified_name(),
            });
        };
        // Among every rule handed in, taken or not: in scoped mode a rule
        // about every file goes to the block, which compares only its own
        // sections, so a scoped rule of the same name would reach Claude
        // twice under one route with neither render refusing it.
        if !names.insert(rule.name()) {
            return Err(ClaudeRulesError::Twice {
                route: rule.name().to_string(),
            });
        }
        let taken = match mode {
            Mode::Scoped => manifest.is_scoped(),
            Mode::Fallback => true,
        };
        if !taken {
            continue;
        }
        let route = named(rule.name())?;
        let file = rule.file();
        let bytes = fs::read(&file).map_err(|source| ArtifactError::Read { path: file, source })?;
        if manifest.is_scoped() {
            scoped.insert(route.clone());
        }
        asked.insert(route, bytes);
    }
    // By route, the hash and the folder given last for one given twice.
    let mut owned: BTreeMap<String, (&ContentHash, Names)> = BTreeMap::new();
    for record in recorded {
        owned.insert(named(&record.route)?, (&record.hash, record.names));
    }
    // Where each route's file is: where it was recorded, or the new folder.
    let names_of = |route: &str| {
        owned
            .get(route)
            .map_or(Names::Mindweaver, |(_, names)| *names)
    };

    let mut routes: Vec<&String> = asked.keys().chain(owned.keys()).collect();
    routes.sort();
    routes.dedup();
    let top = root.join(folder());
    let real = crate::paths::canonical(root).map_err(|source| ClaudeRulesError::Read {
        path: root.to_path_buf(),
        source,
    })?;
    for route in &routes {
        inside(&real, &root.join(file_under(names_of(route), route)))?;
    }
    // Only a render asked for a rule is held to where it is: one that only
    // takes files away is how rules written before a project was linked
    // below, or before the one at the workspace's root was unlinked, come
    // out again, and refusing it would keep the very leak the refusal is
    // for.
    if !asked.is_empty() {
        if let Some(misplaced) = agents_md::misplaced(workspace, homes, root, &top) {
            return Err(misplaced.into());
        }
    }
    let plans: Vec<(String, Names, PathBuf, Plan)> = routes
        .into_iter()
        .map(|route| {
            let mut names = names_of(route);
            let mut path = root.join(file_under(names, route));
            let mut found = look(&path);
            // Recorded nowhere, and nothing at its place in the new folder:
            // a file at its place in the old one is somebody's too, as a
            // file Claude would read beside the one this would write.
            if !owned.contains_key(route) && matches!(found, Found::Absent) {
                let older = root.join(file_under(Names::Mindflayer, route));
                if !matches!(look(&older), Found::Absent) {
                    found = Found::Other;
                    path = older;
                    names = Names::Mindflayer;
                }
            }
            let plan = plan(
                asked.get(route).map(Vec::as_slice),
                owned.get(route).map(|(hash, _)| *hash),
                found,
            );
            (route.clone(), names, path, plan)
        })
        .collect();

    // Out of git before anything is written, as an install's copies are.
    let key = install::local(project);
    let kept: Vec<(String, Names)> = plans
        .iter()
        .filter(|(route, _, _, plan)| plan.keeps(owned.contains_key(route)))
        .map(|(route, names, _, _)| (route.clone(), *names))
        .collect();
    let excluded = match key {
        Some(key) if !kept.is_empty() => Some(exclude::add(root, key, &entries(&kept))?),
        _ => None,
    };

    // Every file written before any is taken away, so a rule renamed is
    // never missing from Claude's folder between its old file and its new.
    let missed = missed(project);
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    let mut removals = Vec::new();
    for (route, names, path, plan) in plans {
        if scoped.contains(&route) && !missed.is_empty() {
            warnings.push(Warning::ClaudeOnly {
                route: route.clone(),
                missed: missed.clone(),
            });
        }
        let recorded = owned.get(&route).map(|(hash, _)| (*hash).clone());
        let (written, hash) = match plan {
            Plan::Create(bytes) => match create(&path, &bytes) {
                Ok(()) => (Written::Created, Some(hash::bytes(&bytes))),
                Err(error) => {
                    warnings.push(failed(&route, &path, &error));
                    (Written::Failed, None)
                }
            },
            Plan::Replace(bytes) => match replace_bytes_unresolved(&path, &bytes) {
                Ok(()) => (Written::Changed, Some(hash::bytes(&bytes))),
                Err((_, error)) => {
                    warnings.push(failed(&route, &path, &error));
                    (Written::Failed, recorded)
                }
            },
            Plan::Keep(bytes) => (Written::Unchanged, Some(hash::bytes(&bytes))),
            Plan::Edited => {
                warnings.push(Warning::Edited {
                    route: route.clone(),
                    path: path.clone(),
                });
                (Written::Edited, recorded)
            }
            Plan::Foreign => {
                warnings.push(Warning::Foreign {
                    route: route.clone(),
                    path: path.clone(),
                });
                (Written::Foreign, None)
            }
            Plan::Unlooked(error) => {
                warnings.push(failed(&route, &path, &error));
                (Written::Failed, recorded)
            }
            // Said once it is done, below.
            Plan::Remove => {
                removals.push(files.len());
                (Written::Removed, None)
            }
            Plan::Gone => (Written::Gone, None),
        };
        files.push(RuleFile {
            route,
            path,
            names,
            written,
            hash,
        });
    }
    for index in removals {
        let file = &mut files[index];
        if let Err(error) = fs::remove_file(&file.path) {
            warnings.push(failed(&file.route, &file.path, &error));
            file.written = Written::Failed;
            file.hash = owned.get(&file.route).map(|(hash, _)| (*hash).clone());
        }
    }
    // Pruned after a file gone by itself as after one taken away, so the
    // folders it leaves go the same way, each up to its own folder's top.
    for file in &files {
        if matches!(file.written, Written::Removed | Written::Gone) {
            if let Some(parent) = file.path.parent() {
                prune(&root.join(folder_under(file.names)), parent);
            }
        }
    }
    // Out of the exclude block last, so no file shows in `git status` on its
    // way out: each that was Mindweaver's and is not any more, a file that
    // failed to land included, since a line naming an empty place would keep
    // whatever somebody writes there later out of git without a word. Never
    // somebody's file's, which no entry of this project's was written for.
    let dropped: Vec<(String, Names)> = files
        .iter()
        .filter(|file| {
            file.hash.is_none()
                && (matches!(file.written, Written::Removed | Written::Gone)
                    || kept.iter().any(|(route, _)| *route == file.route))
        })
        .map(|file| (file.route.clone(), file.names))
        .collect();
    let still_excluded = key
        .filter(|_| !dropped.is_empty())
        .and_then(|key| exclude::remove(root, key, &entries(&dropped)).err())
        .map(|error| error.to_string());
    // Back in route order, a removal's failure among them.
    warnings.sort_by(|a, b| a.route().cmp(b.route()));
    Ok(Rendered {
        files,
        warnings,
        excluded,
        still_excluded,
    })
}

impl Warning {
    /// The route of the rule it is about.
    fn route(&self) -> &str {
        match self {
            Warning::ClaudeOnly { route, .. }
            | Warning::Foreign { route, .. }
            | Warning::Edited { route, .. }
            | Warning::Failed { route, .. } => route,
        }
    }
}

/// The agents `project` names that a rule about some files only does not
/// reach, in report order: every agent but those that read
/// [`reads_rule_files`], for a project a `mind.toml` marks, which names
/// none, so any of them may work in it.
fn missed(project: &MindProject) -> Vec<Agent> {
    let named = project
        .settings()
        .map_or(&Agent::ALL[..], |settings| &settings.agents[..]);
    let mut missed: Vec<Agent> = named
        .iter()
        .copied()
        .filter(|agent| !reads_rule_files(*agent))
        .collect();
    missed.sort();
    missed.dedup();
    missed
}

/// Whether `agent` reads a project's `.claude/rules`, and honours `paths`
/// in what it finds there.
///
/// Claude does (docs/agents.md, verified on 2.1.309). Codex and OpenCode do
/// not: their instruction files are the `AGENTS.md` chain, and OpenCode's
/// `instructions` (docs-only, both); were either to read the folder after
/// all, [`Warning::ClaudeOnly`] would be too cautious, never wrong about
/// Claude. One arm an agent with no wildcard, so a new agent does not
/// compile until somebody has said whether it reads them. Not
/// [`Agent::reads`], which says no folder of rules is read: what is
/// written there is listed from the ledger, as its rule's target, so no
/// listing takes these files for somebody's.
fn reads_rule_files(agent: Agent) -> bool {
    match agent {
        Agent::Claude => true,
        Agent::Codex | Agent::OpenCode => false,
    }
}

/// What [`Warning::ClaudeOnly`] says of the rule at `route`, which the
/// agents in `missed` are not given.
fn claude_only(route: &str, missed: &[Agent]) -> String {
    let names: Vec<&str> = missed.iter().map(|agent| agent.name()).collect();
    let (spoken, reads) = match names.split_last() {
        Some((last, [])) => ((*last).to_owned(), "reads"),
        Some((last, rest)) => (format!("{} and {last}", rest.join(", ")), "read"),
        None => (String::new(), "read"),
    };
    format!("rule `{route}` states `paths`, so it reaches claude only: {spoken} {reads} no .claude/rules")
}

/// `route` held to the rules every name is ([`name_issues`]), since it
/// becomes folders and a file: lowercase words joined by single hyphens and
/// slashes never climb out of the folder, nor name one twice.
fn named(route: &str) -> Result<String, ClaudeRulesError> {
    let issues = name_issues(route);
    if issues.is_empty() {
        Ok(route.to_owned())
    } else {
        Err(ClaudeRulesError::Route {
            route: route.to_owned(),
            problem: issue_list(&issues),
        })
    }
}

/// Refuse `file` when a link on its way would put it outside the project
/// whose root is `real`, canonical.
///
/// A repository can hold `.claude` as a link to the user's own `~/.claude`,
/// whose `rules` Claude reads in every project, or `.claude/rules` as one to
/// another repository's: written through, this project's rules would reach
/// projects that are not this one, and a removal would take a file out of
/// somebody else's folder. So every folder from the project down to the
/// file's own is resolved, as far as it is there, and refused when it leads
/// out (`agents_md::linked_inside` refuses an `AGENTS.md` the same way). A
/// link that stays inside the project is followed: where a project keeps its
/// `.claude` is its own business. Its exclude entry still names the route
/// through the link, which git does not follow, so in a local project such a
/// file is not kept out (docs/architecture.md, open questions, as for an
/// install's copy). The file itself is never followed ([`look`]).
fn inside(real: &Path, file: &Path) -> Result<(), ClaudeRulesError> {
    let Some(folder) = file.parent() else {
        return Ok(());
    };
    match install::resolved(folder) {
        Some(target) if !target.starts_with(real) => Err(ClaudeRulesError::LinkedOut {
            folder: folder.to_path_buf(),
            target,
        }),
        _ => Ok(()),
    }
}

/// What a render is to do at one rule's place.
enum Plan {
    /// Nothing is there: write the rule.
    Create(Vec<u8>),
    /// Mindweaver's file is there, holding what was recorded: write the rule
    /// as it is now over it.
    Replace(Vec<u8>),
    /// The rule as it is now is there already.
    Keep(Vec<u8>),
    /// Mindweaver's file is there, and its rule no longer asked for.
    Remove,
    /// The file recorded is not there, and its rule no longer asked for.
    Gone,
    /// What is there was recorded, and has changed since.
    Edited,
    /// What is there was never recorded.
    Foreign,
    /// What is there cannot be looked at.
    Unlooked(io::Error),
}

impl Plan {
    /// Whether the file is Mindweaver's once the plan is carried out, and so
    /// kept out of git in a local project: one it writes or finds current,
    /// and one recorded that it leaves where it is.
    fn keeps(&self, recorded: bool) -> bool {
        match self {
            Plan::Create(_) | Plan::Replace(_) | Plan::Keep(_) | Plan::Edited => true,
            Plan::Unlooked(_) => recorded,
            Plan::Remove | Plan::Gone | Plan::Foreign => false,
        }
    }
}

/// One exclude entry for each route's file, in the folder of the names
/// beside it, as the project spells it.
fn entries(routes: &[(String, Names)]) -> Vec<exclude::Entry> {
    routes
        .iter()
        .map(|(route, names)| exclude::Entry::file(file_under(*names, route)))
        .collect()
}

/// What to do at a rule's place, given the rule as it is now when it is
/// asked for, the hash recorded there when one is, and what is `found`.
fn plan(asked: Option<&[u8]>, recorded: Option<&ContentHash>, found: Found) -> Plan {
    let ours = |bytes: &[u8]| recorded.is_some_and(|hash| hash::bytes(bytes) == *hash);
    match (asked, found) {
        (_, Found::Unreadable(error)) => Plan::Unlooked(error),
        (Some(rule), Found::Absent) => Plan::Create(rule.to_vec()),
        // Holding the rule byte for byte makes a recorded file current, even
        // one somebody edited back into it, but never makes an unrecorded
        // one Mindweaver's: it would then be taken away with its rule.
        (Some(rule), Found::File(held)) if recorded.is_some() && held == rule => Plan::Keep(held),
        (Some(rule), Found::File(held)) if ours(&held) => Plan::Replace(rule.to_vec()),
        (None, Found::File(held)) if ours(&held) => Plan::Remove,
        (None, Found::Absent) => Plan::Gone,
        (_, Found::File(_) | Found::Other) if recorded.is_some() => Plan::Edited,
        (_, Found::File(_) | Found::Other) => Plan::Foreign,
    }
}

/// What is at a rule's place.
enum Found {
    Absent,
    /// A file, not a link to one, and what it holds.
    File(Vec<u8>),
    /// A link, a folder, or anything else Mindweaver never writes there.
    Other,
    Unreadable(io::Error),
}

/// What is at `path`, a link looked at rather than followed: Mindweaver
/// writes none, so one there is somebody's, and what it points at too.
fn look(path: &Path) -> Found {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Found::Absent,
        Err(error) => Found::Unreadable(error),
        Ok(metadata) if metadata.is_file() => match fs::read(path) {
            Ok(bytes) => Found::File(bytes),
            Err(error) => Found::Unreadable(error),
        },
        Ok(_) => Found::Other,
    }
}

/// Write a new rule file whole, with the folders it needs, refusing one that
/// is there by now.
///
/// `create_new` rather than a temporary renamed into place, which would put
/// this file over one somebody made after the look found none. A write that
/// fails takes away what it made, so no half of a rule is left for Claude to
/// read, or to be taken for somebody's later.
pub(crate) fn create(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    if let Err(error) = file.write_all(contents) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

/// Remove the folders a removal leaves empty, from `from` up to and
/// including `top`, the `mindweaver` folder, or a Mindflayer build's
/// `mindflayer`.
///
/// Not past it: `.claude/rules` and `.claude` may be somebody's even when
/// empty, and Claude makes `.claude` itself, so nothing tells whether
/// Mindweaver made them. Below it, a folder only groups routes, and an empty
/// one groups nothing.
///
/// A folder not there is taken as pruned already, so the climb goes on: a
/// recorded file found gone may have gone with its whole folder, and the one
/// above can be empty all the same.
fn prune(top: &Path, from: &Path) {
    let mut current = Some(from);
    while let Some(folder) = current {
        if !folder.starts_with(top) {
            break;
        }
        match fs::remove_dir(folder) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => break,
        }
        if folder == top {
            break;
        }
        current = folder.parent();
    }
}

fn failed(route: &str, path: &Path, error: &io::Error) -> Warning {
    Warning::Failed {
        route: route.to_owned(),
        path: path.to_path_buf(),
        error: error.to_string(),
    }
}

/// Why a project's rules could not be rendered into Claude's rules folder.
#[derive(Debug, Error)]
pub enum ClaudeRulesError {
    #[error("rule `{route}` cannot be named in Claude's rules folder: {problem}")]
    Route { route: String, problem: String },
    #[error("two rules are named `{route}`, and Claude's rules folder holds one file a name")]
    Twice { route: String },
    #[error("`{name}` is not a rule, so it has no file in Claude's rules folder")]
    NotARule { name: String },
    #[error(transparent)]
    Rule(#[from] ArtifactError),
    #[error("{0}, so no rule file was written, since none could be kept out of git")]
    Exclude(#[from] ExcludeError),
    #[error(
        "{folder}: the workspace's own folder, where it links no project, so no rule files \
         are written in its .claude/rules: the agents of every project below would read them"
    )]
    WorkspaceRoot { folder: PathBuf },
    #[error(
        "{folder}: above {}, which the workspace links, so no rule files are written in its \
         .claude/rules: their agents would read them too",
        spelled(.projects)
    )]
    AboveProjects {
        folder: PathBuf,
        projects: Vec<PathBuf>,
    },
    #[error(
        "{file} is inside {home}, which {reader} reads for every project, so no rules are \
         written into it: Mindweaver gives rules to one project at a time"
    )]
    UserScope {
        file: PathBuf,
        home: PathBuf,
        reader: &'static str,
    },
    #[error(
        "{folder}: leads through a link to {target}, outside the project, so no rule file is \
         written or taken away there: what it leads to is not the project's"
    )]
    LinkedOut { folder: PathBuf, target: PathBuf },
    #[error("{path}: cannot be read: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

codes!(ClaudeRulesError {
    Self::Route { .. } => "claude_rules_route_unusable",
    Self::Twice { .. } => "claude_rules_route_twice",
    Self::NotARule { .. } => "claude_rules_not_a_rule",
    Self::WorkspaceRoot { .. } => "claude_rules_workspace_root",
    Self::AboveProjects { .. } => "claude_rules_above_projects",
    Self::UserScope { .. } => "claude_rules_user_scope",
    Self::LinkedOut { .. } => "claude_rules_linked_out",
    Self::Read { .. } => "claude_rules_read_failed",
} carried {
    Self::Rule(error) => error,
    Self::Exclude(error) => error,
});

impl From<Misplaced> for ClaudeRulesError {
    fn from(misplaced: Misplaced) -> Self {
        match misplaced {
            Misplaced::UserScope { file, home, reader } => {
                ClaudeRulesError::UserScope { file, home, reader }
            }
            Misplaced::WorkspaceRoot { folder } => ClaudeRulesError::WorkspaceRoot { folder },
            Misplaced::AboveProjects { folder, projects } => {
                ClaudeRulesError::AboveProjects { folder, projects }
            }
        }
    }
}

#[cfg(test)]
#[path = "claude_rules_test.rs"]
mod tests;
