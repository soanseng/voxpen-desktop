use serde::{Deserialize, Serialize};

use crate::pipeline::state::Language;

/// Lifecycle state for a transcription history entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionStatus {
    Completed,
    Failed,
}

impl TranscriptionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "failed" => Self::Failed,
            _ => Self::Completed,
        }
    }
}

/// A single transcription history entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TranscriptionEntry {
    pub id: String,
    pub timestamp: i64,
    pub original_text: String,
    pub refined_text: Option<String>,
    pub language: Language,
    pub audio_duration_ms: u64,
    pub provider: String,
    #[serde(default = "default_status")]
    pub status: TranscriptionStatus,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub audio_path: Option<String>,
}

impl TranscriptionEntry {
    /// Returns the best available text -- refined if available, otherwise original.
    pub fn display_text(&self) -> &str {
        self.refined_text.as_deref().unwrap_or(&self.original_text)
    }
}

fn default_status() -> TranscriptionStatus {
    TranscriptionStatus::Completed
}

/// SQL to create the transcriptions table.
pub const CREATE_TABLE_SQL: &str = "\
CREATE TABLE IF NOT EXISTS transcriptions (
    id TEXT PRIMARY KEY NOT NULL,
    timestamp INTEGER NOT NULL,
    original_text TEXT NOT NULL,
    refined_text TEXT,
    language TEXT NOT NULL,
    audio_duration_ms INTEGER NOT NULL,
    provider TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'completed',
    error_message TEXT,
    audio_path TEXT
)";

/// SQL to insert a transcription entry.
pub const INSERT_SQL: &str = "\
INSERT INTO transcriptions (
    id, timestamp, original_text, refined_text, language, audio_duration_ms,
    provider, status, error_message, audio_path
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

/// SQL to query transcriptions with limit and offset, newest first.
pub const QUERY_SQL: &str = "\
SELECT id, timestamp, original_text, refined_text, language, audio_duration_ms,
       provider, status, error_message, audio_path
FROM transcriptions ORDER BY timestamp DESC LIMIT ? OFFSET ?";

/// SQL to search transcriptions by text content.
pub const SEARCH_SQL: &str = "\
SELECT id, timestamp, original_text, refined_text, language, audio_duration_ms,
       provider, status, error_message, audio_path
FROM transcriptions
WHERE original_text LIKE ? OR refined_text LIKE ? OR error_message LIKE ?
ORDER BY timestamp DESC LIMIT ? OFFSET ?";

/// SQL to get a single transcription by id.
pub const GET_BY_ID_SQL: &str = "\
SELECT id, timestamp, original_text, refined_text, language, audio_duration_ms,
       provider, status, error_message, audio_path
FROM transcriptions WHERE id = ?";

/// SQL to mark a failed transcription as completed after retry.
pub const UPDATE_COMPLETED_SQL: &str = "\
UPDATE transcriptions
SET original_text = ?, refined_text = ?, language = ?, audio_duration_ms = ?,
    provider = ?, status = 'completed', error_message = NULL
WHERE id = ?";

/// SQL to update a failed transcription after another failed retry.
pub const UPDATE_FAILED_SQL: &str = "\
UPDATE transcriptions
SET provider = ?, status = 'failed', error_message = ?
WHERE id = ?";

/// SQL to delete a single transcription by id.
pub const DELETE_SQL: &str = "DELETE FROM transcriptions WHERE id = ?";

/// SQL to delete all transcriptions.
pub const DELETE_ALL_SQL: &str = "DELETE FROM transcriptions";

/// SQL to delete transcriptions older than a given timestamp.
pub const CLEANUP_SQL: &str = "DELETE FROM transcriptions WHERE timestamp < ?";

/// SQL to count total transcriptions.
pub const COUNT_SQL: &str = "SELECT COUNT(*) FROM transcriptions";

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(refined: Option<&str>) -> TranscriptionEntry {
        TranscriptionEntry {
            id: "abc-123".to_string(),
            timestamp: 1_700_000_000,
            original_text: "raw transcription".to_string(),
            refined_text: refined.map(String::from),
            language: Language::Chinese,
            audio_duration_ms: 5_000,
            provider: "groq".to_string(),
            status: TranscriptionStatus::Completed,
            error_message: None,
            audio_path: None,
        }
    }

    fn failed_entry() -> TranscriptionEntry {
        TranscriptionEntry {
            id: "failed-123".to_string(),
            timestamp: 1_700_000_001,
            original_text: String::new(),
            refined_text: None,
            language: Language::Auto,
            audio_duration_ms: 250,
            provider: "openai".to_string(),
            status: TranscriptionStatus::Failed,
            error_message: Some("openai HTTP 500: upstream failed".to_string()),
            audio_path: Some("/tmp/recording.wav".to_string()),
        }
    }

    #[test]
    fn should_return_refined_text_as_display_when_available() {
        let entry = sample_entry(Some("polished text"));
        assert_eq!(entry.display_text(), "polished text");
    }

    #[test]
    fn should_return_original_text_as_display_when_no_refinement() {
        let entry = sample_entry(None);
        assert_eq!(entry.display_text(), "raw transcription");
    }

    #[test]
    fn should_roundtrip_through_json() {
        let entry = sample_entry(Some("refined"));
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: TranscriptionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, deserialized);
    }

    #[test]
    fn should_serialize_entry_with_null_refined_text() {
        let entry = sample_entry(None);
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains(r#""refined_text":null"#));

        let deserialized: TranscriptionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.refined_text, None);
    }

    #[test]
    fn should_serialize_failed_entry_with_retry_metadata() {
        let entry = failed_entry();
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains(r#""status":"failed""#));
        assert!(json.contains(r#""error_message":"openai HTTP 500: upstream failed""#));
        assert!(json.contains(r#""audio_path":"/tmp/recording.wav""#));
    }
}
