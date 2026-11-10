//! GitHub's code search, for the `SKILL.md` files a query finds: the
//! documented REST API (`/search/code`), which answers no code search
//! without a token, so it is asked only with one, sent as a secret header,
//! which no redirect carries to another site and the HTTP cache keeps
//! nothing of.

use serde::Deserialize;

use super::{below, entries, printable, Entry, Listed, PER_FINDER};
use crate::port::{HttpError, HttpRequest, Token};

/// The version of the REST API asked for, so a later default changes
/// nothing read here.
const API_VERSION: &str = "2022-11-28";

/// What a search of `query` asks at `base`: the files named `SKILL.md` that
/// match it, the first page, sending `token` as a bearer one.
pub(super) fn request(
    base: &str,
    query: &str,
    token: Option<&Token>,
) -> Result<HttpRequest, HttpError> {
    let searched = format!("{query} filename:SKILL.md");
    let per_page = PER_FINDER.to_string();
    let request = below(
        base,
        "search/code",
        &[("q", &searched), ("per_page", &per_page)],
    )?
    .header("accept", "application/vnd.github+json")?
    .header("x-github-api-version", API_VERSION)?;
    match token {
        Some(token) => request.secret("authorization", &format!("Bearer {}", token.reveal())),
        None => Ok(request),
    }
}

#[derive(Deserialize)]
struct Answer {
    items: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct Item {
    path: String,
    repository: Repository,
}

#[derive(Deserialize)]
struct Repository {
    html_url: Option<String>,
    full_name: Option<String>,
}

/// Each file found, as the skill whose folder holds it: in the repository
/// GitHub shows it at, or its `owner/repo`, in the folder the `SKILL.md` is
/// in, named after that folder, or after the repository at its root.
///
/// A file by another name, which a search by a file's name finds in any
/// case, is no skill's, since a gather reads `SKILL.md` alone. One that
/// does not read is said by its path, which GitHub names every file found
/// by.
pub(super) fn read(answer: serde_json::Value) -> Result<Vec<Entry>, String> {
    let answer: Answer =
        serde_json::from_value(answer).map_err(|error| printable(&error.to_string()))?;
    Ok(entries(answer.items, "path", |item: Item| {
        let (folder, file) = item.path.rsplit_once('/').unwrap_or(("", &item.path));
        if file != "SKILL.md" {
            return Err(format!(
                "it is the file `{}`, not a `SKILL.md`",
                printable(file)
            ));
        }
        let repository = item.repository;
        Ok(Listed {
            name: folder
                .rsplit('/')
                .next()
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            address: repository.html_url.or(repository.full_name),
            path: Some(folder.to_owned()),
            ..Listed::default()
        })
    }))
}
