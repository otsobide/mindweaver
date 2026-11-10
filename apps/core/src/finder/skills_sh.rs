//! skills.sh's search, `/api/search`: undocumented, and read as it answers
//! today, so unstable. Each skill comes with the repository it is in, as
//! GitHub's `owner/repo`, and how many times it was installed, but not the
//! folder it is in: a take gathers its whole repository, where a gather
//! finds it in whichever layout it is kept, beside every other skill there.

use serde::Deserialize;

use super::{below, count, entries, printable, Entry, Listed, PER_FINDER};
use crate::port::{HttpError, HttpRequest};

/// What a search of `query` asks at `base`.
pub(super) fn request(base: &str, query: &str) -> Result<HttpRequest, HttpError> {
    let limit = PER_FINDER.to_string();
    below(base, "api/search", &[("q", query), ("limit", &limit)])?
        .header("accept", "application/json")
}

#[derive(Deserialize)]
struct Answer {
    skills: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct Skill {
    name: Option<String>,
    #[serde(rename = "skillId")]
    skill_id: Option<String>,
    /// `owner/repo/skill`.
    id: Option<String>,
    source: Option<String>,
    installs: Option<serde_json::Value>,
}

/// Each skill listed, by its name, in the repository its `source` names,
/// or the first two parts of its `id` when it names none.
pub(super) fn read(answer: serde_json::Value) -> Result<Vec<Entry>, String> {
    let answer: Answer =
        serde_json::from_value(answer).map_err(|error| printable(&error.to_string()))?;
    Ok(entries(answer.skills, "name", |skill: Skill| {
        let from_id = skill.id.as_deref().and_then(|id| {
            let mut parts = id.splitn(3, '/');
            match (parts.next(), parts.next(), parts.next()) {
                (Some(owner), Some(repo), Some(_)) => Some(format!("{owner}/{repo}")),
                _ => None,
            }
        });
        Ok(Listed {
            name: skill.name.or(skill.skill_id),
            address: skill.source.or(from_id),
            installs: count(skill.installs.as_ref()),
            ..Listed::default()
        })
    }))
}
