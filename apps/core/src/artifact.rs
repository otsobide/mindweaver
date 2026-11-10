//! One artifact of any kind: what it declares, where it lives, and whether it
//! is well formed.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::ser::{Serialize, SerializeMap, SerializeStruct, Serializer};
use thiserror::Error;

use crate::agent::Agent;
use crate::code::codes;
use crate::frontmatter::{self, FrontMatterError};
use crate::kind::{Kind, Layout};
use crate::mcp::{InputKind, McpDefinition, McpError};
use crate::rule::{Lenient, RuleManifest};
use crate::skill::{
    Compatibility, SkillManifest, MAX_COMPATIBILITY_LEN, MAX_DESCRIPTION_LEN, MAX_NAME_SEGMENT_LEN,
};
use crate::value::ArtifactName;

/// What an artifact declared about itself.
///
/// This enum, not a field on [`Artifact`], is where the kinds differ, and it
/// *is* the kind, so an artifact cannot carry a discriminant that disagrees
/// with what was parsed, because there is only one. Each kind's payload says
/// what that kind requires: a skill without a description is not a state this
/// type can hold, while a rule's description is as optional as the rest of its
/// front matter.
///
/// `PartialEq` and no `Eq`: an MCP server's definition passes its extras
/// through as the TOML they are, and TOML has floats, which are not equal to
/// themselves when they are not a number. Holding them as text to keep `Eq`
/// was the other way, and would hand a renderer a string to parse again.
#[derive(Debug, Clone, PartialEq)]
pub enum Declared {
    /// A skill's front matter.
    Skill(SkillManifest),
    /// A rule's front matter, which is optional: most rules are a markdown
    /// file and that is all, and declare nothing.
    Rule(RuleManifest),
    /// An MCP server's definition, the whole of its file.
    Mcp(McpDefinition),
}

impl Declared {
    /// The kind this is a declaration for.
    pub fn kind(&self) -> Kind {
        match self {
            Declared::Skill(_) => Kind::Skill,
            Declared::Rule(_) => Kind::Rule,
            Declared::Mcp(_) => Kind::Mcp,
        }
    }
}

/// An artifact on disk.
///
/// The body is not held: it is the bulk of the file and a listing never shows
/// it. What a listing does show (one line saying what the thing is) is
/// captured at load, because the file had to be read anyway.
///
/// Usually one copy, at [`path`](Self::path). A project marked by its
/// settings can hold one skill in several of its agents' folders, an
/// install's copies or a user's own, as well as in the folder it authors
/// in, and discovery lists those as one artifact ([`crate::Catalog`]): what
/// it declares is read from the first copy, the project's own when it has
/// one, and every copy is still [validated](Self::validate) where it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Artifact {
    name: ArtifactName,
    path: PathBuf,
    project: PathBuf,
    declared: Declared,
    summary: Option<String>,
    /// How the copy at `path` stands where it is.
    placement: Placement,
    /// Every other copy, in the order discovery found them.
    others: Vec<ArtifactCopy>,
    /// Whether the copies its project's agents read do not all hold the
    /// same names and bytes.
    differ: bool,
    /// The names of the definitions beside an MCP server's that are its own
    /// in another case, as discovery found them: none for any other kind.
    twins: Vec<ArtifactName>,
}

/// Who reads a copy where it sits, and who put it there.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Placement {
    agents: Vec<Agent>,
    authored: bool,
    installed: bool,
}

impl Placement {
    /// A copy in a folder its holder keeps artifacts in, which no agent is
    /// said to read and nothing says was installed: what every artifact was
    /// before a project named its agents, and what one loaded on its own is
    /// until discovery says otherwise.
    fn authored() -> Self {
        Placement {
            agents: Vec::new(),
            authored: true,
            installed: false,
        }
    }

    fn provenance(&self) -> Provenance {
        if self.installed {
            Provenance::Installed
        } else if self.authored {
            Provenance::Authored
        } else {
            Provenance::Foreign
        }
    }
}

/// One copy of an artifact: where it is, which agents read it there, who put
/// it there, and what is wrong with it there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactCopy {
    path: PathBuf,
    placement: Placement,
    issues: Vec<ValidationIssue>,
}

impl ArtifactCopy {
    /// The directory a skill is, or the file a rule is.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The agents among the project's that read it where it sits, each once,
    /// in report order: none for a project that names no agents, and none in
    /// a folder none of them reads.
    pub fn agents(&self) -> &[Agent] {
        &self.placement.agents
    }

    /// Whether it sits in a folder its holder keeps artifacts in: a folder a
    /// project or a workspace authors in, or a workspace's shelf, rather than
    /// only a folder an agent reads.
    pub fn is_authored(&self) -> bool {
        self.placement.authored
    }

    /// Whether the ledger the copies were checked against records
    /// Mindweaver putting it there; never, when they were checked against
    /// none.
    pub fn is_installed(&self) -> bool {
        self.placement.installed
    }

    /// Who put it there, as far as anything says.
    pub fn provenance(&self) -> Provenance {
        self.placement.provenance()
    }

    /// Everything wrong with it, judged on this copy's own file and folder.
    pub fn issues(&self) -> &[ValidationIssue] {
        &self.issues
    }
}

/// As a listing shows a copy: where it is, who reads it there, who put it
/// there and what is wrong with it there, each by the name its accessor
/// gives it. By hand, since who reads and who put it are held together in
/// a placement of the artifact's own that no front end sees.
impl Serialize for ArtifactCopy {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut copy = serializer.serialize_struct("ArtifactCopy", 4)?;
        copy.serialize_field("path", &self.path.to_string_lossy())?;
        copy.serialize_field("agents", self.agents())?;
        copy.serialize_field("provenance", &self.provenance())?;
        copy.serialize_field("issues", self.issues())?;
        copy.end()
    }
}

/// Who put a copy where it is, as far as its folder and the ledger say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// The ledger records Mindweaver putting it there.
    Installed,
    /// In a folder its holder keeps artifacts in, and recorded by nothing:
    /// the holder's own work. A workspace's own artifacts and what it
    /// gathered onto its shelf read the same way, though nothing asks it of
    /// them.
    Authored,
    /// In an agent's folder, outside any its holder authors in, and recorded
    /// by nothing: somebody's, put there some other way, and never
    /// Mindweaver's to change.
    Foreign,
}

impl Provenance {
    /// How a report says it: one lowercase word, which once printed is a
    /// contract.
    pub const fn slug(self) -> &'static str {
        match self {
            Provenance::Installed => "installed",
            Provenance::Authored => "authored",
            Provenance::Foreign => "foreign",
        }
    }
}

/// By its slug, for the reason a kind is: a renamed variant cannot change
/// what a front end compares.
impl Serialize for Provenance {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.slug())
    }
}

/// As a listing shows it: what it is, where its first copy is and in which
/// project, who put it there and who reads it, what is wrong with it, and
/// every copy with what is wrong with each.
///
/// By hand rather than derived: an artifact holds what it declared, the
/// whole of its front matter, which is `show`'s to print rather than a
/// listing's, and how each copy stands in a placement no front end sees.
/// Each field is named for the accessor that gives it, and says what that
/// accessor says: `issues` is [`validate`](Artifact::validate), every copy's
/// together, and `copies` is [`copies`](Artifact::copies), the first first.
impl Serialize for Artifact {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut artifact = serializer.serialize_struct("Artifact", 10)?;
        artifact.serialize_field("kind", &self.kind())?;
        artifact.serialize_field("name", &self.name)?;
        artifact.serialize_field("summary", &self.summary)?;
        artifact.serialize_field("path", &self.path.to_string_lossy())?;
        artifact.serialize_field("project", &self.project.to_string_lossy())?;
        artifact.serialize_field("provenance", &self.provenance())?;
        artifact.serialize_field("agents", &self.agents())?;
        artifact.serialize_field("issues", &self.validate())?;
        artifact.serialize_field("copies_differ", &self.differ)?;
        artifact.serialize_field("copies", &self.copies())?;
        artifact.end()
    }
}

impl Artifact {
    /// Load a skill from its directory.
    pub fn skill(
        directory: impl Into<PathBuf>,
        project: impl Into<PathBuf>,
    ) -> Result<Self, ArtifactError> {
        let directory = directory.into();
        let source = read(&directory.join(manifest_of(Kind::Skill)))?;
        Self::skill_holding(directory, project, &source)
    }

    /// A skill in `directory` whose manifest holds `source`, read as
    /// [`skill`](Self::skill) reads one from the disk: what it would be
    /// once written there, for a front end to be told what `validate` would
    /// say of it before it is ([`crate::draft`]).
    pub(crate) fn skill_holding(
        directory: impl Into<PathBuf>,
        project: impl Into<PathBuf>,
        source: &str,
    ) -> Result<Self, ArtifactError> {
        let directory = directory.into();
        let path = directory.join(manifest_of(Kind::Skill));
        let document = frontmatter::split(source).map_err(|source| ArtifactError::FrontMatter {
            path: path.clone(),
            source,
        })?;
        let manifest =
            SkillManifest::parse(document.front_matter).map_err(|source| ArtifactError::Parse {
                path: path.clone(),
                source,
            })?;

        // A skill says what it is, so nothing is derived: the summary is the
        // first line of the description its author wrote.
        let summary = first_line(&manifest.description);
        Ok(Self {
            name: ArtifactName::from(&manifest.name),
            path: directory,
            project: project.into(),
            declared: Declared::Skill(manifest),
            summary,
            placement: Placement::authored(),
            others: Vec::new(),
            differ: false,
            twins: Vec::new(),
        })
    }

    /// Load a rule from its file, named by its route under the rules folder.
    ///
    /// The name is passed in rather than taken from the path because only the
    /// caller knows which folder the route is relative to.
    pub fn rule(
        file: impl Into<PathBuf>,
        name: String,
        project: impl Into<PathBuf>,
    ) -> Result<Self, ArtifactError> {
        let path = file.into();
        let source = read(&path)?;
        Ok(Self::rule_holding(path, name, project, &source))
    }

    /// A rule at `file` holding `source`, read as [`rule`](Self::rule) reads
    /// one from the disk, as [`skill_holding`](Self::skill_holding) reads a
    /// skill.
    pub(crate) fn rule_holding(
        file: impl Into<PathBuf>,
        name: String,
        project: impl Into<PathBuf>,
        source: &str,
    ) -> Self {
        let path = file.into();
        // Reading it never fails, as a skill's can: a rule's front matter is
        // optional, so whatever cannot be read as front matter is content.
        let (manifest, body) = RuleManifest::read(source);
        // The opening line of the body, not of the file, and not a declared
        // description: the body is what an agent reads once the front matter
        // is taken off, so its opening line is what the agent meets first.
        Self {
            name: ArtifactName::from(name),
            path,
            project: project.into(),
            declared: Declared::Rule(manifest),
            summary: opening_line(body),
            placement: Placement::authored(),
            others: Vec::new(),
            differ: false,
            twins: Vec::new(),
        }
    }

    /// Load an MCP server's definition from its file, named by the file's
    /// stem, which the caller knows, as it knows a rule's route.
    pub fn mcp(
        file: impl Into<PathBuf>,
        name: String,
        project: impl Into<PathBuf>,
    ) -> Result<Self, ArtifactError> {
        let path = file.into();
        let source = read(&path)?;
        Self::mcp_holding(path, name, project, &source)
    }

    /// A definition at `file` holding `source`, read as [`mcp`](Self::mcp)
    /// reads one from the disk, as [`skill_holding`](Self::skill_holding)
    /// reads a skill.
    ///
    /// Named by its file, as a rule is by its route, rather than by the
    /// `name` it declares: the file is what a change finds it by, so one
    /// whose two disagree is still the one its file names, and `validate`
    /// says they disagree.
    pub(crate) fn mcp_holding(
        file: impl Into<PathBuf>,
        name: String,
        project: impl Into<PathBuf>,
        source: &str,
    ) -> Result<Self, ArtifactError> {
        let path = file.into();
        let definition =
            McpDefinition::parse(source).map_err(|source| ArtifactError::Definition {
                path: path.clone(),
                source: Box::new(source),
            })?;
        // A definition says what it is, as a skill does: the summary is the
        // first line of its description.
        let summary = first_line(&definition.description);
        Ok(Self {
            name: ArtifactName::from(name),
            path,
            project: project.into(),
            declared: Declared::Mcp(definition),
            summary,
            placement: Placement::authored(),
            others: Vec::new(),
            differ: false,
            twins: Vec::new(),
        })
    }

    /// What kind of artifact this is.
    pub fn kind(&self) -> Kind {
        self.declared.kind()
    }

    /// The name it is referred to by.
    ///
    /// Declared, for a skill. For a rule, its route under the rules folder
    /// without the extension, so `git/no-force-push` is a name and the folders
    /// above it are grouping and nothing more.
    pub fn name(&self) -> &ArtifactName {
        &self.name
    }

    /// `skill:commit-style`: the name qualified by its kind.
    pub fn qualified_name(&self) -> String {
        format!(
            "{}{}{}",
            self.kind().slug(),
            crate::catalog::QUALIFIER,
            self.name
        )
    }

    /// The directory a skill is, or the file a rule or a definition is: its
    /// first copy's, when it has several.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every copy, the one at [`path`](Self::path) first, each with what is
    /// wrong with it where it is.
    pub fn copies(&self) -> Vec<ArtifactCopy> {
        let first = ArtifactCopy {
            path: self.path.clone(),
            placement: self.placement.clone(),
            issues: self.issues(),
        };
        std::iter::once(first)
            .chain(self.others.iter().cloned())
            .collect()
    }

    /// Every agent among the project's that reads some copy of it, each once,
    /// in report order.
    pub fn agents(&self) -> Vec<Agent> {
        let mut agents: Vec<Agent> = self
            .placements()
            .flat_map(|placement| placement.agents.iter().copied())
            .collect();
        agents.sort();
        agents.dedup();
        agents
    }

    /// Who put it there, over every copy: installed when the ledger records
    /// every one, and otherwise what the copies it does not record say, since
    /// those are somebody's: authored when one of them sits in a folder its
    /// holder authors in, and foreign when none does.
    pub fn provenance(&self) -> Provenance {
        let unrecorded: Vec<&Placement> = self
            .placements()
            .filter(|placement| !placement.installed)
            .collect();
        if unrecorded.is_empty() {
            Provenance::Installed
        } else if unrecorded.iter().any(|placement| placement.authored) {
            Provenance::Authored
        } else {
            Provenance::Foreign
        }
    }

    /// Whether the copies its project's agents read do not all hold the
    /// same names and bytes: then the agents reading them do not all load
    /// the same thing. Claude and Codex each read one of the standard
    /// folders, and OpenCode, reading both, loads whichever it takes.
    /// Discovery says so as a failure too. A copy no agent reads, the
    /// project's own source, is compared with none.
    pub fn copies_differ(&self) -> bool {
        self.differ
    }

    /// How each copy stands, the first first.
    fn placements(&self) -> impl Iterator<Item = &Placement> {
        std::iter::once(&self.placement).chain(self.others.iter().map(|copy| &copy.placement))
    }

    /// Say who reads the copy at `path` and whether it sits in a folder its
    /// holder authors in: discovery's to say, once it has loaded the copy
    /// from a folder it knows that of.
    pub(crate) fn placed(&mut self, agents: Vec<Agent>, authored: bool) {
        self.placement.agents = agents;
        self.placement.authored = authored;
    }

    /// Whether the copy at [`path`](Self::path) sits in a folder its holder
    /// authors in: what discovery puts first among copies of one name.
    pub(crate) fn first_is_authored(&self) -> bool {
        self.placement.authored
    }

    /// Whether one of the project's agents reads the copy at
    /// [`path`](Self::path): what discovery compares with the others.
    pub(crate) fn first_is_read(&self) -> bool {
        !self.placement.agents.is_empty()
    }

    /// Take `other`, loaded under the same name from another place, as one
    /// more copy of this one, judged as it is there; `differs` when it does
    /// not hold what the first copy an agent reads holds.
    pub(crate) fn take_copy(&mut self, other: Artifact, differs: bool) {
        let issues = other.issues();
        self.others.push(ArtifactCopy {
            path: other.path,
            placement: other.placement,
            issues,
        });
        self.others.extend(other.others);
        self.differ |= differs || other.differ;
    }

    /// Say which definitions beside this one are named as it is in another
    /// case: discovery's to say, once it has loaded every one in a folder.
    pub(crate) fn twinned(&mut self, twins: Vec<ArtifactName>) {
        self.twins = twins;
    }

    /// Mark as installed each copy `recorded` says Mindweaver put where it
    /// is, and every other as not.
    pub(crate) fn record(&mut self, recorded: impl Fn(&Path) -> bool) {
        self.placement.installed = recorded(&self.path);
        for copy in &mut self.others {
            copy.placement.installed = recorded(&copy.path);
        }
    }

    /// The root of the mind project this was found in.
    pub fn project(&self) -> &Path {
        &self.project
    }

    /// The name of that project.
    pub fn project_name(&self) -> Option<&str> {
        self.project.file_name()?.to_str()
    }

    /// What this artifact declared.
    pub fn declared(&self) -> &Declared {
        &self.declared
    }

    /// One line saying what this is: declared by a skill, derived from the
    /// opening line of its body for a rule, and absent when there is nothing
    /// to show.
    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }

    /// The description, when this declares one: always for a skill and a
    /// definition, and for a rule only when its front matter says.
    pub fn description(&self) -> Option<&str> {
        match &self.declared {
            Declared::Skill(manifest) => Some(&manifest.description),
            Declared::Rule(manifest) => manifest.description.stated().map(String::as_str),
            Declared::Mcp(definition) => Some(&definition.description),
        }
    }

    /// The skill front matter, when this is a skill.
    pub fn manifest(&self) -> Option<&SkillManifest> {
        match &self.declared {
            Declared::Skill(manifest) => Some(manifest),
            Declared::Rule(_) | Declared::Mcp(_) => None,
        }
    }

    /// The MCP server's definition, when this is one.
    pub fn definition(&self) -> Option<&McpDefinition> {
        match &self.declared {
            Declared::Mcp(definition) => Some(definition),
            Declared::Skill(_) | Declared::Rule(_) => None,
        }
    }

    /// The markdown an agent reads: the body after the front matter, or the
    /// whole file when there is none. For a definition, which no agent reads
    /// as it is, the whole of its file.
    ///
    /// What a renderer puts in front of an agent, so a rule's front matter is
    /// never pasted into a file that has no use for it, and what `show` prints
    /// beneath what the front matter declares.
    pub fn contents(&self) -> Result<String, ArtifactError> {
        let path = self.file();
        let source = read(&path)?;
        match self.declared {
            Declared::Skill(_) => {
                let document =
                    frontmatter::split(&source).map_err(|source| ArtifactError::FrontMatter {
                        path: path.clone(),
                        source,
                    })?;
                Ok(document.body.to_owned())
            }
            // Decided again from the file as it is now, by the same reading
            // that loaded it.
            Declared::Rule(_) => Ok(RuleManifest::read(&source).1.to_owned()),
            // What it says is the transport and its inputs, which the file
            // holds whole: there is no body apart from what it declares.
            Declared::Mcp(_) => Ok(source),
        }
    }

    /// The file this artifact was read from.
    pub fn file(&self) -> PathBuf {
        match self.kind().layout() {
            Layout::Directory { manifest } => self.path.join(manifest),
            Layout::Files { .. } | Layout::Flat { .. } => self.path.clone(),
        }
    }

    /// Everything wrong with this artifact, in the order it is worth fixing.
    ///
    /// Pure: every fact it judges was captured when the file was read, so
    /// checking cannot fail on its own and a caller never has to handle an
    /// error from asking whether something is valid.
    ///
    /// Every copy is judged on its own file and folder, and what one of the
    /// others has that the first does not is added after the first's: an
    /// agent loads the copy in the folder it reads, so a copy listed under
    /// one name is no reason to check only one. [`copies`](Self::copies)
    /// says which copy has what.
    pub fn validate(&self) -> Vec<ValidationIssue> {
        let mut issues = self.issues();
        for issue in self.others.iter().flat_map(|copy| &copy.issues) {
            if !issues.contains(issue) {
                issues.push(issue.clone());
            }
        }
        issues
    }

    /// What [`validate`](Self::validate) would say of this skill once it is
    /// filed in a folder called `folder` rather than the one it was read
    /// from: for one gathered from the root of a source, read from the
    /// clone's folder and filed under the name it declares
    /// ([`crate::gather`]). Any other kind is judged as it is.
    pub(crate) fn validate_filed_as(&self, folder: &str) -> Vec<ValidationIssue> {
        match &self.declared {
            Declared::Skill(manifest) if self.others.is_empty() => {
                skill_issues(manifest, Some(folder))
            }
            Declared::Skill(_) | Declared::Rule(_) | Declared::Mcp(_) => self.validate(),
        }
    }

    /// Everything wrong with the copy at `path`.
    fn issues(&self) -> Vec<ValidationIssue> {
        match &self.declared {
            Declared::Skill(manifest) => skill_issues(
                manifest,
                self.path.file_name().and_then(|name| name.to_str()),
            ),
            Declared::Rule(manifest) => {
                // Every kind is looked up by name, so every kind is checked
                // for one.
                let mut issues = name_issues(&self.name);
                // A rule's front matter is optional, so the one thing that
                // makes it unusable is having nothing to say. `summary` is
                // None only when no line of the body held any text.
                if self.summary.is_none() {
                    issues.push(ValidationIssue::Empty);
                }
                issues.extend(rule_issues(manifest));
                issues
            }
            Declared::Mcp(definition) => mcp_issues(
                definition,
                &self.name,
                self.path.file_name().and_then(|name| name.to_str()),
                &self.twins,
            ),
        }
    }
}

/// Everything wrong with a definition filed under `name`, in a file called
/// `file` when that file has a name to compare, beside definitions named
/// `twins`, each its name in another case.
///
/// Its name first, held to the keys every agent takes and to what each
/// keeps for itself ([`mcp_name_issues`]), then the file it is in, then the
/// definition itself ([`McpDefinition::issues`]). Apart from
/// [`Artifact::validate`], as [`skill_issues`] is, so that contents given
/// whole are judged before they are written ([`mcp_strict_issues`]).
fn mcp_issues(
    definition: &McpDefinition,
    name: &str,
    file: Option<&str>,
    twins: &[ArtifactName],
) -> Vec<ValidationIssue> {
    let mut issues = mcp_name_issues(name, twins);
    // The file is what an install keys the entry by and what a change finds
    // the definition by, so a `name` that says otherwise is one of two names
    // for one server.
    if definition.name != name {
        issues.push(ValidationIssue::NameFileMismatch {
            name: definition.name.to_string(),
            file: file.unwrap_or(name).to_owned(),
        });
    }
    issues.extend(definition.issues());
    issues
}

/// Everything wrong with `name` as an MCP server's: not the kebab-case
/// every other kind's name is held to, but the keys every agent's config
/// takes, less those an agent keeps for itself ([`crate::mcp::name`]), and
/// one name with each of `twins`, which differ from it only by case.
fn mcp_name_issues(name: &str, twins: &[ArtifactName]) -> Vec<ValidationIssue> {
    let mut issues = crate::mcp::name::issues(name);
    issues.extend(twins.iter().map(|twin| ValidationIssue::McpNameCaseClash {
        name: name.to_owned(),
        twin: twin.to_string(),
    }));
    issues
}

/// Whatever `name` breaks as an MCP server's that Mindweaver will not
/// write: what [`mcp_strict_issues`] says of a definition's name, asked of
/// a name before there is a definition to judge it with, as a rename asks
/// of the name it is given, beside the definitions named `twins`.
pub(crate) fn mcp_name_strict_issues(name: &str, twins: &[ArtifactName]) -> Vec<ValidationIssue> {
    mcp_name_issues(name, twins)
        .into_iter()
        .filter(|issue| !issue.is_warning())
        .collect()
}

/// Whatever a definition filed under `name`, in a file called `file`
/// beside definitions named `twins`, breaks that Mindweaver will not
/// write: every issue `validate` would call it invalid for, which is all
/// it says but a name an agent only uses for its own
/// ([`ValidationIssue::McpNameClaimed`]), a warning.
///
/// A definition's half of [`strict_issues`], judged on what it will hold
/// before a byte of it is written, as a rule's is ([`rule_text_issues`]):
/// contents given whole, and a definition renamed. `twins` are read off the
/// folder it is written in by the change, as discovery reads them off the
/// folder it lists, so that the two hold it to one rule.
pub(crate) fn mcp_strict_issues(
    definition: &McpDefinition,
    name: &str,
    file: &str,
    twins: &[ArtifactName],
) -> Vec<ValidationIssue> {
    mcp_issues(definition, name, Some(file), twins)
        .into_iter()
        .filter(|issue| !issue.is_warning())
        .collect()
}

/// Everything wrong with a skill that declares `manifest`, filed in a folder
/// called `folder` when that folder has a name to compare.
///
/// Apart from [`Artifact::validate`] so that a skill can be judged where it is
/// going rather than where it is: [`strict_issues`].
fn skill_issues(manifest: &SkillManifest, folder: Option<&str>) -> Vec<ValidationIssue> {
    // Every kind is looked up by name, so every kind is checked for one.
    let mut issues = name_issues(&manifest.name);

    // The directory name is how an agent locates the skill it was told to
    // invoke, so a mismatch means one that lists fine and never loads.
    if let Some(folder) = folder {
        if folder != manifest.name {
            issues.push(ValidationIssue::NameDirectoryMismatch {
                name: manifest.name.clone(),
                directory: folder.to_owned(),
            });
        }
    }
    let description = manifest.description.trim();
    let length = description.chars().count();
    if description.is_empty() {
        issues.push(ValidationIssue::DescriptionEmpty);
    } else if length > MAX_DESCRIPTION_LEN {
        issues.push(ValidationIssue::DescriptionTooLong { length });
    }
    // Optional, but once written it has to say something and fit: an agent
    // may show it to the model when the skill is loaded.
    match &manifest.compatibility {
        Compatibility::Unstated => {}
        Compatibility::Stated(compatibility) => {
            let compatibility = compatibility.trim();
            let length = compatibility.chars().count();
            if compatibility.is_empty() {
                issues.push(ValidationIssue::CompatibilityEmpty);
            } else if length > MAX_COMPATIBILITY_LEN {
                issues.push(ValidationIssue::CompatibilityTooLong { length });
            }
        }
        Compatibility::NotAString => issues.push(ValidationIssue::CompatibilityNotAString),
    }
    // Read leniently, so the skill still lists; said here, so the entries an
    // agent expecting strings would trip on are known.
    if manifest.metadata.not_a_map {
        issues.push(ValidationIssue::MetadataNotAMap);
    }
    issues.extend(
        manifest
            .metadata
            .ignored
            .iter()
            .map(|key| ValidationIssue::MetadataNotAString { key: key.clone() }),
    );
    issues.extend(
        manifest
            .metadata
            .repeated
            .iter()
            .map(|key| ValidationIssue::MetadataRepeated { key: key.clone() }),
    );
    issues
}

/// What a rule's front matter left unread, each only worth a warning: what is
/// not read is still in the file, and the rule still says what it says.
///
/// And each refused when Mindweaver writes a rule from a template, which is
/// the time to find out that its front matter would not be read.
pub(crate) fn rule_issues(manifest: &RuleManifest) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if let Some(reason) = &manifest.unparsed {
        issues.push(ValidationIssue::FrontMatterUnparsed {
            reason: reason.clone(),
        });
    }
    if manifest.paths == Lenient::Ignored {
        issues.push(ValidationIssue::PathsNotStrings);
    }
    if manifest.description == Lenient::Ignored {
        issues.push(ValidationIssue::DescriptionNotAString);
    }
    issues
}

/// Whichever strict rules ([`ValidationIssue::is_strict`]) a skill declaring
/// `manifest` breaks once it is filed in a folder called `folder`.
///
/// Asked of the skill as it is about to be written, rather than as it sits:
/// the folder it is filed in is the one Mindweaver is about to name, which is
/// not always the one it was read from.
pub(crate) fn strict_issues(manifest: &SkillManifest, folder: &str) -> Vec<ValidationIssue> {
    skill_issues(manifest, Some(folder))
        .into_iter()
        .filter(ValidationIssue::is_strict)
        .collect()
}

/// Whatever a rule reading `source` breaks that Mindweaver will not write:
/// having nothing to say, which `validate` calls invalid, and whatever its
/// front matter leaves unread ([`rule_issues`]), which a template is refused
/// for.
///
/// A rule's half of [`strict_issues`], judged on the text it will hold
/// before a byte of it is written, as contents given whole are
/// ([`crate::lifecycle::replace_contents`]). Not its name, which is its
/// route rather than anything it says, and which the text cannot change.
pub(crate) fn rule_text_issues(source: &str) -> Vec<ValidationIssue> {
    let (manifest, body) = RuleManifest::read(source);
    let mut issues = Vec::new();
    // `opening_line`, as a rule's summary is found when it is read, so a
    // rule written is one that reads as saying something.
    if opening_line(body).is_none() {
        issues.push(ValidationIssue::Empty);
    }
    issues.extend(rule_issues(&manifest));
    issues
}

/// Read a file, naming it if that fails.
fn read(path: &Path) -> Result<String, ArtifactError> {
    fs::read_to_string(path).map_err(|source| ArtifactError::Read {
        path: path.to_path_buf(),
        source,
    })
}

/// The manifest file a directory-shaped kind holds.
pub(crate) fn manifest_of(kind: Kind) -> &'static str {
    match kind.layout() {
        Layout::Directory { manifest } => manifest,
        Layout::Files { .. } | Layout::Flat { .. } => {
            unreachable!("{kind} is not a directory-shaped kind")
        }
    }
}

/// The first line of a declared description that carries any text.
///
/// Not literally the first line: a description written as a YAML block scalar
/// often opens with a blank one, and listing that skill with no summary at all
/// would be a formatting accident showing through.
fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

/// The first line of a document that carries any text.
///
/// Leading `#` are stripped, so a document that opens with a heading is
/// summarised by that heading rather than by a row of hashes, which is what
/// most markdown files opening with a title actually want.
fn opening_line(source: &str) -> Option<String> {
    source
        .lines()
        .map(|line| line.trim_start().trim_start_matches('#').trim())
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

/// Everything wrong with a name, whatever kind it names.
///
/// Shared by [`Artifact::validate`], which judges a name found on disk, and
/// [`crate::create`], which refuses to write one: a name this tool creates has
/// to pass the check this tool applies, and one rule cannot drift from itself.
pub(crate) fn name_issues(name: &str) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if name.is_empty() {
        issues.push(ValidationIssue::NameEmpty);
        return issues;
    }
    for segment in name.split('/') {
        if !is_kebab_case(segment) {
            issues.push(ValidationIssue::NameNotKebabCase {
                segment: segment.to_owned(),
            });
        }
        // Characters, not bytes: the limit exists to keep a name readable,
        // and counting bytes would reject a name in a script that spends more
        // than one per character at half the length.
        let length = segment.chars().count();
        if length > MAX_NAME_SEGMENT_LEN {
            issues.push(ValidationIssue::NameSegmentTooLong {
                segment: segment.to_owned(),
                length,
            });
        }
    }
    issues
}

/// Every problem with a name, as one sentence.
pub(crate) fn issue_list(issues: &[ValidationIssue]) -> String {
    issues
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

/// Lowercase letters and digits, in words joined by single hyphens:
/// `^[a-z0-9]+(-[a-z0-9]+)*$`, the Agent Skills rule for a skill's name: what
/// an agent can be asked to invoke without quoting or case folding.
///
/// Split on the hyphen rather than checked character by character, because
/// that is what makes a leading, trailing or doubled hyphen an empty word, and
/// so wrong, without a rule of its own for each.
fn is_kebab_case(segment: &str) -> bool {
    segment.split('-').all(|word| {
        !word.is_empty()
            && word
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    })
}

/// Something that stops an artifact from being usable, or will.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationIssue {
    NameEmpty,
    NameNotKebabCase {
        segment: String,
    },
    NameSegmentTooLong {
        segment: String,
        length: usize,
    },
    NameDirectoryMismatch {
        name: String,
        directory: String,
    },
    NameFileMismatch {
        name: String,
        file: String,
    },
    DescriptionEmpty,
    DescriptionTooLong {
        length: usize,
    },
    CompatibilityEmpty,
    CompatibilityTooLong {
        length: usize,
    },
    CompatibilityNotAString,
    MetadataNotAMap,
    MetadataNotAString {
        key: String,
    },
    MetadataRepeated {
        key: String,
    },
    Empty,
    FrontMatterUnparsed {
        reason: String,
    },
    PathsNotStrings,
    DescriptionNotAString,
    TransportIncomplete {
        transport: String,
        field: String,
    },
    InputRepeated {
        name: String,
    },
    InputUnreferenced {
        name: String,
        kind: InputKind,
    },
    InputInUserinfo {
        name: String,
    },
    InputNameMalformed {
        name: String,
    },
    EnvVarMalformed {
        name: String,
        variable: String,
    },
    SecretWithoutEnvVar {
        name: String,
    },
    SecretLiteral {
        at: String,
        prefix: String,
    },
    McpNameNotAKey {
        name: String,
    },
    McpNameReserved {
        name: String,
        agent: Agent,
        reserved: String,
    },
    McpNameClaimed {
        name: String,
        agent: Agent,
        claimed: String,
    },
    McpNameCaseClash {
        name: String,
        twin: String,
    },
}

codes!(ValidationIssue {
    Self::NameEmpty => "name_empty",
    Self::NameNotKebabCase { .. } => "name_not_kebab_case",
    Self::NameSegmentTooLong { .. } => "name_segment_too_long",
    Self::NameDirectoryMismatch { .. } => "name_directory_mismatch",
    Self::NameFileMismatch { .. } => "name_file_mismatch",
    Self::DescriptionEmpty => "description_empty",
    Self::DescriptionTooLong { .. } => "description_too_long",
    Self::CompatibilityEmpty => "compatibility_empty",
    Self::CompatibilityTooLong { .. } => "compatibility_too_long",
    Self::CompatibilityNotAString => "compatibility_not_a_string",
    Self::MetadataNotAMap => "metadata_not_a_map",
    Self::MetadataNotAString { .. } => "metadata_not_a_string",
    Self::MetadataRepeated { .. } => "metadata_repeated",
    Self::Empty => "content_empty",
    Self::FrontMatterUnparsed { .. } => "front_matter_unparsed",
    Self::PathsNotStrings => "paths_not_strings",
    Self::DescriptionNotAString => "description_not_a_string",
    Self::TransportIncomplete { .. } => "transport_incomplete",
    Self::InputRepeated { .. } => "input_repeated",
    Self::InputUnreferenced { .. } => "input_unreferenced",
    Self::InputInUserinfo { .. } => "input_in_userinfo",
    Self::InputNameMalformed { .. } => "input_name_malformed",
    Self::EnvVarMalformed { .. } => "env_var_malformed",
    Self::SecretWithoutEnvVar { .. } => "secret_without_env_var",
    Self::SecretLiteral { .. } => "secret_literal",
    Self::McpNameNotAKey { .. } => "mcp_name_not_a_key",
    Self::McpNameReserved { .. } => "mcp_name_reserved",
    Self::McpNameClaimed { .. } => "mcp_name_claimed",
    Self::McpNameCaseClash { .. } => "mcp_name_case_clash",
});

impl ValidationIssue {
    /// Whether this breaks one of the strict rules: the ones Mindweaver will
    /// not break when it writes a skill into a project (creating, renaming
    /// or installing one) rather than only report.
    ///
    /// The name, the folder it is filed in, and the description, because
    /// those are what agents refuse or trip on: Codex skips a skill whose name
    /// is over 64 characters or whose description is empty, and an agent finds
    /// a skill by its folder or by its name, so one whose two disagree is
    /// found by one and not the other. Not `compatibility` and `metadata`,
    /// which an agent reads past.
    pub fn is_strict(&self) -> bool {
        matches!(
            self,
            ValidationIssue::NameEmpty
                | ValidationIssue::NameNotKebabCase { .. }
                | ValidationIssue::NameSegmentTooLong { .. }
                | ValidationIssue::NameDirectoryMismatch { .. }
                | ValidationIssue::DescriptionEmpty
                | ValidationIssue::DescriptionTooLong { .. }
        )
    }

    /// Whether this is only worth a warning: something an agent reads past,
    /// so the artifact still works, and `validate` says so without calling it
    /// invalid.
    ///
    /// `compatibility` and `metadata`, the same ones `is_strict` lets through:
    /// were they invalid, a skill installed with nested metadata would fail
    /// its project's `validate` from then on, and the only way to clear it
    /// would be to edit the copy, which stops Mindweaver from updating it.
    /// And whatever a rule's front matter leaves unread, since a rule needs
    /// none to be read: front matter that does not parse is content, and a key
    /// written in another shape is ignored. Matched in full rather than with
    /// `matches!`, so a new issue has to say which it is.
    pub fn is_warning(&self) -> bool {
        match self {
            ValidationIssue::CompatibilityEmpty
            | ValidationIssue::CompatibilityTooLong { .. }
            | ValidationIssue::CompatibilityNotAString
            | ValidationIssue::MetadataNotAMap
            | ValidationIssue::MetadataNotAString { .. }
            | ValidationIssue::MetadataRepeated { .. }
            | ValidationIssue::FrontMatterUnparsed { .. }
            | ValidationIssue::PathsNotStrings
            | ValidationIssue::DescriptionNotAString
            | ValidationIssue::McpNameClaimed { .. } => true,
            ValidationIssue::NameEmpty
            | ValidationIssue::NameNotKebabCase { .. }
            | ValidationIssue::NameSegmentTooLong { .. }
            | ValidationIssue::NameDirectoryMismatch { .. }
            | ValidationIssue::NameFileMismatch { .. }
            | ValidationIssue::DescriptionEmpty
            | ValidationIssue::DescriptionTooLong { .. }
            | ValidationIssue::Empty
            | ValidationIssue::TransportIncomplete { .. }
            | ValidationIssue::InputRepeated { .. }
            | ValidationIssue::InputUnreferenced { .. }
            | ValidationIssue::InputInUserinfo { .. }
            | ValidationIssue::InputNameMalformed { .. }
            | ValidationIssue::EnvVarMalformed { .. }
            | ValidationIssue::SecretWithoutEnvVar { .. }
            | ValidationIssue::SecretLiteral { .. }
            | ValidationIssue::McpNameNotAKey { .. }
            | ValidationIssue::McpNameReserved { .. }
            | ValidationIssue::McpNameCaseClash { .. } => false,
        }
    }
}

/// As an object: its code, what it is about by the name its variant gives
/// each, whether it is only worth a warning, and its message.
///
/// By hand rather than derived, for the reason a kind is: a derive tags each
/// variant with its Rust name, which a rename would change, where the code
/// is the word a front end compares and is held to stay. `warning` is
/// [`is_warning`](Self::is_warning), core's to say, so the panel tells an
/// artifact that does not work from one that only says something odd
/// without a list of its own; the message is there for the same reason, to
/// be shown rather than written again in TypeScript.
impl Serialize for ValidationIssue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("code", self.code())?;
        for (name, carried) in self.carried() {
            match carried {
                Carried::Text(text) => map.serialize_entry(name, text)?,
                Carried::Count(count) => map.serialize_entry(name, &count)?,
            }
        }
        map.serialize_entry("warning", &self.is_warning())?;
        map.serialize_entry("message", &self.to_string())?;
        map.end()
    }
}

/// One thing an issue carries beside its code, as its JSON holds it.
enum Carried<'a> {
    Text(&'a str),
    Count(usize),
}

impl ValidationIssue {
    /// What it carries beside its code, each by the name its variant gives
    /// it, in the order written: what its JSON holds between its `code` and
    /// its `warning`, and what its TypeScript says it holds, read off this
    /// one match so the two cannot differ.
    fn carried(&self) -> Vec<(&'static str, Carried<'_>)> {
        // Matched in full, so a new issue has to say what it carries.
        match self {
            ValidationIssue::NameEmpty
            | ValidationIssue::DescriptionEmpty
            | ValidationIssue::CompatibilityEmpty
            | ValidationIssue::CompatibilityNotAString
            | ValidationIssue::MetadataNotAMap
            | ValidationIssue::Empty
            | ValidationIssue::PathsNotStrings
            | ValidationIssue::DescriptionNotAString => Vec::new(),
            ValidationIssue::NameNotKebabCase { segment } => {
                vec![("segment", Carried::Text(segment))]
            }
            ValidationIssue::NameSegmentTooLong { segment, length } => vec![
                ("segment", Carried::Text(segment)),
                ("length", Carried::Count(*length)),
            ],
            ValidationIssue::NameDirectoryMismatch { name, directory } => vec![
                ("name", Carried::Text(name)),
                ("directory", Carried::Text(directory)),
            ],
            ValidationIssue::NameFileMismatch { name, file } => {
                vec![("name", Carried::Text(name)), ("file", Carried::Text(file))]
            }
            ValidationIssue::DescriptionTooLong { length }
            | ValidationIssue::CompatibilityTooLong { length } => {
                vec![("length", Carried::Count(*length))]
            }
            ValidationIssue::MetadataNotAString { key }
            | ValidationIssue::MetadataRepeated { key } => vec![("key", Carried::Text(key))],
            ValidationIssue::FrontMatterUnparsed { reason } => {
                vec![("reason", Carried::Text(reason))]
            }
            ValidationIssue::TransportIncomplete { transport, field } => vec![
                ("transport", Carried::Text(transport)),
                ("field", Carried::Text(field)),
            ],
            ValidationIssue::InputRepeated { name }
            | ValidationIssue::InputInUserinfo { name }
            | ValidationIssue::InputNameMalformed { name }
            | ValidationIssue::SecretWithoutEnvVar { name }
            | ValidationIssue::McpNameNotAKey { name } => {
                vec![("name", Carried::Text(name))]
            }
            ValidationIssue::EnvVarMalformed { name, variable } => vec![
                ("name", Carried::Text(name)),
                ("variable", Carried::Text(variable)),
            ],
            ValidationIssue::SecretLiteral { at, prefix } => {
                vec![("at", Carried::Text(at)), ("prefix", Carried::Text(prefix))]
            }
            ValidationIssue::McpNameCaseClash { name, twin } => {
                vec![("name", Carried::Text(name)), ("twin", Carried::Text(twin))]
            }
            ValidationIssue::McpNameReserved {
                name,
                agent,
                reserved,
            } => vec![
                ("name", Carried::Text(name)),
                ("agent", Carried::Text(agent.name())),
                ("reserved", Carried::Text(reserved)),
            ],
            ValidationIssue::McpNameClaimed {
                name,
                agent,
                claimed,
            } => vec![
                ("name", Carried::Text(name)),
                ("agent", Carried::Text(agent.name())),
                ("claimed", Carried::Text(claimed)),
            ],
            ValidationIssue::InputUnreferenced { name, kind } => vec![
                ("name", Carried::Text(name)),
                ("kind", Carried::Text(kind.slug())),
            ],
        }
    }
}

/// A union of objects, one an issue of each code, its code a literal, so
/// the panel narrows an issue to what it carries by comparing its `code`:
/// `{ code: "name_segment_too_long", segment: string, length: number,
/// warning: boolean, message: string }`. What each carries is read off
/// [`carried`](ValidationIssue::carried), the match the serializer writes
/// from, on one issue of each code ([`ValidationIssue::every`]).
#[cfg(feature = "ts")]
impl ts_rs::TS for ValidationIssue {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;

    fn docs() -> Option<String> {
        Some(crate::typescript::docs(&[
            " Something that stops an artifact from being usable, or will: its code,",
            " what it is about, whether it is only worth a warning, and its message.",
        ]))
    }

    fn name(_: &ts_rs::Config) -> String {
        "ValidationIssue".to_owned()
    }

    fn decl(config: &ts_rs::Config) -> String {
        format!("type ValidationIssue ={};", Self::inline(config))
    }

    fn decl_concrete(config: &ts_rs::Config) -> String {
        Self::decl(config)
    }

    fn inline(_: &ts_rs::Config) -> String {
        crate::typescript::lines(ValidationIssue::every().iter().map(|issue| {
            let mut fields = vec![format!("code: {:?}", issue.code())];
            fields.extend(issue.carried().into_iter().map(|(name, carried)| {
                let typed = match carried {
                    Carried::Text(_) => "string",
                    Carried::Count(_) => "number",
                };
                format!("{name}: {typed}")
            }));
            fields.push("warning: boolean".to_owned());
            fields.push("message: string".to_owned());
            format!("{{ {} }}", fields.join(", "))
        }))
    }

    fn output_path() -> Option<PathBuf> {
        Some("ValidationIssue.ts".into())
    }
}

/// One object of each code, its code a constant, so a reader tells an issue
/// by its `code` alone, as the TypeScript's union is told: each holds what
/// it carries, read off [`carried`](ValidationIssue::carried) as the
/// serializer writes it, each said as schemars says its type, then
/// `warning` and `message`.
#[cfg(feature = "schema")]
impl schemars::JsonSchema for ValidationIssue {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ValidationIssue".into()
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        concat!(module_path!(), "::ValidationIssue").into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let members: Vec<serde_json::Value> = ValidationIssue::every()
            .iter()
            .map(|issue| {
                let mut fields = vec![("code", serde_json::json!({ "const": issue.code() }))];
                for (name, carried) in issue.carried() {
                    let typed = match carried {
                        Carried::Text(_) => generator.subschema_for::<str>(),
                        Carried::Count(_) => generator.subschema_for::<usize>(),
                    };
                    fields.push((name, typed.to_value()));
                }
                fields.push(("warning", generator.subschema_for::<bool>().to_value()));
                fields.push(("message", generator.subschema_for::<str>().to_value()));
                let required: Vec<&str> = fields.iter().map(|(name, _)| *name).collect();
                let properties: serde_json::Map<String, serde_json::Value> = fields
                    .into_iter()
                    .map(|(name, typed)| (name.to_owned(), typed))
                    .collect();
                serde_json::json!({
                    "type": "object",
                    "properties": properties,
                    "required": required,
                })
            })
            .collect();
        crate::schema::schema(serde_json::json!({
            "description": crate::schema::description(&[
                " Something that stops an artifact from being usable, or will: its code,",
                " what it is about, whether it is only worth a warning, and its message.",
            ]),
            "oneOf": members,
        }))
    }
}

#[cfg(any(feature = "ts", feature = "schema"))]
impl ValidationIssue {
    /// One issue of each code, in the order [`CODES`](Self::CODES) lists
    /// them, which a test holds it to: what the TypeScript and the schema
    /// are read off.
    pub(crate) fn every() -> Vec<ValidationIssue> {
        let text = String::new;
        vec![
            ValidationIssue::NameEmpty,
            ValidationIssue::NameNotKebabCase { segment: text() },
            ValidationIssue::NameSegmentTooLong {
                segment: text(),
                length: 0,
            },
            ValidationIssue::NameDirectoryMismatch {
                name: text(),
                directory: text(),
            },
            ValidationIssue::NameFileMismatch {
                name: text(),
                file: text(),
            },
            ValidationIssue::DescriptionEmpty,
            ValidationIssue::DescriptionTooLong { length: 0 },
            ValidationIssue::CompatibilityEmpty,
            ValidationIssue::CompatibilityTooLong { length: 0 },
            ValidationIssue::CompatibilityNotAString,
            ValidationIssue::MetadataNotAMap,
            ValidationIssue::MetadataNotAString { key: text() },
            ValidationIssue::MetadataRepeated { key: text() },
            ValidationIssue::Empty,
            ValidationIssue::FrontMatterUnparsed { reason: text() },
            ValidationIssue::PathsNotStrings,
            ValidationIssue::DescriptionNotAString,
            ValidationIssue::TransportIncomplete {
                transport: text(),
                field: text(),
            },
            ValidationIssue::InputRepeated { name: text() },
            ValidationIssue::InputUnreferenced {
                name: text(),
                kind: InputKind::Env,
            },
            ValidationIssue::InputInUserinfo { name: text() },
            ValidationIssue::InputNameMalformed { name: text() },
            ValidationIssue::EnvVarMalformed {
                name: text(),
                variable: text(),
            },
            ValidationIssue::SecretWithoutEnvVar { name: text() },
            ValidationIssue::SecretLiteral {
                at: text(),
                prefix: text(),
            },
            ValidationIssue::McpNameNotAKey { name: text() },
            ValidationIssue::McpNameReserved {
                name: text(),
                agent: Agent::Claude,
                reserved: text(),
            },
            ValidationIssue::McpNameClaimed {
                name: text(),
                agent: Agent::Claude,
                claimed: text(),
            },
            ValidationIssue::McpNameCaseClash {
                name: text(),
                twin: text(),
            },
        ]
    }
}

impl fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationIssue::NameEmpty => write!(f, "the name is empty"),
            ValidationIssue::NameNotKebabCase { segment } => write!(
                f,
                "`{segment}` is not usable as a name: it has to be lowercase letters and digits, joined by single hyphens"
            ),
            ValidationIssue::NameSegmentTooLong { segment, length } => write!(
                f,
                "`{segment}` is {length} characters, over the {MAX_NAME_SEGMENT_LEN} character limit"
            ),
            ValidationIssue::NameDirectoryMismatch { name, directory } => write!(
                f,
                "`name` is `{name}` but the directory is `{directory}`; they have to match"
            ),
            ValidationIssue::NameFileMismatch { name, file } => write!(
                f,
                "`name` is `{name}` but the file is `{file}`; they have to match"
            ),
            ValidationIssue::DescriptionEmpty => write!(f, "`description` is empty"),
            ValidationIssue::DescriptionTooLong { length } => write!(
                f,
                "`description` is {length} characters, over the {MAX_DESCRIPTION_LEN} character limit"
            ),
            ValidationIssue::CompatibilityEmpty => write!(
                f,
                "`compatibility` is empty; leave it out when the skill needs nothing in particular"
            ),
            ValidationIssue::CompatibilityTooLong { length } => write!(
                f,
                "`compatibility` is {length} characters, over the {MAX_COMPATIBILITY_LEN} character limit"
            ),
            ValidationIssue::CompatibilityNotAString => write!(
                f,
                "`compatibility` is not a string, so it is ignored, and left in the file as written"
            ),
            ValidationIssue::MetadataNotAMap => write!(
                f,
                "`metadata` is not a map of strings, so it is ignored, and left in the file as written"
            ),
            ValidationIssue::MetadataNotAString { key } => write!(
                f,
                "`metadata` entry `{key}` is not a string, so it is ignored, and left in the file as written"
            ),
            ValidationIssue::MetadataRepeated { key } => write!(
                f,
                "`metadata` entry `{key}` is written more than once, so only the first is read"
            ),
            ValidationIssue::Empty => write!(f, "the file has no content beyond any front matter"),
            ValidationIssue::FrontMatterUnparsed { reason } => write!(
                f,
                "the front matter does not parse, so it is read as content, fences and all: {reason}"
            ),
            ValidationIssue::PathsNotStrings => write!(
                f,
                "`paths` is not a pattern or a list of patterns, so it is ignored, and left in the file as written"
            ),
            ValidationIssue::DescriptionNotAString => write!(
                f,
                "`description` is not a string, so it is ignored, and left in the file as written"
            ),
            ValidationIssue::TransportIncomplete { transport, field } => write!(
                f,
                "the {transport} transport has no `{field}`, which no agent can start or reach the server without"
            ),
            ValidationIssue::InputRepeated { name } => write!(
                f,
                "input `{name}` is declared more than once, so which of them `{{{name}}}` stands for is not known"
            ),
            ValidationIssue::InputUnreferenced { name, kind } => write!(
                f,
                "input `{name}` is used nowhere: an input of kind `{}` is written `{{{name}}}` in {}",
                kind.slug(),
                kind.place()
            ),
            ValidationIssue::InputInUserinfo { name } => write!(
                f,
                "input `{name}` is written in the user and password of the transport's `url`, \
                 which no address is kept with, so no agent would be given it: write `{{{name}}}` \
                 after the host, or declare it of kind `header` and write it in a value of the \
                 transport's `headers`"
            ),
            ValidationIssue::InputNameMalformed { name } => write!(
                f,
                "input `{name}` cannot be written between braces where its value goes: an \
                 input's name is letters, digits, hyphens and underscores"
            ),
            ValidationIssue::EnvVarMalformed { name, variable } => write!(
                f,
                "input `{name}` is read from `{variable}`, which not every agent can refer to: \
                 an environment variable's name is letters, digits and underscores, and does \
                 not start with a digit"
            ),
            ValidationIssue::SecretWithoutEnvVar { name } => write!(
                f,
                "secret input `{name}` names no `env_var` to be read from, and a secret reaches \
                 an agent only as a reference to an environment variable, never as a value a \
                 file holds"
            ),
            // Where, and what it starts with, and never the value: the
            // message goes wherever `validate` is read, and the value is a
            // secret.
            ValidationIssue::SecretLiteral { at, prefix } => write!(
                f,
                "`{at}` holds what looks like a token, starting `{prefix}`, and a definition \
                 holds no secret: declare a secret input read from an environment variable, \
                 and write it there between braces"
            ),
            ValidationIssue::McpNameNotAKey { name } => write!(
                f,
                "`{name}` is not usable as an MCP server's name: every agent keys a server by \
                 letters, digits, hyphens and underscores, and nothing else"
            ),
            // Matched on the agent, since what it keeps the name for is its
            // own, so a new one says it.
            ValidationIssue::McpNameReserved {
                name,
                agent,
                reserved,
            } => match agent {
                Agent::Claude | Agent::Codex => write!(
                    f,
                    "`{name}` is a name {} keeps for a server of its own (`{reserved}`), so no \
                     server of yours can be keyed by it",
                    agent.title()
                ),
                Agent::OpenCode => write!(
                    f,
                    "`{name}` is a key OpenCode reads in its `mcp` table itself (`{reserved}`), \
                     so a server keyed by it there would be read as that key"
                ),
            },
            ValidationIssue::McpNameClaimed {
                name,
                agent,
                claimed,
            } => match agent {
                Agent::Claude => write!(
                    f,
                    "`{name}` is the name Claude Code gives the skills Anthropic provides \
                     (`{claimed}`), so a server of yours by it may be taken for theirs; it is \
                     written all the same"
                ),
                Agent::Codex | Agent::OpenCode => write!(
                    f,
                    "`{name}` is a name {} uses for one of its own (`{claimed}`), so a server of \
                     yours by it may be taken for that one; it is written all the same",
                    agent.title()
                ),
            },
            ValidationIssue::McpNameCaseClash { name, twin } => write!(
                f,
                "`{name}` and `{twin}` differ only by case, which a file system that folds case, \
                 as macOS's and Windows' do, holds as one file, so no library holds both \
                 everywhere: rename one of them"
            ),
        }
    }
}

/// Why an artifact could not be loaded. Every variant names the file, because
/// a catalog reports these next to each other and the path tells them apart.
#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("{path}: cannot be read: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: {source}")]
    FrontMatter {
        path: PathBuf,
        #[source]
        source: FrontMatterError,
    },
    #[error("{path}: invalid front matter: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_yaml_ng::Error,
    },
    /// Boxed, since what TOML says of where it stopped is large, and every
    /// error carrying this one would grow to its size.
    #[error("{path}: {source}")]
    Definition {
        path: PathBuf,
        #[source]
        source: Box<McpError>,
    },
}

codes!(ArtifactError {
    Self::Read { .. } => "artifact_read_failed",
    Self::Parse { .. } => "front_matter_invalid",
} carried {
    Self::FrontMatter { source, .. } => source,
    Self::Definition { source, .. } => source,
});

impl ArtifactError {
    /// The file the failure is about.
    pub fn path(&self) -> &Path {
        match self {
            ArtifactError::Read { path, .. }
            | ArtifactError::FrontMatter { path, .. }
            | ArtifactError::Parse { path, .. }
            | ArtifactError::Definition { path, .. } => path,
        }
    }

    /// The same error, its path a route from `root` when it is under it:
    /// for a file read from a folder that is gone by the time it is said, as
    /// a gather's dry run reads its clone ([`crate::gather::dry_run`]).
    pub(crate) fn routed(self, root: &Path) -> Self {
        let routed = |path: PathBuf| match path.strip_prefix(root) {
            Ok(route) => route.to_path_buf(),
            Err(_) => path,
        };
        match self {
            ArtifactError::Read { path, source } => ArtifactError::Read {
                path: routed(path),
                source,
            },
            ArtifactError::FrontMatter { path, source } => ArtifactError::FrontMatter {
                path: routed(path),
                source,
            },
            ArtifactError::Parse { path, source } => ArtifactError::Parse {
                path: routed(path),
                source,
            },
            ArtifactError::Definition { path, source } => ArtifactError::Definition {
                path: routed(path),
                source,
            },
        }
    }
}

#[cfg(test)]
#[path = "artifact_test.rs"]
mod tests;
