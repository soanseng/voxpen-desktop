use crate::licensing::types::{
    CategorizedUsageStatus, UsageCategory, UsageStatus, free_daily_limit, warning_threshold,
};

// ---------------------------------------------------------------------------
// SQL constants for the daily_usage_v2 table (per-category)
// ---------------------------------------------------------------------------

/// Create the daily_usage_v2 table if it does not exist.
pub const SQL_CREATE_DAILY_USAGE_V2: &str = "\
CREATE TABLE IF NOT EXISTS daily_usage_v2 (
    date TEXT NOT NULL,
    category TEXT NOT NULL,
    count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (date, category)
)";

/// Detect whether the legacy daily_usage table exists.
pub const SQL_DETECT_V1: &str = "\
SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='daily_usage'";

/// Migrate v1 data into v2, treating all old counts as VoiceInput.
pub const SQL_MIGRATE_V1_TO_V2: &str = "\
INSERT OR IGNORE INTO daily_usage_v2 (date, category, count)
SELECT date, 'VoiceInput', count FROM daily_usage";

/// Drop the legacy v1 table after migration.
pub const SQL_DROP_V1: &str = "DROP TABLE IF EXISTS daily_usage";

/// Get the usage count for a specific date and category. Returns 0 if no row.
pub const SQL_GET_COUNT: &str = "\
SELECT COALESCE(
    (SELECT count FROM daily_usage_v2 WHERE date = ?1 AND category = ?2),
    0
)";

/// Upsert: increment count for a date+category, inserting if not present.
pub const SQL_INCREMENT: &str = "\
INSERT INTO daily_usage_v2 (date, category, count) VALUES (?1, ?2, 1)
ON CONFLICT(date, category) DO UPDATE SET count = count + 1";

/// Delete usage records older than a given date.
pub const SQL_CLEANUP: &str = "\
DELETE FROM daily_usage_v2 WHERE date < ?1";

// ---------------------------------------------------------------------------
// Pure functions
// ---------------------------------------------------------------------------

/// Convert a UsageCategory to its string key for the database.
pub fn category_to_str(category: UsageCategory) -> &'static str {
    match category {
        UsageCategory::VoiceInput => "VoiceInput",
        UsageCategory::Refinement => "Refinement",
        UsageCategory::FileTranscription => "FileTranscription",
    }
}

/// Compute the usage status for a Free-tier user given the current count
/// and category.
///
/// - count >= limit              => Exhausted
/// - remaining <= warning_thresh => Warning
/// - otherwise                   => Available
pub fn compute_status(category: UsageCategory, count: u32) -> UsageStatus {
    let limit = free_daily_limit(category);
    if count >= limit {
        UsageStatus::Exhausted
    } else {
        let remaining = limit - count;
        if remaining <= warning_threshold(category) {
            UsageStatus::Warning { remaining }
        } else {
            UsageStatus::Available { remaining }
        }
    }
}

/// Compute the full categorized usage status from three counts.
pub fn compute_categorized_status(
    voice_count: u32,
    refine_count: u32,
    file_count: u32,
) -> CategorizedUsageStatus {
    CategorizedUsageStatus {
        voice_input: compute_status(UsageCategory::VoiceInput, voice_count),
        refinement: compute_status(UsageCategory::Refinement, refine_count),
        file_transcription: compute_status(UsageCategory::FileTranscription, file_count),
    }
}

/// Return today's date as "YYYY-MM-DD" in the local timezone.
pub fn today_local() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- compute_status boundary tests: VoiceInput (limit=30, warn=5) --

    #[test]
    fn voice_should_return_available_when_count_is_zero() {
        assert_eq!(
            compute_status(UsageCategory::VoiceInput, 0),
            UsageStatus::Available { remaining: 30 }
        );
    }

    #[test]
    fn voice_should_return_available_when_count_is_24() {
        // remaining = 6, which is > warning_threshold (5)
        assert_eq!(
            compute_status(UsageCategory::VoiceInput, 24),
            UsageStatus::Available { remaining: 6 }
        );
    }

    #[test]
    fn voice_should_return_warning_when_count_is_25() {
        // remaining = 5, which is == warning_threshold
        assert_eq!(
            compute_status(UsageCategory::VoiceInput, 25),
            UsageStatus::Warning { remaining: 5 }
        );
    }

    #[test]
    fn voice_should_return_warning_when_count_is_29() {
        assert_eq!(
            compute_status(UsageCategory::VoiceInput, 29),
            UsageStatus::Warning { remaining: 1 }
        );
    }

    #[test]
    fn voice_should_return_exhausted_when_count_is_30() {
        assert_eq!(
            compute_status(UsageCategory::VoiceInput, 30),
            UsageStatus::Exhausted
        );
    }

    #[test]
    fn voice_should_return_exhausted_when_count_exceeds_limit() {
        assert_eq!(
            compute_status(UsageCategory::VoiceInput, 35),
            UsageStatus::Exhausted
        );
    }

    // -- compute_status boundary tests: Refinement (limit=10, warn=2) --

    #[test]
    fn refine_should_return_available_when_count_is_zero() {
        assert_eq!(
            compute_status(UsageCategory::Refinement, 0),
            UsageStatus::Available { remaining: 10 }
        );
    }

    #[test]
    fn refine_should_return_warning_when_count_is_8() {
        // remaining = 2, which is == warning_threshold
        assert_eq!(
            compute_status(UsageCategory::Refinement, 8),
            UsageStatus::Warning { remaining: 2 }
        );
    }

    #[test]
    fn refine_should_return_exhausted_when_count_is_10() {
        assert_eq!(
            compute_status(UsageCategory::Refinement, 10),
            UsageStatus::Exhausted
        );
    }

    // -- compute_status boundary tests: FileTranscription (limit=2, warn=1) --

    #[test]
    fn file_should_return_available_when_count_is_zero() {
        // remaining = 2 > warn=1 => Available
        // Wait, 2 > 1 is true, so Available { remaining: 2 }
        assert_eq!(
            compute_status(UsageCategory::FileTranscription, 0),
            UsageStatus::Available { remaining: 2 }
        );
    }

    #[test]
    fn file_should_return_warning_when_count_is_1() {
        // remaining = 1, which is == warning_threshold
        assert_eq!(
            compute_status(UsageCategory::FileTranscription, 1),
            UsageStatus::Warning { remaining: 1 }
        );
    }

    #[test]
    fn file_should_return_exhausted_when_count_is_2() {
        assert_eq!(
            compute_status(UsageCategory::FileTranscription, 2),
            UsageStatus::Exhausted
        );
    }

    // -- compute_categorized_status --

    #[test]
    fn should_compute_categorized_status_correctly() {
        let status = compute_categorized_status(0, 8, 2);
        assert_eq!(
            status.voice_input,
            UsageStatus::Available { remaining: 30 }
        );
        assert_eq!(
            status.refinement,
            UsageStatus::Warning { remaining: 2 }
        );
        assert_eq!(status.file_transcription, UsageStatus::Exhausted);
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
    fn sql_create_should_contain_v2_table() {
        assert!(SQL_CREATE_DAILY_USAGE_V2.contains("daily_usage_v2"));
        assert!(SQL_CREATE_DAILY_USAGE_V2.contains("CREATE TABLE IF NOT EXISTS"));
        assert!(SQL_CREATE_DAILY_USAGE_V2.contains("date TEXT"));
        assert!(SQL_CREATE_DAILY_USAGE_V2.contains("category TEXT"));
        assert!(SQL_CREATE_DAILY_USAGE_V2.contains("count INTEGER"));
    }

    #[test]
    fn sql_get_count_should_select_from_v2() {
        assert!(SQL_GET_COUNT.contains("daily_usage_v2"));
        assert!(SQL_GET_COUNT.contains("SELECT"));
        assert!(SQL_GET_COUNT.contains("COALESCE"));
        assert!(SQL_GET_COUNT.contains("category"));
    }

    #[test]
    fn sql_increment_should_upsert_with_category() {
        assert!(SQL_INCREMENT.contains("INSERT INTO daily_usage_v2"));
        assert!(SQL_INCREMENT.contains("ON CONFLICT"));
        assert!(SQL_INCREMENT.contains("count + 1"));
    }

    #[test]
    fn sql_cleanup_should_delete_old_records() {
        assert!(SQL_CLEANUP.contains("DELETE FROM daily_usage_v2"));
        assert!(SQL_CLEANUP.contains("date < ?1"));
    }

    #[test]
    fn category_to_str_should_roundtrip() {
        assert_eq!(category_to_str(UsageCategory::VoiceInput), "VoiceInput");
        assert_eq!(category_to_str(UsageCategory::Refinement), "Refinement");
        assert_eq!(
            category_to_str(UsageCategory::FileTranscription),
            "FileTranscription"
        );
    }
}
