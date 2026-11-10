//! The registry's times, read so that two can be told apart in order, and
//! the clock's written as the registry writes one.
//!
//! The registry writes RFC 3339 as Go does, its fraction of a second as
//! long as it needs and no longer (`2026-10-26T20:15:18.12136Z` beside
//! `2026-06-26T14:07:53.723164Z`), so two cannot be ordered as text: `.7Z`
//! sorts after `.72Z` though it is the earlier. Read here instead, with no
//! date library, as the ledger keeps its times with none ([`crate::port::Clock`]).

/// `text` as seconds since the Unix epoch and the nanoseconds past them,
/// when it is a date and time as RFC 3339 writes one: a date, `T`, a time
/// to the second, any fraction of one, and `Z` or an offset from UTC.
///
/// `None` for anything else, which is then no time to order by: a
/// registry's answer is read whatever it holds.
pub(crate) fn instant(text: &str) -> Option<(i64, u32)> {
    let bytes = text.as_bytes();
    let digits = |from: usize, count: usize| number(bytes.get(from..from + count)?);
    let at = |index: usize, expected: &[u8]| bytes.get(index).is_some_and(|b| expected.contains(b));
    if !(at(4, b"-") && at(7, b"-") && at(10, b"Tt") && at(13, b":") && at(16, b":")) {
        return None;
    }
    let (year, month, day) = (digits(0, 4)?, digits(5, 2)?, digits(8, 2)?);
    let (hour, minute, second) = (digits(11, 2)?, digits(14, 2)?, digits(17, 2)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    // A leap second is the last second of its minute, as near as a count of
    // seconds since the epoch can say it.
    let second = second.min(59);

    let mut rest = &text[19..];
    let mut nanos: u32 = 0;
    if let Some(fraction) = rest.strip_prefix('.') {
        let length = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if length == 0 {
            return None;
        }
        for (place, digit) in fraction.bytes().take(length).enumerate() {
            if place < 9 {
                nanos += u32::from(digit - b'0') * 10u32.pow(8 - place as u32);
            }
        }
        rest = &fraction[length..];
    }
    let offset = match rest {
        "Z" | "z" => 0,
        _ => {
            let sign = match rest.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let zone = rest.as_bytes();
            if zone.len() != 6 || zone[3] != b':' {
                return None;
            }
            let hours = number(&zone[1..3])?;
            let minutes = number(&zone[4..6])?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            sign * (hours * 3600 + minutes * 60)
        }
    };
    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second;
    Some((seconds - offset, nanos))
}

/// The last second RFC 3339 writes, 9999-12-31T23:59:59Z: its year has
/// four digits.
const LAST: i64 = 253_402_300_799;

/// `seconds` since the Unix epoch, in UTC, to the second, as RFC 3339
/// writes them: what a definition says it was fetched at, and a TOML date
/// and time as it is.
///
/// Held between the epoch and the last second a year of four digits
/// writes, so a clock set past either still writes a time the definition
/// reads, rather than a year TOML takes for no date at all.
pub(crate) fn written(seconds: i64) -> String {
    let seconds = seconds.clamp(0, LAST);
    let days = seconds.div_euclid(86_400);
    let of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60
    )
}

/// ASCII digits as the number they write.
fn number(digits: &[u8]) -> Option<i64> {
    digits.iter().all(u8::is_ascii_digit).then(|| {
        digits
            .iter()
            .fold(0, |number, digit| number * 10 + i64::from(digit - b'0'))
    })
}

/// The days from 1970-01-01 to a date of the proleptic Gregorian calendar,
/// as Howard Hinnant's `days_from_civil` counts them.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let from_march = (month + 9) % 12;
    let of_year = (153 * from_march + 2) / 5 + day - 1;
    let of_era_days = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_era_days - 719_468
}

/// The date `days` after 1970-01-01, the other way round.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let of_era = days - era * 146_097;
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let from_march = (5 * of_year + 2) / 153;
    let day = of_year - (153 * from_march + 2) / 5 + 1;
    let month = if from_march < 10 {
        from_march + 3
    } else {
        from_march - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
#[path = "time_test.rs"]
mod tests;
