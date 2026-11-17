//! Lexical path arithmetic: normalising a path, and working out the route
//! from one to another.
//!
//! Everything here is **lexical** but [`canonical`]. Nothing else touches
//! the disk and nothing else resolves symlinks, which is deliberate twice
//! over: a workspace can register a project directory that does not exist
//! yet, and a path that travels through a symlink keeps the shape the user
//! typed rather than the shape the filesystem prefers. The cost is the usual
//! one: `a/b/../c` is treated as `a/c`, which differs from what the kernel
//! would do if `a/b` were a symlink.

use std::path::{Component, Path, PathBuf};

use serde::Serializer;

/// `path` as the file system resolves it, every link followed and every `.`
/// and `..` taken out, spelled on Windows as a person types a path there
/// (`C:\Users\...`) rather than as the verbatim `\\?\C:\Users\...`
/// `std::fs::canonicalize` gives: a message would show that prefix, and a
/// path resolved one way never equals one resolved the other, since their
/// prefixes differ as components. Every resolution in core, and in a test
/// of one, goes through here, so both sides of a comparison are spelled
/// alike. `dunce` keeps the prefix where dropping it would change what the
/// path names (past `MAX_PATH`, a device's name); elsewhere it is
/// `std::fs::canonicalize` itself.
pub fn canonical(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    dunce::canonicalize(path)
}

/// `route`, a route a marker or a setting keeps with `/` on every platform,
/// under `base`, spelled with the platform's separator throughout: `join`
/// keeps a route's `/` as it is, which Windows reads but which a path
/// shown there would then mix, `C:\work\.mindweaver/library/skills\deploy`.
/// An absolute `route` replaces `base`, as with `join`.
pub fn joined(base: &Path, route: impl AsRef<Path>) -> PathBuf {
    base.join(route.as_ref().components().collect::<PathBuf>())
}

/// Resolve `.` and `..` without touching the disk.
///
/// `..` at the root is dropped rather than escaping it, so the result of
/// normalising an absolute path is always still absolute.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                // A directory name is what `..` cancels out.
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                // At a root there is nowhere to climb to, so the `..` is
                // dropped. Popping would turn an absolute path into a
                // relative one, which is never what it means.
                Some(Component::Prefix(_) | Component::RootDir) => {}
                // Nothing to cancel: on a relative path the `..` is part of
                // what the path means, and stacking them has to keep working
                // (`../..` is two levels, not one).
                Some(Component::ParentDir | Component::CurDir) | None => out.push(".."),
            },
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The route from `base` to `target`, using `..` to climb where needed.
///
/// Both paths are normalised first. Returns `None` when there is no route at
/// all: two absolute paths on different Windows drives, or a relative path
/// compared against an absolute one.
pub fn relative_to(target: &Path, base: &Path) -> Option<PathBuf> {
    let target = normalize(target);
    let base = normalize(base);

    let mut target_parts = target.components().peekable();
    let mut base_parts = base.components().peekable();

    // Walk off the shared prefix; what is left of each side is the route.
    while let (Some(t), Some(b)) = (target_parts.peek(), base_parts.peek()) {
        if t != b {
            break;
        }
        target_parts.next();
        base_parts.next();
    }

    // A root or prefix left on either side means the two paths never met:
    // different Windows drives, or one absolute and one relative. Checking
    // both sides matters: an unmatched root on the *target* would otherwise
    // be pushed onto the route and silently reset it to an absolute path.
    let target_rest: Vec<Component> = target_parts.collect();
    let unrooted = |c: &Component| !matches!(c, Component::Prefix(_) | Component::RootDir);
    if !target_rest.iter().all(unrooted) {
        return None;
    }

    let mut route = PathBuf::new();
    for component in base_parts {
        match component {
            // Every directory still to be left behind is one `..` to climb.
            Component::Normal(_) => route.push(".."),
            Component::Prefix(_) | Component::RootDir => return None,
            // A leading `..` normalize() could not resolve: where that lands
            // depends on a working directory this function does not have.
            Component::CurDir | Component::ParentDir => return None,
        }
    }
    for component in target_rest {
        route.push(component.as_os_str());
    }

    // The two are the same directory. `.` says so; an empty path says nothing
    // and would be written to a config file as "".
    if route.as_os_str().is_empty() {
        route.push(".");
    }
    Some(route)
}

/// A path as it should be written into a config file: `/` separated, so a
/// workspace registered on one platform still resolves on the other.
///
/// `None` when the path is not valid UTF-8, which TOML cannot represent and
/// this refuses to mangle.
pub fn to_config_string(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    Some(if std::path::MAIN_SEPARATOR == '/' {
        text.to_owned()
    } else {
        text.replace(std::path::MAIN_SEPARATOR, "/")
    })
}

/// A route as a marker spells it, `/` separated, or as it displays when it
/// is not UTF-8, which no marker holds but a report still has to say: a
/// workspace's entry as `-p` and `mindweaver unlink` take it, or a folder a
/// project's settings name.
pub(crate) fn as_route(route: &Path) -> String {
    to_config_string(route).unwrap_or_else(|| route.display().to_string())
}

/// A path as text in a report's JSON, whatever bytes it holds: a report that
/// failed over one unprintable path would say nothing about the rest.
pub(crate) fn lossy<S: Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&path.to_string_lossy())
}

/// Each of `paths` as [`lossy`] writes one.
pub(crate) fn lossy_all<S: Serializer>(
    paths: &[PathBuf],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(paths.iter().map(|path| path.to_string_lossy()))
}

#[cfg(test)]
#[path = "paths_test.rs"]
mod tests;
