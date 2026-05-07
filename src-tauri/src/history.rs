use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

use voxpen_core::history::{TranscriptionEntry, TranscriptionStatus, CREATE_TABLE_SQL};
use voxpen_core::pipeline::state::Language;

/// Thread-safe SQLite database handle for history operations.
pub struct HistoryDb {
    conn: Mutex<Connection>,
}

impl HistoryDb {
    /// Open (or create) the SQLite database at the given path and run migrations.
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let conn = Connection::open(&path).map_err(|e| format!("failed to open DB: {e}"))?;
        conn.execute_batch(CREATE_TABLE_SQL)
            .map_err(|e| format!("failed to create table: {e}"))?;
        migrate_schema(&conn).map_err(|e| format!("failed to migrate table: {e}"))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Insert a transcription entry.
    pub fn insert(&self, entry: &TranscriptionEntry) -> Result<(), String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            voxpen_core::history::INSERT_SQL,
            rusqlite::params![
                entry.id,
                entry.timestamp,
                entry.original_text,
                entry.refined_text,
                serde_json::to_string(&entry.language).unwrap_or_default(),
                entry.audio_duration_ms,
                entry.provider,
                entry.status.as_str(),
                entry.error_message.as_deref(),
                entry.audio_path.as_deref(),
            ],
        )
        .map_err(|e| format!("insert failed: {e}"))?;
        Ok(())
    }

    /// Query transcriptions with limit and offset, newest first.
    pub fn query(&self, limit: u32, offset: u32) -> Result<Vec<TranscriptionEntry>, String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn
            .prepare(voxpen_core::history::QUERY_SQL)
            .map_err(|e| format!("query prepare failed: {e}"))?;
        let rows = stmt
            .query_map(rusqlite::params![limit, offset], row_to_entry)
            .map_err(|e| format!("query failed: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("row read failed: {e}"))
    }

    /// Search transcriptions by text content.
    pub fn search(
        &self,
        query: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<TranscriptionEntry>, String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let pattern = format!("%{query}%");
        let mut stmt = conn
            .prepare(voxpen_core::history::SEARCH_SQL)
            .map_err(|e| format!("search prepare failed: {e}"))?;
        let rows = stmt
            .query_map(
                rusqlite::params![&pattern, &pattern, &pattern, limit, offset],
                row_to_entry,
            )
            .map_err(|e| format!("search failed: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("row read failed: {e}"))
    }

    /// Fetch a single transcription by id.
    pub fn get(&self, id: &str) -> Result<Option<TranscriptionEntry>, String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn
            .prepare(voxpen_core::history::GET_BY_ID_SQL)
            .map_err(|e| format!("get prepare failed: {e}"))?;
        match stmt.query_row(rusqlite::params![id], row_to_entry) {
            Ok(entry) => Ok(Some(entry)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("get failed: {e}")),
        }
    }

    /// Mark an entry as completed after a successful manual retry.
    pub fn update_completed(
        &self,
        id: &str,
        original_text: &str,
        refined_text: Option<&str>,
        language: &Language,
        audio_duration_ms: u64,
        provider: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            voxpen_core::history::UPDATE_COMPLETED_SQL,
            rusqlite::params![
                original_text,
                refined_text,
                serde_json::to_string(language).unwrap_or_default(),
                audio_duration_ms,
                provider,
                id,
            ],
        )
        .map_err(|e| format!("update completed failed: {e}"))?;
        Ok(())
    }

    /// Refresh failure details after a retry also fails.
    pub fn update_failed(
        &self,
        id: &str,
        provider: &str,
        error_message: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            voxpen_core::history::UPDATE_FAILED_SQL,
            rusqlite::params![provider, error_message, id],
        )
        .map_err(|e| format!("update failed failed: {e}"))?;
        Ok(())
    }

    /// Delete a single transcription by id.
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(voxpen_core::history::DELETE_SQL, rusqlite::params![id])
            .map_err(|e| format!("delete failed: {e}"))?;
        Ok(())
    }
}

fn migrate_schema(conn: &Connection) -> rusqlite::Result<()> {
    let existing = table_columns(conn)?;
    if !existing.iter().any(|c| c == "status") {
        conn.execute(
            "ALTER TABLE transcriptions ADD COLUMN status TEXT NOT NULL DEFAULT 'completed'",
            [],
        )?;
    }
    if !existing.iter().any(|c| c == "error_message") {
        conn.execute(
            "ALTER TABLE transcriptions ADD COLUMN error_message TEXT",
            [],
        )?;
    }
    if !existing.iter().any(|c| c == "audio_path") {
        conn.execute("ALTER TABLE transcriptions ADD COLUMN audio_path TEXT", [])?;
    }
    Ok(())
}

fn table_columns(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("PRAGMA table_info(transcriptions)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    rows.collect()
}

/// Map a rusqlite row to a TranscriptionEntry.
fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<TranscriptionEntry> {
    let status: String = row.get(7)?;
    Ok(TranscriptionEntry {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        original_text: row.get(2)?,
        refined_text: row.get(3)?,
        language: {
            let s: String = row.get(4)?;
            serde_json::from_str(&s).unwrap_or(Language::Auto)
        },
        audio_duration_ms: row.get(5)?,
        provider: row.get(6)?,
        status: TranscriptionStatus::from_db(&status),
        error_message: row.get(8)?,
        audio_path: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(status: TranscriptionStatus) -> TranscriptionEntry {
        TranscriptionEntry {
            id: "entry-1".to_string(),
            timestamp: 1_700_000_000,
            original_text: "hello".to_string(),
            refined_text: None,
            language: Language::English,
            audio_duration_ms: 1000,
            provider: "openai".to_string(),
            status,
            error_message: None,
            audio_path: None,
        }
    }

    #[test]
    fn should_insert_and_read_failed_entry() {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(dir.path().join("history.db")).unwrap();
        let mut failed = entry(TranscriptionStatus::Failed);
        failed.original_text.clear();
        failed.error_message = Some("openai HTTP 500: upstream failed".to_string());
        failed.audio_path = Some("/tmp/audio.wav".to_string());

        db.insert(&failed).unwrap();
        let loaded = db.get(&failed.id).unwrap().unwrap();

        assert_eq!(loaded.status, TranscriptionStatus::Failed);
        assert_eq!(
            loaded.error_message.as_deref(),
            Some("openai HTTP 500: upstream failed")
        );
        assert_eq!(loaded.audio_path.as_deref(), Some("/tmp/audio.wav"));
    }

    #[test]
    fn should_migrate_existing_history_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "\
                CREATE TABLE transcriptions (
                    id TEXT PRIMARY KEY NOT NULL,
                    timestamp INTEGER NOT NULL,
                    original_text TEXT NOT NULL,
                    refined_text TEXT,
                    language TEXT NOT NULL,
                    audio_duration_ms INTEGER NOT NULL,
                    provider TEXT NOT NULL
                );
                INSERT INTO transcriptions VALUES (
                    'legacy', 1, 'old text', NULL, '\"English\"', 1000, 'groq'
                );
                ",
            )
            .unwrap();
        }

        let db = HistoryDb::open(path).unwrap();
        let loaded = db.get("legacy").unwrap().unwrap();

        assert_eq!(loaded.status, TranscriptionStatus::Completed);
        assert_eq!(loaded.error_message, None);
        assert_eq!(loaded.audio_path, None);
    }

    #[test]
    fn should_update_failed_entry_to_completed_after_retry() {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(dir.path().join("history.db")).unwrap();
        let mut failed = entry(TranscriptionStatus::Failed);
        failed.error_message = Some("openai HTTP 500".to_string());
        db.insert(&failed).unwrap();

        db.update_completed(
            &failed.id,
            "retry text",
            Some("refined retry"),
            &Language::Chinese,
            2500,
            "groq",
        )
        .unwrap();
        let loaded = db.get(&failed.id).unwrap().unwrap();

        assert_eq!(loaded.status, TranscriptionStatus::Completed);
        assert_eq!(loaded.original_text, "retry text");
        assert_eq!(loaded.refined_text.as_deref(), Some("refined retry"));
        assert_eq!(loaded.error_message, None);
        assert_eq!(loaded.provider, "groq");
    }
}
