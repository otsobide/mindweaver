//! Mothers of plugin marketplaces: a git repository keeping a catalog,
//! Claude Code's `.claude-plugin/marketplace.json` (#103) or Codex's
//! `.agents/plugins/marketplace.json` (#104), beside the plugins it keeps
//! itself, as the catalogs people gather from keep theirs, and the
//! plugins an Agent Plugins package is (#104).

use serde_json::{json, Value};
use tempfile::TempDir;

use crate::marketplace::{AGENT_PLUGINS_SCHEMA, CATALOG, CODEX_CATALOG};
use crate::mother::artifact::SkillMother;
use crate::mother::fixture::GitRepositoryMother;
use crate::mother::random;

/// Makes marketplace repositories, each with one commit, to gather from.
pub struct MarketplaceMother;

impl MarketplaceMother {
    /// What anthropics/skills calls its catalog, which a gather of it calls
    /// its source.
    pub const ANTHROPIC_SKILLS: &'static str = "anthropic-agent-skills";

    /// The skills [`anthropic_skills`](Self::anthropic_skills)'s
    /// `document-skills` lists, each a folder of `skills/`.
    pub const DOCUMENT_SKILLS: [&'static str; 4] = ["xlsx", "docx", "pptx", "pdf"];

    /// The skills its `example-skills` lists.
    pub const EXAMPLE_SKILLS: [&'static str; 3] =
        ["algorithmic-art", "brand-guidelines", "skill-creator"];

    /// A marketplace repository whose catalog is `catalog`, beside `files`,
    /// each a path from its root and what it holds.
    pub fn create(catalog: &Value, files: &[(String, String)]) -> TempDir {
        Self::create_at(CATALOG, catalog, files)
    }

    /// A Codex marketplace repository (#104), its catalog `catalog` where
    /// Codex keeps one, beside `files`.
    pub fn codex(catalog: &Value, files: &[(String, String)]) -> TempDir {
        Self::create_at(CODEX_CATALOG, catalog, files)
    }

    /// A marketplace repository keeping `catalog` at `place`, a route from
    /// its root, beside `files`.
    pub fn create_at(place: &str, catalog: &Value, files: &[(String, String)]) -> TempDir {
        let text = serde_json::to_string_pretty(catalog).expect("a catalog is JSON");
        let mut all: Vec<(&str, &str)> = vec![(place, text.as_str())];
        all.extend(
            files
                .iter()
                .map(|(path, contents)| (path.as_str(), contents.as_str())),
        );
        GitRepositoryMother::create(&all)
    }

    /// A catalog called `name`, kept by somebody, listing `plugins`.
    pub fn catalog(name: &str, plugins: &[Value]) -> Value {
        json!({
            "name": name,
            "owner": {"name": random::word(), "email": "maintainers@example.com"},
            "metadata": {"description": SkillMother::description(), "version": "1.0.0"},
            "plugins": plugins,
        })
    }

    /// A marketplace modelled on anthropics/skills, as that repository keeps
    /// it: two plugins, each its whole repository (`./`), listing the
    /// skills it is under `skills/`, with what Claude Code reads and no
    /// gather needs (`strict`) beside them, and a skill neither lists. The
    /// document skills each bundle the licence they are under, as
    /// Anthropic's own do; the examples say none.
    pub fn anthropic_skills() -> TempDir {
        let listed = |names: &[&str]| -> Vec<String> {
            names
                .iter()
                .map(|name| format!("./skills/{name}"))
                .collect()
        };
        let catalog = json!({
            "name": Self::ANTHROPIC_SKILLS,
            "owner": {"name": "Keith Lazuka", "email": "klazuka@anthropic.com"},
            "metadata": {"description": "Anthropic example skills", "version": "1.0.0"},
            "plugins": [
                {
                    "name": "document-skills",
                    "description": "Collection of document processing suite including Excel, \
                                    Word, PowerPoint, and PDF capabilities",
                    "source": "./",
                    "strict": false,
                    "skills": listed(&Self::DOCUMENT_SKILLS),
                },
                {
                    "name": "example-skills",
                    "description": "Collection of example skills demonstrating various \
                                    capabilities",
                    "source": "./",
                    "strict": false,
                    "skills": listed(&Self::EXAMPLE_SKILLS),
                },
            ],
        });
        let mut files: Vec<(String, String)> = Vec::new();
        for name in Self::DOCUMENT_SKILLS {
            files.push((
                format!("skills/{name}/SKILL.md"),
                SkillMother::licensed(
                    name,
                    &format!("Work with {name} files"),
                    "Proprietary. LICENSE.txt has complete terms",
                ),
            ));
            files.push((
                format!("skills/{name}/LICENSE.txt"),
                String::from("(c) Anthropic, PBC. All rights reserved.\n"),
            ));
        }
        for name in Self::EXAMPLE_SKILLS {
            files.push((
                format!("skills/{name}/SKILL.md"),
                SkillMother::create(name, &SkillMother::description()),
            ));
        }
        files.push((
            String::from("template/SKILL.md"),
            SkillMother::create("template-skill", "Replace with a description"),
        ));
        files.push((
            String::from("README.md"),
            String::from("# Skills\n\nExamples of skills.\n"),
        ));
        Self::create(&catalog, &files)
    }

    /// A plugin's files at `folder` of its repository, a route from its
    /// root (`.` for the root itself): each of `skills` a folder of its
    /// `skills/`, as Claude Code finds a plugin's skills when its entry
    /// lists none.
    pub fn plugin(folder: &str, skills: &[&str]) -> Vec<(String, String)> {
        let at = |path: &str| match folder {
            "." => path.to_owned(),
            folder => format!("{folder}/{path}"),
        };
        skills
            .iter()
            .map(|name| {
                (
                    at(&format!("skills/{name}/SKILL.md")),
                    SkillMother::create(name, &format!("What {name} does")),
                )
            })
            .collect()
    }

    /// A catalog in Codex's dialect called `name`, listing `plugins`: what
    /// openai/plugins keeps, a name and a title, each entry given the
    /// `policy` and `category` Codex requires.
    pub fn codex_catalog(name: &str, plugins: &[Value]) -> Value {
        let plugins: Vec<Value> = plugins
            .iter()
            .map(|plugin| {
                let mut plugin = plugin.clone();
                plugin["policy"] = json!({
                    "installation": "AVAILABLE",
                    "authentication": random::pick(&["ON_INSTALL", "ON_USE"]),
                });
                plugin["category"] = json!(random::pick(&[
                    "Productivity",
                    "Developer Tools",
                    "Creativity"
                ]));
                plugin
            })
            .collect();
        json!({
            "name": name,
            "interface": {"displayName": random::word()},
            "plugins": plugins,
        })
    }

    /// A Codex `local` source at `path`, a route from the repository's root.
    pub fn local(path: &str) -> Value {
        json!({"source": "local", "path": format!("./{path}")})
    }

    /// What openai/plugins calls its catalog, which a gather of it calls its
    /// source.
    pub const OPENAI_CURATED: &'static str = "openai-curated";

    /// The skills [`openai_plugins`](Self::openai_plugins)'s `design`
    /// keeps, as figma's keeps its, each a folder of its `skills/` its
    /// manifest names.
    pub const DESIGN_SKILLS: [&'static str; 2] = ["design-to-code", "design-system-rules"];

    /// The skills its `meetings` keeps, as zoom's keeps its: one of them
    /// holding a skill of its own inside, part of it.
    pub const MEETING_SKILLS: [&'static str; 2] = ["meeting-sdk", "oauth"];

    /// A Codex marketplace modelled on openai/plugins, as that repository
    /// keeps it: its catalog at `.agents/plugins/marketplace.json`, every
    /// plugin a `local` source under `plugins/` with a
    /// `.codex-plugin/plugin.json`, and a skill of the repository's own in
    /// `.agents/skills`, which no plugin lists.
    ///
    /// - `design`, as figma: its manifest lists `./skills/`, declares its
    ///   licence and its app connectors, and it keeps agents, commands,
    ///   hooks, an `.mcp.json` and an `.app.json` beside its skills.
    /// - `tracker`, as linear: no skill, only its MCP servers, in the
    ///   `.mcp.json` its manifest names, and its app connectors.
    /// - `meetings`, as zoom: a manifest listing no skills, so its skills
    ///   are each folder of `skills/`, one with a skill deeper inside it.
    pub fn openai_plugins() -> TempDir {
        let catalog = Self::codex_catalog(
            Self::OPENAI_CURATED,
            &[
                json!({"name": "tracker", "source": Self::local("plugins/tracker")}),
                json!({"name": "design", "source": Self::local("plugins/design")}),
                json!({"name": "meetings", "source": Self::local("plugins/meetings")}),
            ],
        );
        let manifest = |value: Value| serde_json::to_string_pretty(&value).expect("JSON");
        let mcp = json!({"mcpServers": {"tracker": {"type": "http", "url": "https://mcp.example.com/mcp"}}})
            .to_string();
        let mut files: Vec<(String, String)> = vec![
            (
                String::from("plugins/design/.codex-plugin/plugin.json"),
                manifest(json!({
                    "name": "design",
                    "version": "2.0.20",
                    "description": "Design workflows",
                    "license": "LicenseRef-Design-Terms",
                    "skills": "./skills/",
                    "apps": "./.app.json",
                    "interface": {"displayName": "Design", "category": "Creativity"},
                })),
            ),
            (
                String::from("plugins/design/agents/parity-review.md"),
                String::from("# Parity review\n"),
            ),
            (
                String::from("plugins/design/commands/implement.md"),
                String::from("# Implement\n"),
            ),
            (
                String::from("plugins/design/hooks.json"),
                String::from("{}\n"),
            ),
            (String::from("plugins/design/.mcp.json"), mcp.clone()),
            (
                String::from("plugins/design/.app.json"),
                String::from("{}\n"),
            ),
            (
                String::from("plugins/tracker/.codex-plugin/plugin.json"),
                manifest(json!({
                    "name": "tracker",
                    "license": "MIT",
                    "mcpServers": "./.mcp.json",
                    "apps": "./.app.json",
                })),
            ),
            (String::from("plugins/tracker/.mcp.json"), mcp),
            (
                String::from("plugins/tracker/.app.json"),
                String::from("{}\n"),
            ),
            (
                String::from("plugins/meetings/.codex-plugin/plugin.json"),
                manifest(json!({"name": "meetings", "version": "1.0.0"})),
            ),
            (
                String::from("plugins/meetings/skills/meeting-sdk/web/SKILL.md"),
                SkillMother::create("meeting-sdk-web", "Build a meeting app for the web"),
            ),
            (
                String::from(".agents/skills/plugin-creator/SKILL.md"),
                SkillMother::create("plugin-creator", "Scaffold a plugin"),
            ),
            (
                String::from("README.md"),
                String::from("# Plugins\n\nCurated plugins.\n"),
            ),
        ];
        for name in Self::DESIGN_SKILLS {
            files.push((
                format!("plugins/design/skills/{name}/SKILL.md"),
                SkillMother::create(name, &format!("What {name} does")),
            ));
            files.push((
                format!("plugins/design/skills/{name}/references/api.md"),
                String::from("# API\n"),
            ));
        }
        for name in Self::MEETING_SKILLS {
            files.push((
                format!("plugins/meetings/skills/{name}/SKILL.md"),
                SkillMother::create(name, &format!("What {name} does")),
            ));
        }
        Self::codex(&catalog, &files)
    }

    /// An Agent Plugins package's files at `folder` of its repository (#104),
    /// called `name`: a root `plugin.json` declaring the format's
    /// `$schema` and the licence `licence`, each of `skills` a folder of
    /// its `skills/`, its MCP servers in `mcp.json`, and what the format
    /// never reads as a skill: a `SKILL.md` at its root and one a folder
    /// deeper in `skills/`, under a folder holding none.
    pub fn agent_plugins_package(
        folder: &str,
        name: &str,
        licence: &str,
        skills: &[&str],
    ) -> Vec<(String, String)> {
        let at = |path: &str| match folder {
            "." => path.to_owned(),
            folder => format!("{folder}/{path}"),
        };
        let mut files = vec![
            (
                at("plugin.json"),
                serde_json::to_string_pretty(&json!({
                    "$schema": AGENT_PLUGINS_SCHEMA,
                    "name": name,
                    "version": "1.0.0",
                    "description": SkillMother::description(),
                    "license": licence,
                }))
                .expect("JSON"),
            ),
            (
                at("mcp.json"),
                json!({
                    "$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
                    "mcpServers": {name: {"command": "npx", "args": ["-y", "@acme/server"]}},
                })
                .to_string(),
            ),
            (
                at("SKILL.md"),
                SkillMother::create(name, "The package itself, which is no skill of it"),
            ),
            (
                at("skills/grouped/deeper/SKILL.md"),
                SkillMother::create("deeper", "A skill a folder too deep"),
            ),
        ];
        files.extend(skills.iter().map(|skill| {
            (
                at(&format!("skills/{skill}/SKILL.md")),
                SkillMother::create(skill, &format!("What {skill} does")),
            )
        }));
        files
    }

    /// A marketplace listing `count` plugins, each a GitHub repository of
    /// its own pinned to a commit, as the directories listing thousands
    /// keep theirs: what a gather of it would fetch one by one.
    pub fn listing(count: usize) -> TempDir {
        let plugins: Vec<Value> = (0..count)
            .map(|n| {
                json!({
                    "name": format!("plugin-{n}"),
                    "description": SkillMother::description(),
                    "source": {
                        "source": "github",
                        "repo": format!("{}/plugin-{n}", random::word()),
                        "sha": random::hex(40),
                    },
                })
            })
            .collect();
        Self::create(&Self::catalog("plugin-directory", &plugins), &[])
    }
}
