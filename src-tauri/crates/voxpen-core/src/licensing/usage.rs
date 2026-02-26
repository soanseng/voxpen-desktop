use crate::licensing::types::{UsageStatus, FREE_DAILY_LIMIT, WARNING_THRESHOLD};

// ---------------------------------------------------------------------------
// SQL constants for the daily_usage table
// ---------------------------------------------------------------------------

/// Create the daily_usage table if it does not exist.
pub const SQL_CREATE_DAILY_USAGE: &str = "\
CREATE TABLE IF NOT EXISTS daily_usage (
    date TEXT PRIMARY KEY NOT NULL,
    count INTEGER NOT NULL DEFAULT 0
)";

/// Get the usage count for a specific date. Returns 0 if no row exists.
pub const SQL_GET_COUNT: &str = "\
SELECT COALESCE(
    (SELECT count FROM daily_usage WHERE date = ?1),
    0
)";

/// Upsert: increment count for a date, inserting if not present.
pub const SQL_INCREMENT: &str = "\
INSERT INTO daily_usage (date, count) VALUES (?1, 1)
ON CONFLICT(date) DO UPDATE SET count = count + 1";

/// Delete usage records older than a given date.
pub const SQL_CLEANUP: &str = "\
DELETE FROM daily_usage WHERE date < ?1";

// ---------------------------------------------------------------------------
// Pure functions
// ---------------------------------------------------------------------------

/// Compute the usage status for a Free-tier user given the current count.
///
/// - count < (LIMIT - WARNING_THRESHOLD) => Available
/// - count < LIMIT                       => Warning
/// - count >= LIMIT                      => Exhausted
pub fn compute_status(count: u32) -> UsageStatus {
    if count >= FREE_DAILY_LIMIT {
        UsageStatus::Exhausted
    } else {
        let remaining = FREE_DAILY_LIMIT - count;
        if remaining <= WARNING_THRESHOLD {
            UsageStatus::Warning { remaining }
        } else {
            UsageStatus::Available { remaining }
        }
    }
}

/// Return today's date as "YYYY-MM-DD" in the local timezone.
pub fn today_local() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- compute_status boundary tests --

    #[test]
    fn should_return_available_when_count_is_zero() {
        assert_eq!(
            compute_status(0),
            UsageStatus::Available { remaining: 15 }
        );
    }

    #[test]
    fn should_return_available_when_count_is_11() {
        // remaining = 4, which is > WARNING_THRESHOLD (3)
        assert_eq!(
            compute_status(11),
            UsageStatus::Available { remaining: 4 }
        );
    }

    #[test]
    fn should_return_warning_when_count_is_12() {
        // remaining = 3, which is == WARNING_THRESHOLD
        assert_eq!(
            compute_status(12),
            UsageStatus::Warning { remaining: 3 }
        );
    }

    #[test]
    fn should_return_warning_when_count_is_13() {
        assert_eq!(
            compute_status(13),
            UsageStatus::Warning { remaining: 2 }
        );
    }

    #[test]
    fn should_return_warning_when_count_is_14() {
        assert_eq!(
            compute_status(14),
            UsageStatus::Warning { remaining: 1 }
        );
    }

    #[test]
    fn should_return_exhausted_when_count_is_15() {
        assert_eq!(compute_status(15), UsageStatus::Exhausted);
    }

    #[test]
    fn should_return_exhausted_when_count_exceeds_limit() {
        assert_eq!(compute_status(20), UsageStatus::Exhausted);
    }

    // -- today_local format --

    #[test]
    fn should_return_valid_date_format() {
        let date = today_local();
        // Format: YYYY-MM-DD
        assert_eq!(date.len(), 10);
        assert_eq!(&date[4..5], "-");
        assert_eq!(&date[7..8], "-");

        // Year, month, day should all be numeric
        assert!(date[0..4].chars().all(|c| c.is_ascii_digit()));
        assert!(date[5..7].chars().all(|c| c.is_ascii_digit()));
        assert!(date[8..10].chars().all(|c| c.is_ascii_digit()));
    }

    // -- SQL string assertions --

    #[test]
    fn sql_create_should_contain_daily_usage_table() {
        assert!(SQL_CREATE_DAILY_USAGE.contains("daily_usage"));
        assert!(SQL_CREATE_DAILY_USAGE.contains("CREATE TABLE IF NOT EXISTS"));
        assert!(SQL_CREATE_DAILY_USAGE.contains("date TEXT PRIMARY KEY"));
        assert!(SQL_CREATE_DAILY_USAGE.contains("count INTEGER"));
    }

    #[test]
    fn sql_get_count_should_select_from_daily_usage() {
        assert!(SQL_GET_COUNT.contains("daily_usage"));
        assert!(SQL_GET_COUNT.contains("SELECT"));
        assert!(SQL_GET_COUNT.contains("COALESCE"));
    }

    #[test]
    fn sql_increment_should_upsert() {
        assert!(SQL_INCREMENT.contains("INSERT INTO daily_usage"));
        assert!(SQL_INCREMENT.contains("ON CONFLICT"));
        assert!(SQL_INCREMENT.contains("count + 1"));
    }

    #[test]
    fn sql_cleanup_should_delete_old_records() {
        assert!(SQL_CLEANUP.contains("DELETE FROM daily_usage"));
        assert!(SQL_CLEANUP.contains("date < ?1"));
    }
}
