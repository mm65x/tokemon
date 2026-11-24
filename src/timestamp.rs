use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};

use crate::config::TimezoneMode;

/// Parse a timestamp string, trying multiple formats:
/// 1. RFC 3339 (e.g., "2026-02-20T10:00:00.000Z")
/// 2. Unix seconds (10-digit integer)
/// 3. Unix milliseconds (13-digit integer)
pub fn parse_timestamp(s: &str) -> Option<DateTime<Utc>> {
    // RFC 3339 (most common)
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.to_utc());
    }

    // Try as numeric timestamp
    if let Ok(n) = s.parse::<i64>() {
        return parse_timestamp_numeric(n);
    }

    None
}

/// Parse a numeric timestamp, auto-detecting seconds vs milliseconds.
pub fn parse_timestamp_numeric(n: i64) -> Option<DateTime<Utc>> {
    if n > 1_000_000_000_000 {
        // Milliseconds
        DateTime::from_timestamp_millis(n)
    } else if n > 1_000_000_000 {
        // Seconds
        DateTime::from_timestamp(n, 0)
    } else {
        None
    }
}

/// Parse a millisecond timestamp directly.
pub fn parse_timestamp_millis(ms: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(ms)
}

// ── Period start-date helpers ─────────────────────────────────────────────

/// Convert a UTC instant into a calendar date under `tz`.
#[must_use]
pub fn date_for(ts: DateTime<Utc>, tz: TimezoneMode) -> NaiveDate {
    match tz {
        TimezoneMode::Utc => ts.date_naive(),
        TimezoneMode::Local => ts.with_timezone(&Local).date_naive(),
    }
}

/// Return today's date in the given timezone.
#[must_use]
pub fn today_in(tz: TimezoneMode) -> NaiveDate {
    date_for(Utc::now(), tz)
}

/// Return the Monday of the current ISO week in the given timezone.
#[must_use]
pub fn start_of_week_in(tz: TimezoneMode) -> NaiveDate {
    let today = today_in(tz);
    today - chrono::Duration::days(i64::from(today.weekday().num_days_from_monday()))
}

/// Return the first day of the current month in the given timezone.
#[must_use]
pub fn start_of_month_in(tz: TimezoneMode) -> NaiveDate {
    let today = today_in(tz);
    NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today)
}

/// Return today's date (UTC).
#[must_use]
#[cfg_attr(not(test), allow(dead_code))]
pub fn start_of_today() -> NaiveDate {
    today_in(TimezoneMode::Utc)
}

/// Return the Monday of the current ISO week (UTC).
#[must_use]
#[allow(dead_code)]
pub fn start_of_week() -> NaiveDate {
    start_of_week_in(TimezoneMode::Utc)
}

/// Return the first day of the current month (UTC).
#[must_use]
#[allow(dead_code)]
pub fn start_of_month() -> NaiveDate {
    start_of_month_in(TimezoneMode::Utc)
}

/// Extract a session ID from a file path (stem without extension).
pub fn extract_session_id(path: &std::path::Path) -> Option<String> {
    path.file_stem()?.to_str().map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rfc3339() {
        let ts = parse_timestamp("2026-02-20T10:00:05.000Z");
        assert!(ts.is_some());
        assert!(ts.unwrap().to_rfc3339().starts_with("2026-02-20"));
    }

    #[test]
    fn test_unix_seconds() {
        let ts = parse_timestamp("1740052800");
        assert!(ts.is_some());
    }

    #[test]
    fn test_unix_millis() {
        let ts = parse_timestamp("1740052800000");
        assert!(ts.is_some());
    }

    #[test]
    fn test_invalid() {
        assert!(parse_timestamp("not-a-date").is_none());
        assert!(parse_timestamp("").is_none());
        assert!(parse_timestamp("123").is_none()); // Too small for Unix
    }

    #[test]
    fn test_numeric_detection() {
        // 10-digit = seconds
        let s = parse_timestamp_numeric(1740052800);
        assert!(s.is_some());

        // 13-digit = milliseconds
        let ms = parse_timestamp_numeric(1740052800000);
        assert!(ms.is_some());

        // Should produce same instant
        assert_eq!(s.unwrap(), ms.unwrap());
    }

    #[test]
    fn test_date_for_utc_matches_naive_date() {
        let ts = DateTime::parse_from_rfc3339("2026-06-15T23:30:00Z")
            .unwrap()
            .to_utc();
        assert_eq!(
            date_for(ts, TimezoneMode::Utc),
            NaiveDate::from_ymd_opt(2026, 6, 15).unwrap()
        );
    }
}
