//! Unit tests for `paths.rs`, kept beside it rather than inside it.

use super::*;

fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}

#[test]
fn normalize_removes_single_dots() {
    assert_eq!(normalize(Path::new("/a/./b/./c")), p("/a/b/c"));
}

#[test]
fn normalize_resolves_parent_dirs() {
    assert_eq!(normalize(Path::new("/a/b/../c")), p("/a/c"));
    assert_eq!(normalize(Path::new("/a/b/c/../..")), p("/a"));
}

#[test]
fn normalize_never_climbs_past_the_root() {
    assert_eq!(normalize(Path::new("/../..")), p("/"));
    assert_eq!(normalize(Path::new("/a/../../b")), p("/b"));
}

#[test]
fn normalize_keeps_leading_parents_on_a_relative_path() {
    // Nothing to pop, so the `..` has to survive or the path changes
    // meaning entirely.
    assert_eq!(normalize(Path::new("../a")), p("../a"));
    assert_eq!(normalize(Path::new("../../a")), p("../../a"));
    assert_eq!(normalize(Path::new("a/../../b")), p("../b"));
}

#[test]
fn a_child_is_reached_without_climbing() {
    assert_eq!(
        relative_to(Path::new("/work/collapse"), Path::new("/work")),
        Some(p("collapse"))
    );
    assert_eq!(
        relative_to(Path::new("/work/a/b/c"), Path::new("/work")),
        Some(p("a/b/c"))
    );
}

#[test]
fn a_sibling_is_reached_by_climbing_once() {
    assert_eq!(
        relative_to(Path::new("/work/collapse"), Path::new("/work/mindweaver")),
        Some(p("../collapse"))
    );
}

#[test]
fn a_distant_relative_climbs_as_far_as_it_must() {
    assert_eq!(
        relative_to(Path::new("/other/deep/thing"), Path::new("/work/a/b")),
        Some(p("../../../other/deep/thing"))
    );
}

#[test]
fn the_same_directory_is_a_single_dot() {
    assert_eq!(
        relative_to(Path::new("/work"), Path::new("/work")),
        Some(p("."))
    );
}

#[test]
fn the_route_is_computed_after_normalising_both_sides() {
    assert_eq!(
        relative_to(Path::new("/work/./x/../collapse"), Path::new("/work/a/..")),
        Some(p("collapse"))
    );
}

#[test]
fn there_is_no_route_between_an_absolute_and_a_relative_path() {
    assert_eq!(relative_to(Path::new("/work/a"), Path::new("work")), None);
    assert_eq!(relative_to(Path::new("work"), Path::new("/work/a")), None);
}

#[test]
fn a_route_round_trips_back_to_its_target() {
    // The property that matters: joining the route onto the base has to
    // land on the target, because that is exactly what reading the config
    // back does.
    let cases = [
        ("/work/collapse", "/work"),
        ("/work/collapse", "/work/mindweaver"),
        ("/other/deep/thing", "/work/a/b"),
        ("/work", "/work"),
    ];
    for (target, base) in cases {
        let route = relative_to(Path::new(target), Path::new(base)).unwrap();
        assert_eq!(
            normalize(&Path::new(base).join(&route)),
            normalize(Path::new(target)),
            "{base} + {} should reach {target}",
            route.display()
        );
    }
}

#[test]
fn config_strings_use_forward_slashes() {
    assert_eq!(to_config_string(Path::new("a/b")), Some("a/b".to_owned()));
    assert_eq!(to_config_string(Path::new("..")), Some("..".to_owned()));
}

#[test]
fn a_route_joined_under_a_base_is_spelled_with_the_platform_s_separator_throughout() {
    let base = std::env::temp_dir();

    let joined = joined(&base, ".mindweaver/library/skills");

    assert_eq!(
        joined.to_string_lossy(),
        base.join(".mindweaver")
            .join("library")
            .join("skills")
            .to_string_lossy()
    );
}

#[test]
fn an_absolute_route_replaces_the_base_as_join_would() {
    let base = std::env::temp_dir();
    let absolute = base.join("elsewhere");

    assert_eq!(joined(Path::new("work"), &absolute), absolute);
}
