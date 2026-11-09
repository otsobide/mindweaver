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

/// Whether `url` is reached over ssh: a scheme naming it, legacy spellings
/// included, or the scp-like form, `[user@]host:path`, whose `:` comes
/// before any `/`.
///
/// What a token is never sent over: ssh logs in with keys of its own, and a
/// token handed over for such a URL would be for nothing, so it is refused
/// rather than ignored. A Windows drive letter (`C:\repos`) is a path.
pub fn over_ssh(url: &str) -> bool {
    if let Some((scheme, _)) = url.split_once("://") {
        return is_ssh(scheme);
    }
    match url.find(':') {
        Some(colon) => colon > 1 && !url[..colon].contains(['/', '\\']),
        None => false,
    }
}

/// Whether `url` is a path on this machine rather than an address: no
/// scheme, and not the scp-like form ssh reads ([`over_ssh`]). What a front
/// end resolves against the folder a command was typed in, since `git`
/// would resolve it against wherever it runs.
pub fn is_path(url: &str) -> bool {
    !url.contains("://") && !over_ssh(url)
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
