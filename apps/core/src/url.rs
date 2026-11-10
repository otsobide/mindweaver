//! What may be kept of a URL somebody typed.
//!
//! A module of its own rather than a helper inside [`crate::adapter::gix`],
//! because the question is asked wherever a URL is stored, logged or shown,
//! not only where a clone fails.

use std::borrow::Cow;

/// `text` with every URL in it stripped of the credentials its userinfo
/// carries, so a source can be stored, logged and named in an error without
/// repeating a token somebody typed into it.
///
/// A whole URL is a text with one URL in it. Error text is the other kind:
/// `gix` quotes the URL it failed on among its own words, and a URL git config
/// rewrote with `insteadOf` can carry a credential the typed one did not.
///
/// What goes is everything before the `@` of a `scheme://` URL, except the
/// user name of an ssh URL. That one is who ssh logs in as, which is part of
/// where the repository is rather than a secret, and a later gather from the
/// stored URL needs it. Over http(s) a user name goes too: GitHub takes a
/// token in that place, and a credential helper is what names the user. The
/// scp-like form (`git@host:path`) is left alone, because it has no room for a
/// password: `gix` reads everything before its first `:` as user and host.
///
/// The userinfo is looked for up to the first `/` or whitespace after the
/// `://`, which for an http(s) URL with a `?` or `#` before any `/` is further
/// than `gix` reads it: cutting too much from an odd URL is the safe way to be
/// wrong. Parsing with `gix::url` and dropping the user was the other way, and
/// is rejected because the parser also rewrites what it keeps (a default port
/// dropped, a scheme lowercased), so the error would name a URL nobody typed,
/// and because it cannot find a URL inside an error message.
///
/// A password typed with a `/` in it is the one place the first `/` is too
/// soon: `https://me:p/ss@host` reads to every parser as host `me` and port
/// `p`, which no URL can have, so it is refused as unparseable and would be
/// stored, logged and repeated whole. When what comes before the first `/`
/// has a `:` followed by anything but a port's digits, the `@` is looked for
/// up to the next whitespace instead. A password of digits alone cut by a `/`
/// still reads as a host, a port and a path, to `gix` as to this.
pub fn redacted(text: &str) -> Cow<'_, str> {
    let mut kept = String::new();
    // How much of `text` is already in `kept`.
    let mut copied = 0;
    let mut from = 0;
    while let Some(found) = text[from..].find("://") {
        let separator = from + found;
        let start = separator + "://".len();
        let scheme = scheme_before(text, separator);
        let rest = &text[start..];
        let end = rest
            .find(|c: char| c == '/' || c.is_whitespace())
            .unwrap_or(rest.len());

        let at = match rest[..end].rfind('@') {
            None if password_goes_on(scheme, &rest[..end]) => {
                let line = rest.find(char::is_whitespace).unwrap_or(rest.len());
                rest[..line].rfind('@')
            }
            at => at,
        };
        let Some(at) = at else {
            from = start + end;
            continue;
        };
        from = start + end.max(at + 1);
        let userinfo = &rest[..at];
        let user = userinfo.split_once(':').map_or(userinfo, |(user, _)| user);
        let login = is_ssh(scheme) && !user.is_empty();
        if login && user == userinfo {
            continue;
        }

        kept.push_str(&text[copied..start]);
        if login {
            kept.push_str(user);
            kept.push('@');
        }
        copied = start + at + 1;
    }

    if copied == 0 {
        Cow::Borrowed(text)
    } else {
        kept.push_str(&text[copied..]);
        Cow::Owned(kept)
    }
}

/// Whether `url` carries anything [`redacted`] would take out of it: a
/// password, or a user name anywhere but in front of an ssh host.
pub fn carries_credentials(url: &str) -> bool {
    matches!(redacted(url), Cow::Owned(_))
}

/// The hosts whose repositories are one identity whichever transport names
/// them: `https://`, `git@host:` and `ssh://` reach the same repository
/// there, and the forge reads an owner's and a repository's name in any
/// case and with or without `.git`.
///
/// Only these two, because they are the ones that promise it: on a host of
/// somebody's own, `host:path` is a path from the login's home where
/// `ssh://host/path` is one from the root, and `x` and `x.git` can be two
/// repositories.
const FORGES: [&str; 2] = ["github.com", "gitlab.com"];

/// The URL a gathered source is known by: one identity for every spelling of
/// one repository's address, and no credential in it, so two spellings
/// gather into one source and the identity can be stored and shown.
///
/// On GitHub and GitLab ([`FORGES`]) every transport is one identity,
/// spelled `https://<host>/<path>` in lowercase, without `.git`, a port or a
/// trailing `/`: `git@github.com:Acme/Skills.git`,
/// `ssh://git@github.com/acme/skills` and `https://github.com/acme/skills/`
/// are `https://github.com/acme/skills`, and so is `acme/skills`, the
/// `owner/repo` a person types for a GitHub repository ([`fetched`]).
/// Elsewhere only what cannot tell two repositories apart goes: any
/// credential, the case of the scheme and the host, a trailing `/`, and
/// `file://` in front of a path, which names the same folder without it. A
/// relative path stays as it was typed, its own identity, since which
/// folder it names depends on where it was typed, which a rule of text
/// cannot know.
///
/// [`redacted`] first, so a credential is out of it however oddly it was
/// typed. The user ssh logs in as goes only on a forge, where it is always
/// `git` and names no repository: elsewhere it stays, as [`redacted`] keeps
/// it, since `alice@host:skills` and `bob@host:skills` are folders in two
/// homes, and one repository two logins reach is two sources, as it was
/// before identities, rather than two repositories one. Parsing with
/// `gix::url` was the other way, rejected for the reasons [`redacted`]
/// gives, and because what is kept here is a rule of Mindweaver's, not how
/// one library reads a URL.
pub fn canonical(url: &str) -> String {
    let url = fetched(&redacted(url)).into_owned();
    if let Some((scheme, rest)) = url.split_once("://") {
        let scheme = scheme.to_ascii_lowercase();
        if scheme == "file" {
            // `file:///srv/x` and `file://localhost/srv/x` are `/srv/x`.
            let path = rest.strip_prefix("localhost").unwrap_or(rest);
            if path.starts_with('/') {
                return trimmed(path).to_owned();
            }
            return format!("file://{}", trimmed(rest));
        }
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let (login, host) = logged_in(authority);
        if let Some(forge) = forge(&host) {
            return forged(forge, path);
        }
        let path = trimmed(path);
        return if path.is_empty() {
            format!("{scheme}://{login}{host}")
        } else {
            format!("{scheme}://{login}{host}/{path}")
        };
    }
    if let Some((before, path)) = scp_like(&url) {
        let (login, host) = logged_in(before);
        if let Some(forge) = forge(&host) {
            return forged(forge, path);
        }
        return format!("{login}{host}:{}", trimmed(path));
    }
    trimmed(&url).to_owned()
}

/// What a gather fetches for `url`: `https://github.com/<owner>/<repo>` for
/// the `owner/repo` a person types for a GitHub repository, and `url` as it
/// was typed otherwise.
///
/// `owner/repo` is two plain names with one `/` between them, the first not
/// starting with a `.`: a folder on this machine is a path that says so,
/// `./skills/repo` when it is relative, as `gh` and Claude Code's plugin
/// marketplaces read it. Reading `owner/repo` as a folder whenever one of
/// that name happened to be there was the other way, and is rejected: which
/// repository a command names would then depend on where it ran.
pub fn fetched(url: &str) -> Cow<'_, str> {
    match url.split_once('/') {
        Some((owner, repo))
            if is_plain(owner) && is_plain(repo) && !owner.starts_with('.') && repo != ".." =>
        {
            Cow::Owned(format!("https://github.com/{owner}/{repo}"))
        }
        _ => Cow::Borrowed(url),
    }
}

/// Whether `part` is a name a forge takes for an owner or a repository:
/// letters, digits, `.`, `_` and `-`, at least one of them.
fn is_plain(part: &str) -> bool {
    !part.is_empty()
        && part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// The forge `host` is, without the port it may name.
fn forge(host: &str) -> Option<&'static str> {
    let host = host.split_once(':').map_or(host, |(host, _)| host);
    FORGES.into_iter().find(|forge| *forge == host)
}

/// A repository at `path` on `forge`, as [`canonical`] spells it there.
fn forged(forge: &str, path: &str) -> String {
    let path = trimmed(path.trim_start_matches('/'));
    let path = path.strip_suffix(".git").unwrap_or(path);
    format!("https://{forge}/{}", path.to_ascii_lowercase())
}

/// `path` without the `/`s it ends with, unless it is nothing else.
fn trimmed(path: &str) -> &str {
    match path.trim_end_matches('/') {
        "" if path.starts_with('/') => "/",
        kept => kept,
    }
}

/// The login, `user@` or nothing, and the host in lowercase, of the text
/// before an address's path: after [`redacted`], the only userinfo left is
/// the user ssh logs in as.
fn logged_in(authority: &str) -> (&str, String) {
    match authority.rsplit_once('@') {
        Some((user, host)) => (&authority[..=user.len()], host.to_ascii_lowercase()),
        None => ("", authority.to_ascii_lowercase()),
    }
}

/// The text before the path, `[user@]host`, and the path of an scp-like
/// address: what `gix` reads as one when the `:` comes before any `/`. A
/// single letter before it is a Windows drive, not a host.
fn scp_like(url: &str) -> Option<(&str, &str)> {
    let (before, path) = url.split_once(':')?;
    if before.contains('/') || before.contains('\\') {
        return None;
    }
    let host = before.rsplit_once('@').map_or(before, |(_, host)| host);
    (host.len() > 1).then_some((before, path))
}

/// Whether `authority`, a URL's text from its `://` to its first `/`, is a
/// user and the start of a password a `/` cut short rather than a host: a
/// `:` with something other than a port's digits after it.
///
/// An IPv6 host, in brackets, is full of `:` and is a host all the same. A
/// `file://` URL has no userinfo to cut, and on Windows its drive letter
/// comes with a `:` and nothing after it.
fn password_goes_on(scheme: &str, authority: &str) -> bool {
    if scheme.eq_ignore_ascii_case("file") || authority.starts_with('[') {
        return false;
    }
    authority
        .split_once(':')
        .is_some_and(|(_, port)| port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()))
}

/// The scheme written just before the `://` at `separator`.
fn scheme_before(text: &str, separator: usize) -> &str {
    let head = &text[..separator];
    let start = head
        .trim_end_matches(|c: char| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        .len();
    &head[start..]
}

/// Whether a scheme names the ssh transport, legacy spellings included, as
/// `gix` reads them.
fn is_ssh(scheme: &str) -> bool {
    ["ssh", "git+ssh", "ssh+git"]
        .iter()
        .any(|ssh| scheme.eq_ignore_ascii_case(ssh))
}

#[cfg(test)]
#[path = "url_test.rs"]
mod tests;
