//! api.claude-plugins.dev's search of the skills it lists,
//! `/api/skills/search`: undocumented, and read as it answers today, so
//! unstable. Each skill comes with where it is on GitHub, the address of
//! its folder, which names the branch too, the folder again in its
//! metadata, how many times it was installed and its repository's stars.

use serde::Deserialize;

use super::{below, count, entries, printable, Entry, Listed, PER_FINDER};
use crate::port::{HttpError, HttpRequest};

/// What a search of `query` asks at `base`.
pub(super) fn request(base: &str, query: &str) -> Result<HttpRequest, HttpError> {
    let limit = PER_FINDER.to_string();
    below(
        base,
        "api/skills/search",
        &[("q", query), ("limit", &limit)],
    )?
    .header("accept", "application/json")
}

#[derive(Deserialize)]
struct Answer {
    skills: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct Skill {
    name: Option<String>,
    #[serde(rename = "sourceUrl")]
    source_url: Option<String>,
    stars: Option<serde_json::Value>,
    installs: Option<serde_json::Value>,
    metadata: Option<Metadata>,
}

#[derive(Deserialize)]
struct Metadata {
    #[serde(rename = "repoOwner")]
    repo_owner: Option<String>,
    #[serde(rename = "repoName")]
    repo_name: Option<String>,
    #[serde(rename = "directoryPath")]
    directory_path: Option<String>,
}

/// Each skill listed, at the address of its folder, or, where it gives
/// none, in the repository its metadata names; in the folder its metadata
/// names, or the one that address does.
pub(super) fn read(answer: serde_json::Value) -> Result<Vec<Entry>, String> {
    let answer: Answer =
        serde_json::from_value(answer).map_err(|error| printable(&error.to_string()))?;
    Ok(entries(answer.skills, "name", |skill: Skill| {
        let (repository, path) = match skill.metadata {
            Some(metadata) => (
                metadata
                    .repo_owner
                    .zip(metadata.repo_name)
                    .map(|(owner, repo)| format!("{owner}/{repo}")),
                metadata.directory_path,
            ),
            None => (None, None),
        };
        Ok(Listed {
            name: skill.name,
            address: skill.source_url.or(repository),
            path,
            installs: count(skill.installs.as_ref()),
            stars: count(skill.stars.as_ref()),
        })
    }))
}
