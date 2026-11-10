//! Unit tests for `time.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn a_registry_time_is_read_as_the_seconds_and_nanoseconds_it_names() {
    assert_eq!(instant("1970-01-01T00:00:00Z"), Some((0, 0)));
    assert_eq!(
        instant("2026-11-10T11:00:47.040903Z"),
        Some((1_794_308_447, 40_903_000))
    );
    assert_eq!(
        instant("2026-10-26T20:15:18.12136Z"),
        Some((1_793_045_718, 121_360_000))
    );
    assert_eq!(instant("2000-02-29T12:00:00Z"), Some((951_825_600, 0)));
    assert_eq!(instant("1969-12-31T23:59:59Z"), Some((-1, 0)));
}

/// The reason it is read at all: Go writes a fraction as short as it can,
/// so the later of two times can sort first as text.
#[test]
fn a_time_with_a_shorter_fraction_is_ordered_by_when_it_is_not_by_its_text() {
    let (shorter, longer) = ("2026-10-26T20:15:18.7Z", "2026-10-26T20:15:18.72Z");

    assert!(shorter > longer, "as text, the shorter sorts after");
    assert!(instant(shorter) < instant(longer));
}

#[test]
fn an_offset_is_the_same_instant_in_utc() {
    assert_eq!(
        instant("2026-11-10T12:30:00+01:30"),
        instant("2026-11-10T11:00:00Z")
    );
    assert_eq!(
        instant("2026-11-10T05:00:00-06:00"),
        instant("2026-11-10T11:00:00z")
    );
    assert_eq!(
        instant("2026-11-10t11:00:00Z"),
        instant("2026-11-10T11:00:00Z")
    );
}

#[test]
fn a_fraction_past_nanoseconds_is_cut_rather_than_refused() {
    assert_eq!(
        instant("2026-11-10T11:00:00.1234567891234Z"),
        Some((1_794_308_400, 123_456_789))
    );
}

#[test]
fn what_is_no_rfc_3339_time_is_no_time() {
    for text in [
        "",
        "2026-11-10",
        "2026-11-10 11:00:00Z",
        "2026-11-10T11:00:00",
        "2026-11-10T11:00:00.Z",
        "2026-11-10T11:00Z",
        "2026-13-10T11:00:00Z",
        "2026-11-32T11:00:00Z",
        "2026-11-10T24:00:00Z",
        "2026-11-10T11:60:00Z",
        "2026-11-10T11:00:00+0100",
        "2026-11-10T11:00:00+25:00",
        "2026-11-10T11:00:00Zulu",
        "yesterday",
        "2026-11-10T11:00:00\u{e9}",
    ] {
        assert_eq!(instant(text), None, "{text:?}");
    }
}
