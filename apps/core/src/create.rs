//! Writing a new artifact into a mind project.
//!
//! The one way into a project that is not a copy from the shelf: a skill, a
//! rule or an MCP server somebody is about to write, scaffolded so that it
//! passes `validate` from the moment it exists. Without a template, what is
//! written is the least that does: a skill's front matter and a heading, a
//! rule's opening line, and for a server, which a name and a description do
//! not say how to start, the definition written from what a command line
//! said of its transport ([`Given`]). With one, the template is copied and
//! the name and description filled in.
//!
//! Nothing here records anything in the ledger. A created artifact is its
//! author's work, not something Mindweaver installed, so [`crate::install`]
//! treats it as foreign: never overwritten by a shelf entry of the same name,
//! never removed by unticking one. Nor does anything here drop a record: an
//! old one under the name written is [`crate::lifecycle::add`]'s to drop, so
//! a front end adds with that rather than with these. They stay public as the
//! bare writers, which core's tests build their fixtures with.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

use crate::artifact::{self, issue_list, Artifact, ArtifactError, ValidationIssue};
use crate::code::codes;
use crate::frontmatter::{self, FrontMatterError};
use crate::kind::{Kind, Layout};
use crate::lifecycle::twins_beside;
use crate::mcp::{self, Given, McpDefinition};
use crate::rule::RuleManifest;
use crate::skill::{SkillManifest, MAX_DESCRIPTION_LEN};
use crate::template::{Template, TemplateError};
use crate::url::carries_credentials;
use crate::value::{ArtifactName, Redacted};
use crate::workspace::{MindProject, WorkspaceError};

/// What a template writes where the name goes.
pub const NAME_PLACEHOLDER: &str = "{{name}}";
/// What a template writes where the description goes.
pub const DESCRIPTION_PLACEHOLDER: &str = "{{description}}";

/// What an artifact is written from beside its name and its description.
///
/// One of these, never two: a template says how a server is started as
/// surely as a command line does, so a front end's grammar refuses both
/// before one of these is made.
#[derive(Debug, Clone, Copy)]
pub enum Start<'a> {
    /// The least that passes `validate`: what [`create`] writes.
    Scratch,
    /// A template, copied and filled in: what [`create_from`] writes.
    Template(&'a Template),
    /// What a command line said of how an MCP server is started or
    /// reached: what [`create_server`] writes, and refused for any other
    /// kind.
    Server(&'a Given),
    /// An MCP server's definition whole, as an import from a registry
    /// makes one ([`crate::mcp_registry::Imported::text`]): its `name` and
    /// `description` set to those given, as a template's are, every other
    /// byte kept, and refused for any other kind.
    ///
    /// Through `add`'s path rather than a writer of its own, so what is
    /// beside it in the folder, a file already where it goes and the rules
    /// `validate` holds a definition to are judged by the one function
    /// that judges every server added.
    Definition(&'a str),
}

/// Create an artifact of `kind` called `name` in `project`, and load it back.
///
/// `description` is the one line saying what the artifact is: a skill's
/// `description`, a rule's opening line. Both kinds need one (a skill without
/// it is invalid and a rule without an opening line is an empty file), so it
/// is asked for rather than left as a placeholder somebody forgets to replace.
///
/// Nothing already there is written over, including a folder that is not an
/// artifact at all: it is somebody's, and a scaffold that clobbered it would
/// be the worst kind of convenience.
pub fn create(
    project: &MindProject,
    kind: Kind,
    name: &str,
    description: &str,
) -> Result<Artifact, CreateError> {
    scaffold(project, kind, name, description, Start::Scratch)
}

/// The same, starting from a template.
///
/// A skill template is copied whole, and in its manifest the `name` and
/// `description` are set (whatever the template had for them is replaced)
/// and [`NAME_PLACEHOLDER`] and [`DESCRIPTION_PLACEHOLDER`] in the body are
/// filled in. A rule template is filled in throughout, front matter included,
/// with a `description` its front matter declares set in the same way, and
/// gets the description as a heading on top of its body, below any front
/// matter, when it does not say where the description goes; front matter that
/// `validate` would warn about is refused. Other files are copied as they
/// are: a placeholder in a script is the script's business.
pub fn create_from(
    project: &MindProject,
    kind: Kind,
    name: &str,
    description: &str,
    template: &Template,
) -> Result<Artifact, CreateError> {
    scaffold(project, kind, name, description, Start::Template(template))
}

/// Create the MCP server `name` in `project`, its definition written from
/// how `given` says it is started or reached, and load it back.
///
/// Held to everything `validate` would call it invalid for, as a rename
/// holds one, before a byte is written: the name, beside the definitions in
/// the folder it is written in, and the definition as it would read.
pub fn create_server(
    project: &MindProject,
    name: &str,
    description: &str,
    given: &Given,
) -> Result<Artifact, CreateError> {
    scaffold(project, Kind::Mcp, name, description, Start::Server(given))
}

/// Write what `start` makes of `kind` `name` into `project`.
pub(crate) fn scaffold(
    project: &MindProject,
    kind: Kind,
    name: &str,
    description: &str,
    start: Start<'_>,
) -> Result<Artifact, CreateError> {
    plan(project, kind, name, description, start)?.place(project)
}

/// What [`create`] or [`create_from`] would write, worked out and not yet
/// written: every check made that does not need the disk to say no, and
/// every byte built.
///
/// Apart from writing it so that a front end can ask what `add` would make
/// of an artifact as it is typed ([`crate::draft::added`]), and be told
/// what writing it would be told, from the same checks rather than a copy
/// of them; and so that whatever a template could break is found before a
/// folder is made, as it always was.
#[derive(Debug)]
pub(crate) enum Scaffold {
    /// A skill: the folder it is, and each file in it by its route.
    Skill {
        folder: PathBuf,
        files: Vec<(PathBuf, Vec<u8>)>,
    },
    /// A rule: its one file, and what it holds.
    Rule {
        name: String,
        file: PathBuf,
        text: String,
    },
    /// An MCP server: its definition's one file, and what it holds.
    Mcp {
        name: String,
        file: PathBuf,
        text: String,
    },
}

/// Whether `add` writes `kind` from a name and a description alone: a
/// skill and a rule, and not an MCP server, whose definition passes
/// `validate` only once it says how the server is started or reached,
/// which neither says ([`CreateError::McpUnscaffolded`]), and which `add`
/// is told by a command line's options or a template. What a front end
/// offering `add` by kind, with a name and a description to fill in,
/// offers it for. Matched on the kind, so a kind added says whether it is
/// scaffolded before it compiles, as [`plan`] does.
pub const fn scaffolds(kind: Kind) -> bool {
    match kind {
        Kind::Skill | Kind::Rule => true,
        Kind::Mcp => false,
    }
}

/// Work out what adding `kind` `name`, described as `description`, from
/// `start`, would write into `project`.
pub(crate) fn plan(
    project: &MindProject,
    kind: Kind,
    name: &str,
    description: &str,
    start: Start<'_>,
) -> Result<Scaffold, CreateError> {
    // What the line asks for before what it names: a transport given for a
    // kind no agent starts, or a template of another kind, is the line's
    // mistake whatever the name.
    match start {
        Start::Server(_) if kind != Kind::Mcp => {
            return Err(CreateError::TransportNotMcp { kind });
        }
        Start::Definition(_) if kind != Kind::Mcp => {
            return Err(CreateError::DefinitionNotMcp { kind });
        }
        Start::Template(template) if template.kind != kind => {
            return Err(CreateError::Template(TemplateError::WrongKind {
                name: template.name.clone(),
                is: template.kind,
                wanted: kind,
            }));
        }
        Start::Scratch | Start::Template(_) | Start::Server(_) | Start::Definition(_) => {}
    }
    // Matched on the kind, as `rename` holds a name: a server's to the keys
    // an agent takes and to what sits beside it, read off the folder it is
    // written in as discovery reads it, the others' to kebab-case.
    let twins = match kind {
        Kind::Skill | Kind::Rule => Vec::new(),
        Kind::Mcp => project
            .directory_for(kind)
            .map(|folder| {
                let file = place_in(&folder, name);
                twins_beside(&folder, name, &file)
            })
            .unwrap_or_default(),
    };
    let issues = match kind {
        Kind::Skill | Kind::Rule => artifact::name_issues(name),
        Kind::Mcp => artifact::mcp_name_strict_issues(name, &twins),
    };
    if !issues.is_empty() {
        return Err(CreateError::Name {
            name: name.to_owned(),
            issues,
        });
    }
    let description = description.trim();
    check_description(kind, description)?;

    let directory = project.required_directory(kind)?;
    let template = match start {
        Start::Template(template) => Some(template),
        Start::Scratch | Start::Server(_) | Start::Definition(_) => None,
    };
    // Matched on the kind rather than its layout, so a third kind has to say
    // how it is scaffolded before it compiles.
    match kind {
        Kind::Skill => plan_skill(&directory, name, description, template),
        Kind::Rule => plan_rule(&directory, name, description, template),
        Kind::Mcp => plan_mcp(&directory, name, description, start, &twins),
    }
}

impl Scaffold {
    /// The folder a skill would be, or the file a rule would: what is
    /// already there refuses it.
    pub(crate) fn path(&self) -> &Path {
        match self {
            Scaffold::Skill { folder, .. } => folder,
            Scaffold::Rule { file, .. } | Scaffold::Mcp { file, .. } => file,
        }
    }

    /// What would refuse it once it came to be written, as far as can be
    /// told without writing: something already where it goes, which is left
    /// alone. Told again by the write itself, with `create_new`, since the
    /// disk can change in between.
    pub(crate) fn taken(&self) -> Option<CreateError> {
        let path = self.path();
        fs::symlink_metadata(path)
            .is_ok()
            .then(|| CreateError::Exists {
                path: path.to_path_buf(),
            })
    }

    /// The artifact it would be once written in `project`, read from what
    /// it would hold as discovery reads it from the disk: what `validate`
    /// would then say of it.
    pub(crate) fn read_as(&self, project: &MindProject) -> Result<Artifact, CreateError> {
        match self {
            Scaffold::Skill { folder, files } => {
                let Layout::Directory { manifest } = Kind::Skill.layout() else {
                    unreachable!("a skill is a directory");
                };
                let source = files
                    .iter()
                    .find(|(route, _)| route == Path::new(manifest))
                    .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
                    .unwrap_or_default();
                Ok(Artifact::skill_holding(
                    folder.clone(),
                    project.root(),
                    &source,
                )?)
            }
            Scaffold::Rule { name, file, text } => Ok(Artifact::rule_holding(
                file.clone(),
                name.clone(),
                project.root(),
                text,
            )),
            // Told of what sits beside it, as discovery tells it.
            Scaffold::Mcp { name, file, text } => {
                let mut artifact =
                    Artifact::mcp_holding(file.clone(), name.clone(), project.root(), text)?;
                if let Some(folder) = file.parent() {
                    artifact.twinned(twins_beside(folder, name, file));
                }
                Ok(artifact)
            }
        }
    }

    /// Write it into `project`, and load it back.
    fn place(self, project: &MindProject) -> Result<Artifact, CreateError> {
        match self {
            Scaffold::Skill { folder, files } => place_skill(project, &folder, &files),
            Scaffold::Rule { name, file, text } => {
                if let Some(parent) = file.parent() {
                    create_dir(parent)?;
                }
                write_new(&file, text.as_bytes())?;
                Ok(Artifact::rule(file, name, project.root())?)
            }
            Scaffold::Mcp { name, file, text } => {
                if let Some(parent) = file.parent() {
                    create_dir(parent)?;
                }
                write_new(&file, text.as_bytes())?;
                Ok(Artifact::mcp(file, name, project.root())?)
            }
        }
    }
}

/// Where the definition of the server `name` goes in `directory`, the
/// folder servers are kept in.
fn place_in(directory: &Path, name: &str) -> PathBuf {
    let Layout::Flat { extension } = Kind::Mcp.layout() else {
        unreachable!("a server is a file directly in its folder");
    };
    directory.join(format!("{name}.{extension}"))
}

/// An MCP server: its definition, written from what `start` says of its
/// transport and held to what `validate` would call it invalid for beside
/// `twins`, the definitions in `directory` named as it is in another case.
///
/// Nothing to write a transport from is refused rather than scaffolded: a
/// definition passes `validate` only once it says how the server is
/// started or reached, which a name and a description do not, so one
/// holding neither would be invalid from the moment it exists, which is
/// what scaffolding is there to prevent.
fn plan_mcp(
    directory: &Path,
    name: &str,
    description: &str,
    start: Start<'_>,
    twins: &[ArtifactName],
) -> Result<Scaffold, CreateError> {
    let text = match start {
        Start::Scratch => {
            return Err(CreateError::McpUnscaffolded {
                directory: directory.to_path_buf(),
            })
        }
        Start::Template(template) => {
            let source = template.text()?;
            mcp::declaring(&source, name, Some(description)).map_err(|reason| {
                CreateError::Manifest {
                    reason: format!("the template `{}`: {reason}", template.name),
                }
            })?
        }
        Start::Server(given) => {
            writable(given)?;
            given.text(name, description)
        }
        Start::Definition(source) => {
            mcp::declaring(source, name, Some(description)).map_err(|reason| {
                CreateError::Manifest {
                    reason: format!("the definition given: {reason}"),
                }
            })?
        }
    };
    let file = place_in(directory, name);
    let definition = McpDefinition::parse(&text).map_err(|error| CreateError::Manifest {
        reason: format!("it does not read as an MCP server's definition: {error}"),
    })?;
    // Held to everything `validate` calls a definition invalid for, the
    // strict function `rename` and `edit --contents` hold one to, so `add`
    // refuses exactly that and has no rule of its own for it.
    let file_name = file
        .file_name()
        .map(|file| file.to_string_lossy().into_owned())
        .unwrap_or_default();
    let issues = artifact::mcp_strict_issues(&definition, name, &file_name, twins);
    if !issues.is_empty() {
        return Err(CreateError::Definition {
            name: name.to_owned(),
            issues,
        });
    }
    Ok(Scaffold::Mcp {
        name: name.to_owned(),
        file,
        text,
    })
}

/// Refuse what a command line told of a server's transport that no
/// definition can be written from as given: an address typed with
/// something before its host, which the file would keep, so a credential
/// in it is refused before a byte is written, as a gather refuses one, and
/// said without it; a name given beside a value that holds `=` ([`sayable`]);
/// and a key of `env` or `headers` given twice, which no TOML file holds.
///
/// What `add` asks before it writes a definition from the options, and
/// `edit` before it writes a definition's transport again from them
/// ([`crate::lifecycle::retransport`]), so the two refuse alike.
pub(crate) fn writable(given: &Given) -> Result<(), CreateError> {
    if let Given::Address(address) = given {
        if carries_credentials(&address.url) {
            return Err(CreateError::UrlUserinfo {
                url: Redacted::new(&address.url),
            });
        }
    }
    sayable(given)?;
    if let Some(repeated) = given.repeated() {
        return Err(CreateError::KeyRepeated {
            table: repeated.table,
            key: repeated.key,
        });
    }
    Ok(())
}

/// Refuse a server's options holding a name a command line cannot say as
/// given, one beside a value that holds `=` ([`Given::unsayable`]): which
/// only a request can send, and which its line would say as another pair.
///
/// Asked of a draft as `add` and `edit` ask it ([`writable`]), and of a
/// task by the path policy before its line is built
/// ([`crate::policy::sayable`]), so the draft refuses what the task would,
/// by the same code, and no line is run that writes a pair other than the
/// one judged.
pub(crate) fn sayable(given: &Given) -> Result<(), CreateError> {
    match given.unsayable() {
        Some(unsayable) => Err(CreateError::KeyUnsayable {
            option: unsayable.option,
            key: unsayable.key,
        }),
        None => Ok(()),
    }
}

/// A skill: its own directory, holding a manifest with front matter.
fn plan_skill(
    directory: &Path,
    name: &str,
    description: &str,
    template: Option<&Template>,
) -> Result<Scaffold, CreateError> {
    // Skills are flat. A skill's directory belongs to it, scripts and
    // references included, so a `/` would file one skill inside another's
    // assets, where discovery never looks.
    if name.contains('/') {
        return Err(CreateError::Nested {
            name: name.to_owned(),
        });
    }
    let Layout::Directory { manifest } = Kind::Skill.layout() else {
        unreachable!("a skill is a directory");
    };

    // Everything is worked out before anything is written, so a template
    // whose manifest does not parse costs nothing but the error.
    let files = match template {
        None => vec![(
            PathBuf::from(manifest),
            skill_manifest(name, description, "", "")
                .map_err(|reason| CreateError::Manifest { reason })?
                .into_bytes(),
        )],
        Some(template) => from_template(template, manifest, name, description)?,
    };
    Ok(Scaffold::Skill {
        folder: directory.join(name),
        files,
    })
}

/// Write a skill's `files` into `folder`, which must not be there yet.
fn place_skill(
    project: &MindProject,
    folder: &Path,
    files: &[(PathBuf, Vec<u8>)],
) -> Result<Artifact, CreateError> {
    if let Some(directory) = folder.parent() {
        create_dir(directory)?;
    }
    // `create_dir`, not `create_dir_all`: failing when the folder is already
    // there is the check, with nothing in between for another write to slip
    // into.
    fs::create_dir(folder).map_err(|source| match source.kind() {
        io::ErrorKind::AlreadyExists => CreateError::Exists {
            path: folder.to_path_buf(),
        },
        _ => CreateError::Write {
            path: folder.to_path_buf(),
            source,
        },
    })?;

    let written = files.iter().try_for_each(|(route, contents)| {
        let path = folder.join(route);
        if let Some(parent) = path.parent() {
            create_dir(parent)?;
        }
        write_new(&path, contents)
    });
    if let Err(error) = written {
        // The folder was made a moment ago, by this call, so everything in it
        // is this call's: leaving it would be a skill directory with half a
        // skill in it.
        let _ = fs::remove_dir_all(folder);
        return Err(error);
    }

    // Read back rather than trusted, so what is reported is what a listing
    // will find.
    Ok(Artifact::skill(folder, project.root())?)
}

/// A template's files, with its manifest filled in.
fn from_template(
    template: &Template,
    manifest: &str,
    name: &str,
    description: &str,
) -> Result<Vec<(PathBuf, Vec<u8>)>, CreateError> {
    let mut files = template.skill_files()?;
    let Some((_, source)) = files
        .iter_mut()
        .find(|(route, _)| route == Path::new(manifest))
    else {
        return Err(CreateError::Manifest {
            reason: format!("the template `{}` has no {manifest}", template.name),
        });
    };

    let text = String::from_utf8(std::mem::take(source)).map_err(|_| CreateError::Manifest {
        reason: format!("the template's {manifest} is not UTF-8 text"),
    })?;
    // A manifest with no front matter is all body: it gets one.
    let (front_matter, body) = match frontmatter::split(&text) {
        Ok(document) => (document.front_matter, document.body),
        Err(FrontMatterError::Missing) => ("", text.as_str()),
        Err(error) => {
            return Err(CreateError::Manifest {
                reason: format!("the template's {manifest}: {error}"),
            })
        }
    };
    let front_matter = frontmatter::without_keys(front_matter, &["name", "description"]);
    let body = fill(body, name, description);
    *source = skill_manifest(name, description, &front_matter, &body)
        .map_err(|reason| CreateError::Manifest {
            reason: format!("the template `{}`: {reason}", template.name),
        })?
        .into_bytes();
    Ok(files)
}

/// A rule: one markdown file, filed wherever its route says.
fn plan_rule(
    directory: &Path,
    name: &str,
    description: &str,
    template: Option<&Template>,
) -> Result<Scaffold, CreateError> {
    let Layout::Files { extension } = Kind::Rule.layout() else {
        unreachable!("a rule is a file");
    };

    // Worked out before anything is written, as a skill's manifest is, so a
    // template whose front matter would not be read costs nothing but the
    // error.
    let text = match template {
        None => format!("# {description}\n"),
        Some(template) => {
            let text = template.text()?;
            rule_from_template(&text, name, description).map_err(|reason| {
                CreateError::Manifest {
                    reason: format!("the template `{}`: {reason}", template.name),
                }
            })?
        }
    };

    // The name is a route, so `git/no-force-push` lands in a `git` folder.
    // Every segment has already passed the name check, so none of them is `.`
    // or `..` and the file cannot land outside the rules directory.
    Ok(Scaffold::Rule {
        name: name.to_owned(),
        file: directory.join(format!("{name}.{extension}")),
        text,
    })
}

/// A rule template filled in, with the description as a heading on top of
/// its body when the template does not say where the description goes: a
/// rule's opening line is what every listing shows.
///
/// A `description` the front matter declares is set, as a skill's is,
/// whatever the template had for it: the template's lines for it are
/// dropped, continuation lines and all, and the one typed is serialized in
/// their place. Pasted in as text, a description containing `: ` would not
/// parse, and one opening with `#` would be a comment, declaring nothing.
///
/// Front matter is held to everything `validate` would warn about, so that
/// a template's `paths:` never becomes content without a word: strict when
/// writing, as a skill's manifest is.
fn rule_from_template(text: &str, name: &str, description: &str) -> Result<String, String> {
    #[derive(Serialize)]
    struct Declared<'a> {
        description: &'a str,
    }

    let placed = text.contains(DESCRIPTION_PLACEHOLDER);
    // Where the fences are is found on the template, and whether they hold
    // front matter is decided on what they hold once filled in: `{{name}}`
    // is a YAML flow mapping, so a template scoping its rule to `{{name}}/**`
    // does not parse until it is.
    if let Ok(document) = frontmatter::split(text) {
        // Both fences as the template wrote them, byte order mark and line
        // endings included.
        let opening = &text[..text.find('\n').map_or(text.len(), |end| end + 1)];
        let closing =
            &text[opening.len() + document.front_matter.len()..text.len() - document.body.len()];
        let rest = frontmatter::without_keys(document.front_matter, &["description"]);
        let declared = if rest.len() == document.front_matter.len() {
            String::new()
        } else {
            serde_yaml_ng::to_string(&Declared { description }).map_err(|e| e.to_string())?
        };
        let block = format!(
            "{opening}{declared}{}{closing}",
            fill(&rest, name, description)
        );
        let (manifest, rest) = RuleManifest::read(&block);
        let issues = artifact::rule_issues(&manifest);
        if !issues.is_empty() {
            return Err(issue_list(&issues));
        }
        // A key the line-by-line drop cannot see, such as a quoted one.
        if manifest
            .description
            .stated()
            .is_some_and(|stated| stated != description)
        {
            return Err(String::from(
                "its front matter sets `description` in a way that cannot be replaced",
            ));
        }
        // Read as front matter, so the heading goes below it: above it, the
        // fences would no longer open the file, and the front matter would
        // become content. Otherwise the fences are horizontal rules, and the
        // whole template is body.
        if rest.is_empty() {
            let body = fill(document.body, name, description);
            return Ok(if placed {
                format!("{block}{body}")
            } else {
                let body = body.trim_start_matches(['\n', '\r']);
                format!("{block}\n# {description}\n\n{body}")
            });
        }
    }
    let body = fill(text, name, description);
    Ok(if placed {
        body
    } else {
        format!("# {description}\n\n{body}")
    })
}

/// Refuse a description that would not survive as what it is meant to be.
fn check_description(kind: Kind, description: &str) -> Result<(), CreateError> {
    if description.is_empty() {
        return Err(CreateError::DescriptionEmpty { kind });
    }
    // One line, because a rule's is its opening line and a skill's is the
    // sentence an agent reads to decide whether to load the rest. Anything
    // longer belongs in the file itself.
    if description.contains(['\n', '\r']) {
        return Err(CreateError::DescriptionNotOneLine);
    }
    let length = description.chars().count();
    if length > MAX_DESCRIPTION_LEN {
        return Err(CreateError::DescriptionTooLong { length });
    }
    Ok(())
}

/// A skill's manifest: `name` and `description` first, then whatever else the
/// front matter says, then the body, checked by parsing it back.
///
/// The two keys are serialized rather than formatted: a description is free
/// text, and one containing `: ` or opening with a quote would otherwise write
/// YAML that does not parse back into what was typed.
fn skill_manifest(
    name: &str,
    description: &str,
    rest_of_front_matter: &str,
    body: &str,
) -> Result<String, String> {
    #[derive(Serialize)]
    struct Declared<'a> {
        name: &'a str,
        description: &'a str,
    }

    let declared =
        serde_yaml_ng::to_string(&Declared { name, description }).map_err(|e| e.to_string())?;
    let front_matter = format!("{declared}{rest_of_front_matter}");
    let parsed = SkillManifest::parse(&front_matter)
        .map_err(|error| format!("its front matter does not parse: {error}"))?;
    if parsed.name != name || parsed.description != description {
        return Err(String::from(
            "its front matter sets `name` or `description` in a way that cannot be replaced",
        ));
    }
    // Held to the strict rules `rename` and `install` hold a skill to, so
    // that a rule added there reaches what `add` writes too. The name and the
    // description were checked before anything was built, with messages of
    // their own; this is what keeps those checks from being a second
    // definition of the same rules.
    let issues = artifact::strict_issues(&parsed, name);
    if !issues.is_empty() {
        return Err(format!("it would break a rule: {}", issue_list(&issues)));
    }
    // A body with nothing in it gets the heading a skill without a template
    // opens with, so a manifest is never front matter alone.
    let body = if body.trim().is_empty() {
        format!("\n# {name}\n")
    } else {
        body.to_owned()
    };
    Ok(format!("---\n{front_matter}---\n{body}"))
}

/// A template's text with the placeholders filled in.
fn fill(text: &str, name: &str, description: &str) -> String {
    text.replace(NAME_PLACEHOLDER, name)
        .replace(DESCRIPTION_PLACEHOLDER, description)
}

fn create_dir(path: &Path) -> Result<(), CreateError> {
    fs::create_dir_all(path).map_err(|source| CreateError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// Write a file that must not already exist.
///
/// `create_new` rather than a prior `exists()` check: the check and the write
/// are one operation, so nothing that appears in between is overwritten.
fn write_new(path: &Path, contents: &[u8]) -> Result<(), CreateError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| match source.kind() {
            io::ErrorKind::AlreadyExists => CreateError::Exists {
                path: path.to_path_buf(),
            },
            _ => CreateError::Write {
                path: path.to_path_buf(),
                source,
            },
        })?;
    file.write_all(contents)
        .map_err(|source| CreateError::Write {
            path: path.to_path_buf(),
            source,
        })
}

/// Why an artifact could not be created.
#[derive(Debug, Error)]
pub enum CreateError {
    #[error("{}", issue_list(.issues))]
    Name {
        name: String,
        issues: Vec<ValidationIssue>,
    },
    #[error("`{name}`: a skill's name cannot contain `/`; skills are flat, and only rules are filed in folders")]
    Nested { name: String },
    #[error("a {kind} needs a description: one line saying what it is")]
    DescriptionEmpty { kind: Kind },
    #[error("the description has to be one line; the rest belongs in the file")]
    DescriptionNotOneLine,
    #[error(
        "the description is {length} characters, over the {MAX_DESCRIPTION_LEN} character limit"
    )]
    DescriptionTooLong { length: usize },
    #[error("{path}: already exists, and is left alone")]
    Exists { path: PathBuf },
    #[error("the manifest cannot be written: {reason}")]
    Manifest { reason: String },
    #[error("{path}: cannot be written: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// An MCP server given nothing to write its transport from: no command
    /// to start it by, no address to reach it at, and no template to take
    /// either from.
    #[error(
        "an MCP server is added with how an agent starts it, `--command`, or where it \
         reaches it, `--url`, or from a `--template` that says so; or write its definition \
         as <name>.toml in {directory}"
    )]
    McpUnscaffolded { directory: PathBuf },
    /// A transport given for a kind no agent starts.
    #[error(
        "`--command` and `--url` say how an MCP server is started or reached, and a {} is \
         neither started nor reached",
        .kind.noun()
    )]
    TransportNotMcp { kind: Kind },
    /// A definition given whole for a kind that is no MCP server.
    #[error("a definition says how an MCP server is started or reached, and a {} is neither", .kind.noun())]
    DefinitionNotMcp { kind: Kind },
    /// A key of a transport's table given twice, which no TOML file holds:
    /// said by what it starts with alone when it looks like a token.
    #[error(
        "{} is given twice for the transport's `{table}`, which holds each key once",
        key_said(.key)
    )]
    KeyRepeated { table: &'static str, key: String },
    /// A name given beside a value that holds `=`, which the option that
    /// takes the pair ends at the first one: said by what it starts with
    /// alone when it looks like a token.
    #[error(
        "{} holds `=`, where `{option} NAME=VALUE` ends the name, so no command line can give \
         it; no variable's, header's or input's name holds one",
        key_said(.key)
    )]
    KeyUnsayable { option: &'static str, key: String },
    /// An address typed with something before its host, its userinfo,
    /// which the file would keep: said without it, and as the address
    /// alone when what is left of it still looks like a token.
    #[error(
        "{} was typed with something before its host, which a definition never keeps \
         and no agent is given: send a credential in a header read from an environment \
         variable instead (`--secret-header` or `--bearer-env`)",
        address_said(.url)
    )]
    UrlUserinfo { url: Redacted },
    /// A definition, written from a command line or a template, that
    /// `validate` would call invalid.
    #[error("`{name}` would be a definition `validate` calls invalid, so nothing was written: {}", issue_list(.issues))]
    Definition {
        name: String,
        issues: Vec<ValidationIssue>,
    },
    #[error(transparent)]
    Template(#[from] TemplateError),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
}

/// A key `add` is given twice, as its refusal says it: between backticks,
/// or, when it looks like a token, by what it starts with alone, as
/// `validate` says one, since no definition holds it yet for `validate`
/// to judge, and said whole it would print the secret.
fn key_said(key: &str) -> String {
    match mcp::token_in(key) {
        Some(prefix) => format!("a key that looks like a token, starting `{prefix}`,"),
        None => format!("`{key}`"),
    }
}

/// What is kept of an address typed with a credential, as its refusal says
/// it: between backticks, or, when what is left still looks like a token
/// (one in its query, say), as the address alone, by what that starts
/// with, for the reason [`key_said`] says a key so.
fn address_said(url: &Redacted) -> String {
    match mcp::token_in(url) {
        Some(prefix) => {
            format!("the address, which holds what looks like a token, starting `{prefix}`,")
        }
        None => format!("`{url}`"),
    }
}

codes!(CreateError {
    Self::Name { .. } => "add_name_unfit",
    Self::Nested { .. } => "add_skill_nested",
    Self::DescriptionEmpty { .. } => "add_description_empty",
    Self::DescriptionNotOneLine => "add_description_not_one_line",
    Self::DescriptionTooLong { .. } => "add_description_too_long",
    Self::Exists { .. } => "add_exists",
    Self::Manifest { .. } => "add_manifest_unwritable",
    Self::Write { .. } => "add_write_failed",
    Self::McpUnscaffolded { .. } => "add_mcp_unscaffolded",
    Self::TransportNotMcp { .. } => "add_transport_not_mcp",
    Self::DefinitionNotMcp { .. } => "add_definition_not_mcp",
    Self::KeyRepeated { .. } => "add_mcp_key_repeated",
    Self::KeyUnsayable { .. } => "add_mcp_key_unsayable",
    Self::UrlUserinfo { .. } => "add_mcp_url_userinfo",
    Self::Definition { .. } => "add_mcp_unfit",
} carried {
    Self::Template(error) => error,
    Self::Artifact(error) => error,
    Self::Workspace(error) => error,
});

#[cfg(test)]
#[path = "create_test.rs"]
mod tests;
