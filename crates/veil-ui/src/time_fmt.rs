//! Relative timestamp formatting for conversation entries.
//!
//! Converts absolute `DateTime<Utc>` values into human-readable relative
//! strings like "just now", "5m ago", "yesterday", etc.

use chrono::{DateTime, Utc};

/// Format a timestamp as a relative string ("just now", "5m ago", "2h ago",
/// "yesterday", "3 days ago", "2 weeks ago", "Jan 15").
///
/// Uses `now` as the reference time to enable deterministic testing.
pub fn format_relative(timestamp: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let duration = now.signed_duration_since(timestamp);
    let total_seconds = duration.num_seconds();

    // Future or same instant: defensive fallback
    if total_seconds <= 0 {
        return "just now".to_string();
    }

    let total_minutes = duration.num_minutes();
    let total_hours = duration.num_hours();
    let total_days = duration.num_days();

    if total_seconds < 60 {
        "just now".to_string()
    } else if total_minutes < 60 {
        format!("{}m ago", total_minutes)
    } else if total_hours < 24 {
        format!("{}h ago", total_hours)
    } else if total_hours < 48 {
        "yesterday".to_string()
    } else if total_days < 14 {
        format!("{} days ago", total_days)
    } else if total_days < 60 {
        format!("{} weeks ago", total_days / 7)
    } else if timestamp.format("%Y").to_string() == now.format("%Y").to_string() {
        timestamp.format("%b %-d").to_string()
    } else {
        timestamp.format("%b %-d, %Y").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    // ================================================================
    // Helper: create a fixed "now" reference point
    // ================================================================

    /// A fixed reference time: 2026-04-22 12:00:00 UTC.
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, 22, 12, 0, 0).unwrap()
    }

    // ================================================================
    // Unit 1: Happy path — relative timestamp formatting
    // ================================================================

    #[test]
    fn thirty_seconds_ago_is_just_now() {
        let timestamp = now() - chrono::Duration::seconds(30);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "just now");
    }

    #[test]
    fn five_minutes_ago() {
        let timestamp = now() - chrono::Duration::minutes(5);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "5m ago");
    }

    #[test]
    fn one_minute_ago() {
        let timestamp = now() - chrono::Duration::minutes(1);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "1m ago");
    }

    #[test]
    fn fifty_nine_minutes_ago() {
        let timestamp = now() - chrono::Duration::minutes(59);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "59m ago");
    }

    #[test]
    fn three_hours_ago() {
        let timestamp = now() - chrono::Duration::hours(3);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "3h ago");
    }

    #[test]
    fn one_hour_ago() {
        let timestamp = now() - chrono::Duration::hours(1);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "1h ago");
    }

    #[test]
    fn twenty_three_hours_ago() {
        let timestamp = now() - chrono::Duration::hours(23);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "23h ago");
    }

    #[test]
    fn twenty_six_hours_ago_is_yesterday() {
        let timestamp = now() - chrono::Duration::hours(26);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "yesterday");
    }

    #[test]
    fn five_days_ago() {
        let timestamp = now() - chrono::Duration::days(5);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "5 days ago");
    }

    #[test]
    fn three_weeks_ago() {
        let timestamp = now() - chrono::Duration::weeks(3);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "3 weeks ago");
    }

    #[test]
    fn ninety_days_ago_same_year_shows_month_day() {
        // 90 days before 2026-04-22 is 2026-01-22
        let timestamp = now() - chrono::Duration::days(90);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "Jan 22");
    }

    #[test]
    fn previous_year_shows_month_day_year() {
        // Dec 15, 2025
        let timestamp = Utc.with_ymd_and_hms(2025, 12, 15, 10, 0, 0).unwrap();
        let result = format_relative(timestamp, now());
        assert_eq!(result, "Dec 15, 2025");
    }

    // ================================================================
    // Unit 1: Edge cases — boundary values
    // ================================================================

    #[test]
    fn exactly_sixty_seconds_is_one_minute() {
        let timestamp = now() - chrono::Duration::seconds(60);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "1m ago");
    }

    #[test]
    fn fifty_nine_seconds_is_just_now() {
        let timestamp = now() - chrono::Duration::seconds(59);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "just now");
    }

    #[test]
    fn exactly_sixty_minutes_is_one_hour() {
        let timestamp = now() - chrono::Duration::minutes(60);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "1h ago");
    }

    #[test]
    fn exactly_twenty_four_hours_is_yesterday() {
        let timestamp = now() - chrono::Duration::hours(24);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "yesterday");
    }

    #[test]
    fn exactly_forty_eight_hours_is_two_days_ago() {
        let timestamp = now() - chrono::Duration::hours(48);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "2 days ago");
    }

    #[test]
    fn exactly_fourteen_days_is_two_weeks() {
        let timestamp = now() - chrono::Duration::days(14);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "2 weeks ago");
    }

    #[test]
    fn thirteen_days_is_days_not_weeks() {
        let timestamp = now() - chrono::Duration::days(13);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "13 days ago");
    }

    #[test]
    fn fifty_nine_days_is_weeks() {
        let timestamp = now() - chrono::Duration::days(59);
        // 59 / 7 = 8 weeks
        let result = format_relative(timestamp, now());
        assert_eq!(result, "8 weeks ago");
    }

    #[test]
    fn sixty_days_shows_month_day() {
        // 60 days before 2026-04-22 is 2026-02-21
        let timestamp = now() - chrono::Duration::days(60);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "Feb 21");
    }

    #[test]
    fn future_timestamp_returns_just_now() {
        let timestamp = now() + chrono::Duration::hours(1);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "just now");
    }

    #[test]
    fn same_timestamp_returns_just_now() {
        let result = format_relative(now(), now());
        assert_eq!(result, "just now");
    }

    #[test]
    fn two_days_ago() {
        let timestamp = now() - chrono::Duration::days(2);
        let result = format_relative(timestamp, now());
        assert_eq!(result, "2 days ago");
    }
}
