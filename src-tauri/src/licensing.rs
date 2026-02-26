use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use voxpen_core::error::AppError;
use voxpen_core::licensing::manager::{LicenseStore, UsageDb};
use voxpen_core::licensing::types::{LicenseInfo, UsageCategory};
use voxpen_core::licensing::usage;

// ---------------------------------------------------------------------------
// TauriLicenseStore — persists LicenseInfo in Tauri's encrypted store
// ---------------------------------------------------------------------------

/// Concrete [`LicenseStore`] backed by Tauri's encrypted `secrets.json`.
pub struct TauriLicenseStore {
    app: AppHandle,
}

impl TauriLicenseStore {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl LicenseStore for TauriLicenseStore {
    fn load(&self) -> Option<LicenseInfo> {
        let store = self.app.store("secrets.json").ok()?;
        let value = store.get("license_info")?;
        serde_json::from_value(value).ok()
    }

    fn save(&self, info: &LicenseInfo) -> Result<(), AppError> {
        let store = self
            .app
            .store("secrets.json")
            .map_err(|e| AppError::License(e.to_string()))?;
        let value =
            serde_json::to_value(info).map_err(|e| AppError::License(e.to_string()))?;
        store.set("license_info", value);
        store
            .save()
            .map_err(|e| AppError::License(e.to_string()))?;
        Ok(())
    }

    fn clear(&self) -> Result<(), AppError> {
        let store = self
            .app
            .store("secrets.json")
            .map_err(|e| AppError::License(e.to_string()))?;
        store.delete("license_info");
        store
            .save()
            .map_err(|e| AppError::License(e.to_string()))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// SqliteUsageDb — daily usage counting backed by rusqlite (per-category)
// ---------------------------------------------------------------------------

/// Concrete [`UsageDb`] backed by SQLite, following the same pattern
/// as `HistoryDb` and `DictionaryDb`.
pub struct SqliteUsageDb {
    conn: Mutex<Connection>,
}

impl SqliteUsageDb {
    /// Open (or create) the SQLite database at the given path, create the
    /// v2 table, and migrate legacy v1 data if present.
    pub fn open(db_path: PathBuf) -> Result<Self, String> {
        let conn =
            Connection::open(&db_path).map_err(|e| format!("usage DB: {e}"))?;

        // Create v2 table
        conn.execute(usage::SQL_CREATE_DAILY_USAGE_V2, [])
            .map_err(|e| format!("usage v2 table: {e}"))?;

        // Detect and migrate v1 data
        let has_v1: bool = conn
            .query_row(usage::SQL_DETECT_V1, [], |row| row.get::<_, i32>(0))
            .map(|count| count > 0)
            .unwrap_or(false);

        if has_v1 {
            conn.execute(usage::SQL_MIGRATE_V1_TO_V2, [])
                .map_err(|e| format!("usage migration: {e}"))?;
            conn.execute(usage::SQL_DROP_V1, [])
                .map_err(|e| format!("drop v1: {e}"))?;
        }

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

impl UsageDb for SqliteUsageDb {
    fn get_count(&self, date: &str, category: UsageCategory) -> u32 {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let cat = usage::category_to_str(category);
        conn.query_row(usage::SQL_GET_COUNT, [date, cat], |row| {
            row.get::<_, u32>(0)
        })
        .unwrap_or(0)
    }

    fn increment(&self, date: &str, category: UsageCategory) -> Result<u32, AppError> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let cat = usage::category_to_str(category);
        conn.execute(usage::SQL_INCREMENT, [date, cat])
            .map_err(|e| AppError::License(e.to_string()))?;
        conn.query_row(usage::SQL_GET_COUNT, [date, cat], |row| {
            row.get::<_, u32>(0)
        })
        .map_err(|e| AppError::License(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxpen_core::licensing::manager::UsageDb;
    use voxpen_core::licensing::types::UsageCategory;

    fn open_test_db() -> SqliteUsageDb {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_usage.db");
        std::mem::forget(dir);
        SqliteUsageDb::open(path).unwrap()
    }

    #[test]
    fn should_open_and_create_table() {
        let db = open_test_db();
        assert_eq!(
            db.get_count("2026-02-26", UsageCategory::VoiceInput),
            0
        );
    }

    #[test]
    fn should_increment_and_get_count_per_category() {
        let db = open_test_db();
        let count = db
            .increment("2026-02-26", UsageCategory::VoiceInput)
            .unwrap();
        assert_eq!(count, 1);
        let count = db
            .increment("2026-02-26", UsageCategory::VoiceInput)
            .unwrap();
        assert_eq!(count, 2);
        assert_eq!(
            db.get_count("2026-02-26", UsageCategory::VoiceInput),
            2
        );
        // Other category should still be 0
        assert_eq!(
            db.get_count("2026-02-26", UsageCategory::Refinement),
            0
        );
    }

    #[test]
    fn should_track_separate_categories() {
        let db = open_test_db();
        db.increment("2026-02-26", UsageCategory::VoiceInput)
            .unwrap();
        db.increment("2026-02-26", UsageCategory::Refinement)
            .unwrap();
        db.increment("2026-02-26", UsageCategory::Refinement)
            .unwrap();
        db.increment("2026-02-26", UsageCategory::FileTranscription)
            .unwrap();

        assert_eq!(
            db.get_count("2026-02-26", UsageCategory::VoiceInput),
            1
        );
        assert_eq!(
            db.get_count("2026-02-26", UsageCategory::Refinement),
            2
        );
        assert_eq!(
            db.get_count("2026-02-26", UsageCategory::FileTranscription),
            1
        );
    }

    #[test]
    fn should_track_separate_dates() {
        let db = open_test_db();
        db.increment("2026-02-26", UsageCategory::VoiceInput)
            .unwrap();
        db.increment("2026-02-26", UsageCategory::VoiceInput)
            .unwrap();
        db.increment("2026-02-27", UsageCategory::VoiceInput)
            .unwrap();

        assert_eq!(
            db.get_count("2026-02-26", UsageCategory::VoiceInput),
            2
        );
        assert_eq!(
            db.get_count("2026-02-27", UsageCategory::VoiceInput),
            1
        );
        assert_eq!(
            db.get_count("2026-02-28", UsageCategory::VoiceInput),
            0
        );
    }

    #[test]
    fn should_return_zero_for_unknown_date() {
        let db = open_test_db();
        assert_eq!(
            db.get_count("9999-12-31", UsageCategory::VoiceInput),
            0
        );
    }

    #[test]
    fn should_migrate_v1_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("migrate_test.db");

        // Create a v1 database
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute(
                "CREATE TABLE daily_usage (date TEXT PRIMARY KEY NOT NULL, count INTEGER NOT NULL DEFAULT 0)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO daily_usage (date, count) VALUES ('2026-02-25', 5)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO daily_usage (date, count) VALUES ('2026-02-26', 3)",
                [],
            )
            .unwrap();
        }

        // Open with migration
        let db = SqliteUsageDb::open(path.clone()).unwrap();

        // V1 data should be in v2 as VoiceInput
        assert_eq!(
            db.get_count("2026-02-25", UsageCategory::VoiceInput),
            5
        );
        assert_eq!(
            db.get_count("2026-02-26", UsageCategory::VoiceInput),
            3
        );
        // Other categories should be 0
        assert_eq!(
            db.get_count("2026-02-25", UsageCategory::Refinement),
            0
        );

        // V1 table should be gone
        let conn = Connection::open(&path).unwrap();
        let has_v1: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='daily_usage'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(has_v1, 0);

        std::mem::forget(dir);
    }
}
