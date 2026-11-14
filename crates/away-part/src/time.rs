//! The time, written the way a log collector reads it.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The time now, since 1 January 1970. A clock set before that reads as that day.
pub fn now() -> Duration {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default()
}

/// A time in UTC with its milliseconds, as RFC 3339 writes it: `2026-11-14T09:14:02.120Z`.
pub fn rfc3339(since_1970: Duration) -> String {
    let seconds = since_1970.as_secs();
    let (year, month, day) = date(seconds / 86_400);
    let of_day = seconds % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        of_day / 3_600,
        of_day % 3_600 / 60,
        of_day % 60,
        since_1970.subsec_millis()
    )
}

/// The date that many days after 1 January 1970, in the calendar everyone uses now.
/// After Howard Hinnant's `civil_from_days`, for days that are not before 1970.
fn date(days: u64) -> (u64, u64, u64) {
    let shifted = days + 719_468; // days since 1 March of the year 0
    let era = shifted / 146_097; // a span of 400 years
    let day_of_era = shifted % 146_097;
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 { month_from_march + 3 } else { month_from_march - 9 };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: u64, millis: u32) -> String {
        rfc3339(Duration::new(seconds, millis * 1_000_000))
    }

    #[test]
    fn the_first_moment() {
        assert_eq!(at(0, 0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn a_known_moment_with_milliseconds() {
        assert_eq!(at(1_700_000_000, 120), "2023-11-14T22:13:20.120Z");
    }

    #[test]
    fn a_leap_day_and_the_day_after() {
        assert_eq!(at(951_782_400, 0), "2000-02-29T00:00:00.000Z");
        assert_eq!(at(951_868_800, 0), "2000-03-01T00:00:00.000Z");
    }

    #[test]
    fn the_last_second_of_a_year() {
        assert_eq!(at(1_798_761_599, 999), "2026-12-31T23:59:59.999Z");
        assert_eq!(at(1_798_761_600, 0), "2027-01-01T00:00:00.000Z");
    }

    #[test]
    fn a_century_that_is_no_leap_year() {
        assert_eq!(at(4_107_456_000, 0), "2100-02-28T00:00:00.000Z");
        assert_eq!(at(4_107_542_400, 0), "2100-03-01T00:00:00.000Z");
    }
}
