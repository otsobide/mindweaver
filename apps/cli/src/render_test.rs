//! Unit tests for `render.rs`, kept beside it rather than inside it.

use super::*;

/// Each moment as Python's `datetime.fromtimestamp(s, UTC)` says it: the
/// epoch, either side of it, a leap day of a year divisible by 400, one of
/// a year divisible by 4, the last minute of a year, and a century's first
/// day, which is no leap year's.
#[test]
fn a_time_the_ledger_stamps_is_said_as_its_day_and_minute_in_utc() {
    for (seconds, said) in [
        (0, "1970-01-01 00:00 UTC"),
        (-1, "1969-12-31 23:59 UTC"),
        (951_782_400, "2000-02-29 00:00 UTC"),
        (951_868_799, "2000-02-29 23:59 UTC"),
        (1_709_210_400, "2024-02-29 12:40 UTC"),
        (1_735_689_599, "2024-12-31 23:59 UTC"),
        (1_790_000_000, "2026-09-21 14:13 UTC"),
        (4_102_444_800, "2100-01-01 00:00 UTC"),
    ] {
        assert_eq!(utc(seconds), said, "{seconds}");
    }
}

/// A later moment is said later, read as text, through four years, a leap
/// year among them, a moment every sixteen hours or so: what no table of
/// moments holds for every day.
#[test]
fn a_later_moment_is_said_later_through_four_years() {
    let start = 1_704_067_200; // 2024-01-01 00:00 UTC
    let mut was = utc(start - 60);
    for minute in (0..4 * 366 * 24 * 60).step_by(997) {
        let now = utc(start + minute * 60);
        assert!(now > was, "{now} after {was}");
        was = now;
    }
}
