use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use voxink_core::error::AppError;
use voxink_core::licensing::manager::{LicenseStore, UsageDb};
use voxink_core::licensing::types::LicenseInfo;
use voxink_core::licensing::usage;

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
// SqliteUsageDb — daily usage counting backed by rusqlite
// ---------------------------------------------------------------------------

/// Concrete [`UsageDb`] backed by SQLite, following the same pattern
/// as `HistoryDb` and `DictionaryDb`.
pub struct SqliteUsageDb {
    conn: Mutex<Connection>,
}

impl SqliteUsageDb {
    /// Open (or create) the SQLite database at the given path and create
    /// the `daily_usage` table if it does not exist.
    pub fn open(db_path: PathBuf) -> Result<Self, String> {
        let conn =
            Connection::open(&db_path).map_err(|e| format!("usage DB: {e}"))?;
        conn.execute(usage::SQL_CREATE_DAILY_USAGE, [])
            .map_err(|e| format!("usage table: {e}"))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

impl UsageDb for SqliteUsageDb {
    fn get_count(&self, date: &str) -> u32 {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.query_row(usage::SQL_GET_COUNT, [date], |row| row.get::<_, u32>(0))
            .unwrap_or(0)
    }

    fn increment(&self, date: &str) -> Result<u32, AppError> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(usage::SQL_INCREMENT, [date])
            .map_err(|e| AppError::License(e.to_string()))?;
        conn.query_row(usage::SQL_GET_COUNT, [date], |row| row.get::<_, u32>(0))
            .map_err(|e| AppError::License(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxink_core::licensing::manager::UsageDb;

    fn open_test_db() -> SqliteUsageDb {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_usage.db");
        std::mem::forget(dir);
        SqliteUsageDb::open(path).unwrap()
    }

    #[test]
    fn should_open_and_create_table() {
        let db = open_test_db();
        assert_eq!(db.get_count("2026-02-26"), 0);
    }

    #[test]
    fn should_increment_and_get_count() {
        let db = open_test_db();
        let count = db.increment("2026-02-26").unwrap();
        assert_eq!(count, 1);
        let count = db.increment("2026-02-26").unwrap();
        assert_eq!(count, 2);
        assert_eq!(db.get_count("2026-02-26"), 2);
    }

    #[test]
    fn should_track_separate_dates() {
        let db = open_test_db();
        db.increment("2026-02-26").unwrap();
        db.increment("2026-02-26").unwrap();
        db.increment("2026-02-27").unwrap();

        assert_eq!(db.get_count("2026-02-26"), 2);
        assert_eq!(db.get_count("2026-02-27"), 1);
        assert_eq!(db.get_count("2026-02-28"), 0);
    }

    #[test]
    fn should_return_zero_for_unknown_date() {
        let db = open_test_db();
        assert_eq!(db.get_count("9999-12-31"), 0);
    }
}
