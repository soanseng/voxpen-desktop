use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

use voxink_core::dictionary::{DictionaryEntry, CREATE_TABLE_SQL};

/// Thread-safe SQLite database handle for dictionary operations.
pub struct DictionaryDb {
    conn: Mutex<Connection>,
}

impl DictionaryDb {
    /// Open (or create) the SQLite database at the given path and run migrations.
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let conn =
            Connection::open(&path).map_err(|e| format!("failed to open DB: {e}"))?;
        conn.execute_batch(CREATE_TABLE_SQL)
            .map_err(|e| format!("failed to create dictionary table: {e}"))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Add a word to the dictionary. Trims whitespace. Ignores duplicates.
    pub fn add(&self, word: &str) -> Result<(), String> {
        let trimmed = word.trim();
        if trimmed.is_empty() {
            return Err("word cannot be empty".to_string());
        }
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        conn.execute(
            voxink_core::dictionary::INSERT_SQL,
            rusqlite::params![trimmed, now],
        )
        .map_err(|e| format!("insert failed: {e}"))?;
        Ok(())
    }

    /// Get all dictionary entries, newest first.
    pub fn get_all(&self) -> Result<Vec<DictionaryEntry>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(voxink_core::dictionary::QUERY_ALL_SQL)
            .map_err(|e| format!("query prepare failed: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(DictionaryEntry {
                    id: row.get(0)?,
                    word: row.get(1)?,
                    created_at: row.get(2)?,
                })
            })
            .map_err(|e| format!("query failed: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("row read failed: {e}"))
    }

    /// Get word strings only (for prompt injection), newest first with limit.
    pub fn get_words(&self, limit: u32) -> Result<Vec<String>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(voxink_core::dictionary::GET_WORDS_SQL)
            .map_err(|e| format!("query prepare failed: {e}"))?;
        let rows = stmt
            .query_map(rusqlite::params![limit], |row| row.get(0))
            .map_err(|e| format!("query failed: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("row read failed: {e}"))
    }

    /// Count total dictionary entries.
    pub fn count(&self) -> Result<usize, String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(voxink_core::dictionary::COUNT_SQL, [], |row| {
            row.get::<_, usize>(0)
        })
        .map_err(|e| format!("count failed: {e}"))
    }

    /// Delete a dictionary entry by id.
    pub fn delete(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            voxink_core::dictionary::DELETE_SQL,
            rusqlite::params![id],
        )
        .map_err(|e| format!("delete failed: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_test_db() -> DictionaryDb {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        std::mem::forget(dir);
        DictionaryDb::open(path).unwrap()
    }

    #[test]
    fn should_open_and_create_table() {
        let db = open_test_db();
        assert_eq!(db.count().unwrap(), 0);
    }

    #[test]
    fn should_add_and_retrieve_entry() {
        let db = open_test_db();
        db.add("語墨").unwrap();
        let entries = db.get_all().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].word, "語墨");
    }

    #[test]
    fn should_trim_whitespace_on_add() {
        let db = open_test_db();
        db.add("  Anthropic  ").unwrap();
        let entries = db.get_all().unwrap();
        assert_eq!(entries[0].word, "Anthropic");
    }

    #[test]
    fn should_reject_empty_word() {
        let db = open_test_db();
        let result = db.add("   ");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn should_ignore_duplicate_words() {
        let db = open_test_db();
        db.add("語墨").unwrap();
        db.add("語墨").unwrap();
        assert_eq!(db.count().unwrap(), 1);
    }

    #[test]
    fn should_delete_entry_by_id() {
        let db = open_test_db();
        db.add("語墨").unwrap();
        let entries = db.get_all().unwrap();
        db.delete(entries[0].id).unwrap();
        assert_eq!(db.count().unwrap(), 0);
    }

    #[test]
    fn should_return_words_with_limit() {
        let db = open_test_db();
        db.add("word1").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.add("word2").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.add("word3").unwrap();

        let words = db.get_words(2).unwrap();
        assert_eq!(words.len(), 2);
        assert_eq!(words[0], "word3");
        assert_eq!(words[1], "word2");
    }

    #[test]
    fn should_order_entries_newest_first() {
        let db = open_test_db();
        db.add("first").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.add("second").unwrap();

        let entries = db.get_all().unwrap();
        assert_eq!(entries[0].word, "second");
        assert_eq!(entries[1].word, "first");
    }
}
