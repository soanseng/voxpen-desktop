use serde::{Deserialize, Serialize};

/// A single dictionary vocabulary entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DictionaryEntry {
    pub id: i64,
    pub word: String,
    pub created_at: i64,
}

/// SQL to create the dictionary_entries table.
pub const CREATE_TABLE_SQL: &str = "\
CREATE TABLE IF NOT EXISTS dictionary_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    word TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL
)";

/// SQL to insert a dictionary entry, ignoring duplicates.
pub const INSERT_SQL: &str = "\
INSERT OR IGNORE INTO dictionary_entries (word, created_at) VALUES (?, ?)";

/// SQL to query all entries, newest first.
pub const QUERY_ALL_SQL: &str = "\
SELECT id, word, created_at FROM dictionary_entries ORDER BY created_at DESC";

/// SQL to get words only (for prompt injection), newest first with limit.
pub const GET_WORDS_SQL: &str = "\
SELECT word FROM dictionary_entries ORDER BY created_at DESC LIMIT ?";

/// SQL to count total entries.
pub const COUNT_SQL: &str = "SELECT COUNT(*) FROM dictionary_entries";

/// SQL to delete a single entry by id.
pub const DELETE_SQL: &str = "DELETE FROM dictionary_entries WHERE id = ?";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_serialize_entry_to_json() {
        let entry = DictionaryEntry {
            id: 1,
            word: "語墨".to_string(),
            created_at: 1_700_000_000,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("語墨"));
        assert!(json.contains("1700000000"));
    }

    #[test]
    fn should_roundtrip_entry_through_json() {
        let entry = DictionaryEntry {
            id: 42,
            word: "Anthropic".to_string(),
            created_at: 1_700_000_000,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: DictionaryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, deserialized);
    }

    #[test]
    fn should_have_create_table_sql_with_unique_constraint() {
        assert!(CREATE_TABLE_SQL.contains("UNIQUE"));
        assert!(CREATE_TABLE_SQL.contains("dictionary_entries"));
    }

    #[test]
    fn should_have_insert_sql_with_ignore() {
        assert!(INSERT_SQL.contains("INSERT OR IGNORE"));
    }
}
