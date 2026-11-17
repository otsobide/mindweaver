//! Route arithmetic held as properties over paths nobody spelled: every
//! registry entry, exclude line and copy's place is computed by these few
//! functions, so a change that breaks one case of them moves where things
//! are written for every caller at once.
//!
//! The unit tests beside `paths.rs` name the cases; these draw paths from
//! segments of every shape a route can hold (names, `.`, `..`) and hold what
//! must be true of all of them.

use std::path::{Component, Path, PathBuf};

use mindweaver_core::mother::random;
use mindweaver_core::paths::{normalize, relative_to, to_config_string};

const DRAWS: usize = 500;

/// A folder name, sometimes with a dot or a space in it, which are names
/// like any other to the arithmetic.
fn name() -> String {
    random::pick(&[
        random::word(),
        random::kebab(),
        format!("{}.{}", random::word(), random::word()),
        format!("{} {}", random::word(), random::word()),
        format!(".{}", random::word()),
    ])
}

/// A route of names alone, at least one.
fn names() -> PathBuf {
    (0..random::between(1, 4)).map(|_| name()).collect()
}

/// A route that may hold `.` and `..` anywhere among its names.
fn tangled() -> PathBuf {
    (0..random::between(0, 6))
        .map(|_| match random::between(0, 5) {
            0 => String::from("."),
            1 => String::from(".."),
            _ => name(),
        })
        .collect()
}

/// An absolute path: the root and a route below it. On Windows a root
/// alone, `\b`, is relative to the drive, and only a path with one is
/// absolute.
fn absolute() -> PathBuf {
    let root = if cfg!(windows) { r"C:\" } else { "/" };
    Path::new(root).join(tangled())
}

fn has(path: &Path, what: fn(&Component) -> bool) -> bool {
    path.components().any(|component| what(&component))
}

#[test]
fn normalizing_twice_is_normalizing_once() {
    for _ in 0..DRAWS {
        let path = random::pick(&[tangled(), absolute()]);
        let once = normalize(&path);

        assert_eq!(normalize(&once), once, "{path:?}");
    }
}

#[test]
fn a_normalized_path_holds_no_dot_and_no_parent_after_a_name() {
    for _ in 0..DRAWS {
        let path = random::pick(&[tangled(), absolute()]);
        let normal = normalize(&path);

        assert!(
            !has(&normal, |c| matches!(c, Component::CurDir)),
            "{path:?} gave {normal:?}"
        );
        // Every `..` left stands before every name: one after a name would
        // have cancelled it.
        let parts: Vec<Component> = normal.components().collect();
        let first_name = parts
            .iter()
            .position(|c| matches!(c, Component::Normal(_)))
            .unwrap_or(parts.len());
        assert!(
            parts[first_name..]
                .iter()
                .all(|c| !matches!(c, Component::ParentDir)),
            "{path:?} gave {normal:?}"
        );
    }
}

#[test]
fn an_absolute_path_stays_absolute_however_far_it_climbs() {
    for _ in 0..DRAWS {
        let path = absolute();
        let normal = normalize(&path);

        assert!(normal.is_absolute(), "{path:?} gave {normal:?}");
        assert!(
            !has(&normal, |c| matches!(c, Component::ParentDir)),
            "{path:?} gave {normal:?}"
        );
    }
}

#[test]
fn names_alone_are_already_normal() {
    for _ in 0..DRAWS {
        let path = names();

        assert_eq!(normalize(&path), path);
        assert_eq!(
            normalize(&Path::new("/").join(&path)),
            Path::new("/").join(&path)
        );
    }
}

#[test]
fn a_name_and_the_parent_after_it_cancel_out() {
    for _ in 0..DRAWS {
        let (before, name, after) = (names(), name(), names());

        let path = before.join(&name).join("..").join(&after);

        assert_eq!(normalize(&path), before.join(&after), "{path:?}");
    }
}

#[test]
fn a_route_from_a_base_leads_back_to_the_target() {
    for _ in 0..DRAWS {
        let base = normalize(&absolute());
        let target = normalize(&absolute());

        let route =
            relative_to(&target, &base).unwrap_or_else(|| panic!("{target:?} from {base:?}"));

        assert!(route.is_relative(), "{route:?}");
        assert_eq!(
            normalize(&base.join(&route)),
            target,
            "{target:?} from {base:?}"
        );
    }
}

#[test]
fn a_target_inside_its_base_is_reached_without_climbing() {
    for _ in 0..DRAWS {
        let base = normalize(&absolute());
        let inside = names();

        let route = relative_to(&base.join(&inside), &base).unwrap();

        assert_eq!(route, inside, "{inside:?} in {base:?}");
    }
}

#[test]
fn a_route_climbs_once_for_each_folder_of_the_base_it_leaves() {
    for _ in 0..DRAWS {
        let shared = normalize(&absolute());
        let (left, reached) = (names(), names());

        let route = relative_to(&shared.join(&reached), &shared.join(&left)).unwrap();

        let climbs = route
            .components()
            .take_while(|c| matches!(c, Component::ParentDir))
            .count();
        // What the two have in common past `shared` is not climbed: left and
        // reached may start with one name by chance.
        let common = left
            .components()
            .zip(reached.components())
            .take_while(|(a, b)| a == b)
            .count();
        assert_eq!(
            climbs,
            left.components().count() - common,
            "{reached:?} from {left:?}"
        );
    }
}

#[test]
fn a_directory_is_reached_from_itself_by_a_single_dot() {
    for _ in 0..DRAWS {
        let path = random::pick(&[absolute(), names()]);

        assert_eq!(
            relative_to(&path, &path),
            Some(PathBuf::from(".")),
            "{path:?}"
        );
    }
}

#[test]
fn no_route_joins_an_absolute_path_and_a_relative_one() {
    for _ in 0..DRAWS {
        let (absolute, relative) = (absolute(), names());

        assert_eq!(
            relative_to(&absolute, &relative),
            None,
            "{absolute:?} {relative:?}"
        );
        assert_eq!(
            relative_to(&relative, &absolute),
            None,
            "{relative:?} {absolute:?}"
        );
    }
}

#[test]
fn a_base_that_climbs_past_where_it_starts_has_no_route_from_it() {
    for _ in 0..DRAWS {
        let base = Path::new("..").join(names());

        assert_eq!(relative_to(&names(), &base), None, "{base:?}");
    }
}

#[test]
fn a_config_string_is_the_path_with_forward_slashes_and_nothing_else_changed() {
    for _ in 0..DRAWS {
        let path = random::pick(&[tangled(), absolute()]);

        let written = to_config_string(&path).unwrap();

        assert!(!written.contains('\\'), "{written}");
        assert_eq!(Path::new(&written), path, "{path:?}");
    }
}

#[cfg(unix)]
#[test]
fn a_path_that_is_no_utf8_has_no_config_string() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let path = Path::new(OsStr::from_bytes(b"/work/\xffdeploy"));

    assert_eq!(to_config_string(path), None);
}
