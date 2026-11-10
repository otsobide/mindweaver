//! Mothers of Claude Code plugin marketplaces (#103): a git repository
//! keeping a catalog, `.claude-plugin/marketplace.json`, beside the plugins
//! it keeps itself, as the catalogs people gather from keep theirs.

use serde_json::{json, Value};
use tempfile::TempDir;

use crate::marketplace::CATALOG;
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
        let text = serde_json::to_string_pretty(catalog).expect("a catalog is JSON");
        let mut all: Vec<(&str, &str)> = vec![(CATALOG, text.as_str())];
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
