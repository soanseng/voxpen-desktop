use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

use voxink_core::history::{TranscriptionEntry, CREATE_TABLE_SQL};
use voxink_core::pipeline::state::Language;

/// Thread-safe SQLite database handle for history operations.
pub struct HistoryDb {
    conn: Mutex<Connection>,
}

impl HistoryDb {
    /// Open (or create) the SQLite database at the given path and run migrations.
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let conn =
            Connection::open(&path).map_err(|e| format!("failed to open DB: {e}"))?;
        conn.execute_batch(CREATE_TABLE_SQL)
            .map_err(|e| format!("failed to create table: {e}"))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Insert a transcription entry.
    pub fn insert(&self, entry: &TranscriptionEntry) -> Result<(), String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            voxink_core::history::INSERT_SQL,
            rusqlite::params![
                entry.id,
                entry.timestamp,
                entry.original_text,
                entry.refined_text,
                serde_json::to_string(&entry.language).unwrap_or_default(),
                entry.audio_duration_ms,
                entry.provider,
            ],
        )
        .map_err(|e| format!("insert failed: {e}"))?;
        Ok(())
    }

    /// Query transcriptions with limit and offset, newest first.
    pub fn query(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<TranscriptionEntry>, String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn
            .prepare(voxink_core::history::QUERY_SQL)
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
            .prepare(voxink_core::history::SEARCH_SQL)
            .map_err(|e| format!("search prepare failed: {e}"))?;
        let rows = stmt
            .query_map(
                rusqlite::params![&pattern, &pattern, limit, offset],
                row_to_entry,
            )
            .map_err(|e| format!("search failed: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("row read failed: {e}"))
    }

    /// Delete a single transcription by id.
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(voxink_core::history::DELETE_SQL, rusqlite::params![id])
            .map_err(|e| format!("delete failed: {e}"))?;
        Ok(())
    }
}

/// Map a rusqlite row to a TranscriptionEntry.
fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<TranscriptionEntry> {
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
    })
}
