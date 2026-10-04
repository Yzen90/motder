//! Minimal time helpers (std only).

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// "just now", "5m ago", "3h ago", "12d ago", or a date for older messages.
pub fn relative(then: u64, now: u64) -> String {
    let age = now.saturating_sub(then);
    match age {
        0..60 => "just now".into(),
        60..3_600 => format!("{}m ago", age / 60),
        3_600..86_400 => format!("{}h ago", age / 3_600),
        86_400..5_184_000 => format!("{}d ago", age / 86_400),
        _ => iso8601(then)[..10].to_string(),
    }
}

/// RFC 3339 / ISO 8601 UTC timestamp, e.g. `2026-10-04T20:34:00Z`.
pub fn iso8601(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
/// Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_known_values() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso8601(1_791_146_040), "2026-10-04T20:34:00Z");
    }

    #[test]
    fn relative_buckets() {
        assert_eq!(relative(100, 100), "just now");
        assert_eq!(relative(200, 100), "just now");
        assert_eq!(relative(0, 59), "just now");
        assert_eq!(relative(0, 125), "2m ago");
        assert_eq!(relative(0, 7_200), "2h ago");
        assert_eq!(relative(0, 3 * 86_400), "3d ago");
        assert_eq!(relative(0, 400 * 86_400), "1970-01-01");
    }
}
