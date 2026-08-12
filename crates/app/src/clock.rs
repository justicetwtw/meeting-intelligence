use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since the Unix epoch, saturating rather than panicking on a
/// system clock set before 1970.
pub fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Formats a UTC timestamp as RFC 3339, for example `2026-08-12T09:00:00Z`.
///
/// Implemented directly so meeting timestamps do not pull a date library into
/// the application layer for one format.
pub fn format_rfc3339(unix_seconds: u64) -> String {
    let days = i64::try_from(unix_seconds / 86_400).unwrap_or(0);
    let seconds_of_day = unix_seconds % 86_400;
    let (year, month, day) = civil_from_days(days);

    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds_of_day / 3_600,
        (seconds_of_day % 3_600) / 60,
        seconds_of_day % 60
    )
}

/// Howard Hinnant's `civil_from_days`, shifted to an era beginning on 0000-03-01.
fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = u32::try_from(day_of_year - (153 * shifted_month + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    })
    .unwrap_or(1);

    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_instants_format_correctly() {
        assert_eq!(format_rfc3339(0), "1970-01-01T00:00:00Z");
        // 2026-08-12T09:00:00Z
        assert_eq!(format_rfc3339(1_786_525_200), "2026-08-12T09:00:00Z");
        assert_eq!(format_rfc3339(1_786_611_600), "2026-08-13T09:00:00Z");
        // A leap day.
        assert_eq!(format_rfc3339(1_709_164_800), "2024-02-29T00:00:00Z");
        // End of a year.
        assert_eq!(format_rfc3339(1_767_225_599), "2025-12-31T23:59:59Z");
    }

    #[test]
    fn the_clock_returns_a_plausible_present_day_timestamp() {
        let now = unix_time_ms();

        // Later than 2026-01-01 and before the year 2100.
        assert!(now > 1_767_225_600_000);
        assert!(now < 4_102_444_800_000);
    }
}
