# Custom Vocabulary (Dictionary) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Allow users to maintain a personal word list that biases both STT (Whisper prompt) and LLM refinement (system prompt injection) for improved accuracy — especially Chinese homophones like 語墨 vs 語末.

**Architecture:** New `dictionary` module in `voxpen-core` with SQLite CRUD (mirroring `history` module pattern). A new `DictionaryDb` Tauri wrapper (mirroring `HistoryDb`). Vocabulary is fetched once per recording stop, appended to both Whisper prompt and LLM system prompt. A new "Vocabulary" tab in the React settings UI provides add/delete management.

**Tech Stack:** Rust (rusqlite), Tauri IPC commands, React + Tailwind CSS, react-i18next

---

### Task 1: Dictionary Core Module — Entity + SQL Constants

**Files:**
- Create: `src-tauri/crates/voxpen-core/src/dictionary.rs`
- Modify: `src-tauri/crates/voxpen-core/src/lib.rs`

**Step 1: Write the failing test**

Add to `src-tauri/crates/voxpen-core/src/dictionary.rs`:

```rust
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
```

**Step 2: Register the module**

In `src-tauri/crates/voxpen-core/src/lib.rs`, add:

```rust
pub mod dictionary;
```

(Add after `pub mod history;`)

**Step 3: Run tests to verify they pass**

Run: `cargo test -p voxpen-core dictionary`
Expected: 4 tests PASS

**Step 4: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/dictionary.rs src-tauri/crates/voxpen-core/src/lib.rs
git commit -m "feat: add dictionary entry struct and SQL constants"
```

---

### Task 2: DictionaryDb Tauri Wrapper — CRUD with SQLite

**Files:**
- Create: `src-tauri/src/dictionary.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod dictionary;`)

**Step 1: Write the failing test**

Create `src-tauri/src/dictionary.rs`:

```rust
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

use voxpen_core::dictionary::{DictionaryEntry, CREATE_TABLE_SQL};

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
            voxpen_core::dictionary::INSERT_SQL,
            rusqlite::params![trimmed, now],
        )
        .map_err(|e| format!("insert failed: {e}"))?;
        Ok(())
    }

    /// Get all dictionary entries, newest first.
    pub fn get_all(&self) -> Result<Vec<DictionaryEntry>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(voxpen_core::dictionary::QUERY_ALL_SQL)
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
            .prepare(voxpen_core::dictionary::GET_WORDS_SQL)
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
        conn.query_row(voxpen_core::dictionary::COUNT_SQL, [], |row| {
            row.get::<_, usize>(0)
        })
        .map_err(|e| format!("count failed: {e}"))
    }

    /// Delete a dictionary entry by id.
    pub fn delete(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            voxpen_core::dictionary::DELETE_SQL,
            rusqlite::params![id],
        )
        .map_err(|e| format!("delete failed: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn open_memory_db() -> DictionaryDb {
        // Use a temp file so rusqlite doesn't share in-memory DBs
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        // Keep dir alive by leaking — tests are short-lived
        std::mem::forget(dir);
        DictionaryDb::open(path).unwrap()
    }

    #[test]
    fn should_open_and_create_table() {
        let db = open_memory_db();
        assert_eq!(db.count().unwrap(), 0);
    }

    #[test]
    fn should_add_and_retrieve_entry() {
        let db = open_memory_db();
        db.add("語墨").unwrap();
        let entries = db.get_all().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].word, "語墨");
    }

    #[test]
    fn should_trim_whitespace_on_add() {
        let db = open_memory_db();
        db.add("  Anthropic  ").unwrap();
        let entries = db.get_all().unwrap();
        assert_eq!(entries[0].word, "Anthropic");
    }

    #[test]
    fn should_reject_empty_word() {
        let db = open_memory_db();
        let result = db.add("   ");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn should_ignore_duplicate_words() {
        let db = open_memory_db();
        db.add("語墨").unwrap();
        db.add("語墨").unwrap(); // Should not error, just ignore
        assert_eq!(db.count().unwrap(), 1);
    }

    #[test]
    fn should_delete_entry_by_id() {
        let db = open_memory_db();
        db.add("語墨").unwrap();
        let entries = db.get_all().unwrap();
        db.delete(entries[0].id).unwrap();
        assert_eq!(db.count().unwrap(), 0);
    }

    #[test]
    fn should_return_words_with_limit() {
        let db = open_memory_db();
        db.add("word1").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.add("word2").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.add("word3").unwrap();

        let words = db.get_words(2).unwrap();
        assert_eq!(words.len(), 2);
        // Newest first
        assert_eq!(words[0], "word3");
        assert_eq!(words[1], "word2");
    }

    #[test]
    fn should_order_entries_newest_first() {
        let db = open_memory_db();
        db.add("first").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.add("second").unwrap();

        let entries = db.get_all().unwrap();
        assert_eq!(entries[0].word, "second");
        assert_eq!(entries[1].word, "first");
    }
}
```

**Step 2: Register the module**

In `src-tauri/src/lib.rs`, add after `mod history;`:

```rust
mod dictionary;
```

**Step 3: Add `tempfile` dev-dependency**

In `src-tauri/Cargo.toml`, add under `[dev-dependencies]`:

```toml
tempfile = "3"
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p voxpen-app dictionary`
Expected: 8 tests PASS

**Step 5: Commit**

```bash
git add src-tauri/src/dictionary.rs src-tauri/src/lib.rs src-tauri/Cargo.toml
git commit -m "feat: add DictionaryDb with CRUD operations and tests"
```

---

### Task 3: Wire DictionaryDb into AppState + Tauri IPC Commands

**Files:**
- Modify: `src-tauri/src/state.rs:124-131` — add `dictionary` field to `AppState`
- Modify: `src-tauri/src/lib.rs:52-70` — init DictionaryDb alongside HistoryDb
- Modify: `src-tauri/src/commands.rs` — add 4 new commands
- Modify: `src-tauri/src/lib.rs:146-154` — register new commands

**Step 1: Add `dictionary` to `AppState`**

In `src-tauri/src/state.rs`, add after the `history` field (line 130):

```rust
    pub dictionary: Arc<crate::dictionary::DictionaryDb>,
```

**Step 2: Initialize `DictionaryDb` in setup**

In `src-tauri/src/lib.rs`, after the `history_db` initialization (around line 60), add:

```rust
            let dictionary_db =
                dictionary::DictionaryDb::open(db_path.clone())
                    .expect("failed to open dictionary DB");
```

Note: Reuse the same `db_path` — `DictionaryDb::open` uses its own `CREATE TABLE IF NOT EXISTS` so sharing the SQLite file is fine. But actually, `db_path` is `app_data_dir.join("voxpen.db")` — both tables live in the same database file, but `DictionaryDb` opens a **separate connection**. This is fine for SQLite WAL mode. Alternatively, we can use the same DB path.

Actually, looking at the code, `HistoryDb::open` opens a new `Connection` to the given path. We should use the same path so both tables live in one DB file. This means `DictionaryDb::open(db_path.clone())` will work — but `db_path` is moved into `HistoryDb::open`. We need to clone it first.

Fix: Change the `HistoryDb` init to:
```rust
            let db_path = app_data_dir.join("voxpen.db");
            let history_db =
                history::HistoryDb::open(db_path.clone()).expect("failed to open history DB");
            let dictionary_db =
                dictionary::DictionaryDb::open(db_path).expect("failed to open dictionary DB");
```

And add `dictionary` to the `AppState` struct initialization:

```rust
            let app_state = AppState {
                controller: Arc::new(Mutex::new(controller)),
                settings,
                recorder: Arc::new(recorder),
                clipboard: Arc::new(clipboard_mgr),
                keyboard: Arc::new(keyboard_mgr),
                history: Arc::new(history_db),
                dictionary: Arc::new(dictionary_db),
            };
```

**Step 3: Add Tauri IPC commands**

Add to `src-tauri/src/commands.rs`:

```rust
use voxpen_core::dictionary::DictionaryEntry;

/// Get all dictionary entries.
#[tauri::command]
pub async fn get_dictionary_entries(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<DictionaryEntry>, String> {
    state.dictionary.get_all()
}

/// Get dictionary entry count.
#[tauri::command]
pub async fn get_dictionary_count(
    state: tauri::State<'_, AppState>,
) -> Result<usize, String> {
    state.dictionary.count()
}

/// Add a word to the dictionary.
#[tauri::command]
pub async fn add_dictionary_entry(
    state: tauri::State<'_, AppState>,
    word: String,
) -> Result<(), String> {
    state.dictionary.add(&word)
}

/// Delete a dictionary entry by id.
#[tauri::command]
pub async fn delete_dictionary_entry(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    state.dictionary.delete(id)
}
```

**Step 4: Register commands in invoke_handler**

In `src-tauri/src/lib.rs`, add to the `generate_handler!` macro (line 146-154):

```rust
            commands::get_dictionary_entries,
            commands::get_dictionary_count,
            commands::add_dictionary_entry,
            commands::delete_dictionary_entry,
```

**Step 5: Run build to verify compilation**

Run: `cd /home/scipio/projects/voxpen-desktop && cargo build -p voxpen-app`
Expected: BUILD SUCCESS

**Step 6: Commit**

```bash
git add src-tauri/src/state.rs src-tauri/src/lib.rs src-tauri/src/commands.rs
git commit -m "feat: wire DictionaryDb into AppState and Tauri IPC commands"
```

---

### Task 4: Vocabulary Prompt Builder — STT + LLM Injection

**Files:**
- Create: `src-tauri/crates/voxpen-core/src/pipeline/vocabulary.rs`
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/mod.rs`

This module builds prompt strings from vocabulary words for both Whisper and LLM.

**Step 1: Write the failing tests + implementation**

Create `src-tauri/crates/voxpen-core/src/pipeline/vocabulary.rs`:

```rust
use crate::pipeline::state::Language;

/// Maximum token budget for Whisper prompt vocabulary.
/// Base prompt uses ~10-20 tokens, leaving ~200 for vocabulary.
const WHISPER_VOCAB_TOKEN_BUDGET: usize = 200;

/// Estimate token count for a string.
/// CJK character ≈ 2 tokens, Latin character ≈ 0.25 tokens.
fn estimate_tokens(s: &str) -> usize {
    let mut tokens = 0.0_f64;
    for ch in s.chars() {
        if ch.is_ascii() {
            tokens += 0.25;
        } else {
            tokens += 2.0;
        }
    }
    tokens.ceil() as usize
}

/// Build vocabulary hint string for Whisper STT prompt parameter.
///
/// Appends vocabulary words to the language-specific base prompt, separated by `, `.
/// Truncates from the end (oldest entries) if over token budget.
/// Returns `None` if vocabulary is empty.
pub fn build_stt_hint(words: &[String], language: &Language) -> Option<String> {
    if words.is_empty() {
        return None;
    }

    let base = language.prompt();
    let separator = " ";
    let mut result = base.to_string();
    let base_tokens = estimate_tokens(&result);
    let mut remaining = WHISPER_VOCAB_TOKEN_BUDGET.saturating_sub(base_tokens);

    let mut added = Vec::new();
    for word in words {
        let word_tokens = estimate_tokens(word) + 1; // +1 for ", " separator
        if word_tokens > remaining {
            break;
        }
        added.push(word.as_str());
        remaining -= word_tokens;
    }

    if added.is_empty() {
        return None;
    }

    result.push_str(separator);
    result.push_str(&added.join(", "));
    Some(result)
}

/// Build vocabulary suffix for LLM refinement system prompt.
///
/// Returns a localized suffix like:
/// - Chinese: `\n\n術語表（請優先使用這些詞彙）：語墨, Anthropic`
/// - English: `\n\nVocabulary (prefer these terms): VoxPen, Anthropic`
/// Returns `None` if vocabulary is empty.
pub fn build_llm_suffix(words: &[String], language: &Language) -> Option<String> {
    if words.is_empty() {
        return None;
    }

    let joined = words.join(", ");
    let suffix = match language {
        Language::English => format!("\n\nVocabulary (prefer these terms): {joined}"),
        Language::Japanese => format!("\n\n用語集（以下の用語を優先してください）：{joined}"),
        _ => format!("\n\n術語表（請優先使用這些詞彙）：{joined}"),
    };
    Some(suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_return_none_for_empty_vocabulary_stt() {
        assert!(build_stt_hint(&[], &Language::Chinese).is_none());
    }

    #[test]
    fn should_return_none_for_empty_vocabulary_llm() {
        assert!(build_llm_suffix(&[], &Language::Chinese).is_none());
    }

    #[test]
    fn should_append_words_to_base_prompt() {
        let words = vec!["語墨".to_string(), "Anthropic".to_string()];
        let hint = build_stt_hint(&words, &Language::Chinese).unwrap();
        assert!(hint.starts_with("繁體中文轉錄。"));
        assert!(hint.contains("語墨"));
        assert!(hint.contains("Anthropic"));
    }

    #[test]
    fn should_truncate_when_over_token_budget() {
        // Create many long CJK words to exceed budget
        let words: Vec<String> = (0..200)
            .map(|i| format!("很長的詞彙名稱{i}"))
            .collect();
        let hint = build_stt_hint(&words, &Language::Chinese).unwrap();
        // Should not contain all 200 words
        let comma_count = hint.matches(',').count();
        assert!(comma_count < 199, "should truncate, got {comma_count} commas");
    }

    #[test]
    fn should_build_chinese_llm_suffix() {
        let words = vec!["語墨".to_string(), "Anthropic".to_string()];
        let suffix = build_llm_suffix(&words, &Language::Chinese).unwrap();
        assert!(suffix.contains("術語表"));
        assert!(suffix.contains("語墨, Anthropic"));
    }

    #[test]
    fn should_build_english_llm_suffix() {
        let words = vec!["VoxPen".to_string()];
        let suffix = build_llm_suffix(&words, &Language::English).unwrap();
        assert!(suffix.contains("Vocabulary (prefer these terms)"));
        assert!(suffix.contains("VoxPen"));
    }

    #[test]
    fn should_build_japanese_llm_suffix() {
        let words = vec!["語墨".to_string()];
        let suffix = build_llm_suffix(&words, &Language::Japanese).unwrap();
        assert!(suffix.contains("用語集"));
    }

    #[test]
    fn should_use_chinese_suffix_for_auto_language() {
        let words = vec!["語墨".to_string()];
        let suffix = build_llm_suffix(&words, &Language::Auto).unwrap();
        assert!(suffix.contains("術語表"));
    }

    #[test]
    fn should_estimate_cjk_tokens_higher_than_ascii() {
        let cjk_tokens = estimate_tokens("語墨");
        let ascii_tokens = estimate_tokens("VoxPen");
        assert!(cjk_tokens > ascii_tokens);
    }
}
```

**Step 2: Register the module**

In `src-tauri/crates/voxpen-core/src/pipeline/mod.rs`, add:

```rust
pub mod vocabulary;
```

**Step 3: Run tests**

Run: `cargo test -p voxpen-core vocabulary`
Expected: 9 tests PASS

**Step 4: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/vocabulary.rs src-tauri/crates/voxpen-core/src/pipeline/mod.rs
git commit -m "feat: add vocabulary prompt builder for STT and LLM injection"
```

---

### Task 5: Integrate Vocabulary into STT Pipeline

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/state.rs:56-63` — add `vocabulary_prompt()` to `Language`
- Modify: `src-tauri/crates/voxpen-core/src/api/groq.rs:85-96` — use vocabulary hint in prompt field
- Modify: `src-tauri/crates/voxpen-core/src/api/groq.rs:24-52` — add `prompt_override` to `SttConfig`
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/transcribe.rs:9-16` — accept vocabulary hint

**Step 1: Add `prompt_override` to `SttConfig`**

In `src-tauri/crates/voxpen-core/src/api/groq.rs`, add a field to `SttConfig`:

```rust
pub struct SttConfig {
    pub api_key: String,
    pub model: String,
    pub language: Language,
    pub response_format: String,
    /// If set, overrides `language.prompt()` for the Whisper `prompt` parameter.
    /// Used to inject vocabulary hints.
    pub prompt_override: Option<String>,
}
```

Update `SttConfig::new`:

```rust
    pub fn new(api_key: String, language: Language) -> Self {
        Self {
            api_key,
            model: DEFAULT_STT_MODEL.to_string(),
            language,
            response_format: "verbose_json".to_string(),
            prompt_override: None,
        }
    }
```

**Step 2: Use `prompt_override` in the API call**

In `transcribe_with_base_url` (line 96), change:

```rust
    // Always add prompt hint
    form = form.text("prompt", config.language.prompt().to_string());
```

To:

```rust
    // Add prompt hint — use override (with vocabulary) if available
    let prompt = config
        .prompt_override
        .as_deref()
        .unwrap_or(config.language.prompt());
    form = form.text("prompt", prompt.to_string());
```

**Step 3: Update `transcribe.rs` to accept vocabulary**

In `src-tauri/crates/voxpen-core/src/pipeline/transcribe.rs`, change the signature:

```rust
/// Orchestrate PCM → WAV → STT transcription.
///
/// If `vocabulary_hint` is provided, it overrides the default language prompt
/// to include vocabulary words for improved recognition accuracy.
pub async fn transcribe(
    pcm_data: &[i16],
    config: &SttConfig,
    vocabulary_hint: Option<&str>,
) -> Result<String, AppError> {
    if pcm_data.is_empty() {
        return Err(AppError::Audio("no audio data".to_string()));
    }

    let mut config = config.clone();
    if let Some(hint) = vocabulary_hint {
        config.prompt_override = Some(hint.to_string());
    }

    let wav_data = encoder::pcm_to_wav(pcm_data);
    groq::transcribe(&config, &wav_data).await
}
```

Update the test helper similarly:

```rust
#[cfg(test)]
async fn transcribe_with_base_url(
    pcm_data: &[i16],
    config: &SttConfig,
    base_url: &str,
) -> Result<String, AppError> {
    if pcm_data.is_empty() {
        return Err(AppError::Audio("no audio data".to_string()));
    }

    let wav_data = encoder::pcm_to_wav(pcm_data);
    groq::transcribe_with_base_url(config, &wav_data, base_url).await
}
```

**Step 4: Fix call sites**

In `src-tauri/src/state.rs` (GroqSttProvider), update the `transcribe` call (line 52):

```rust
            transcribe::transcribe(&pcm_data, &config, None).await
```

**Step 5: Run tests to make sure nothing broke**

Run: `cargo test -p voxpen-core`
Expected: All existing tests PASS (the `transcribe` tests call the internal `transcribe_with_base_url` which doesn't go through the modified public function)

**Step 6: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/api/groq.rs src-tauri/crates/voxpen-core/src/pipeline/transcribe.rs src-tauri/src/state.rs
git commit -m "feat: add vocabulary hint support to STT pipeline"
```

---

### Task 6: Integrate Vocabulary into LLM Refinement Pipeline

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/refine.rs:10-17` — accept vocabulary
- Modify: `src-tauri/src/state.rs:72-93` — pass vocabulary through `GroqLlmProvider`

**Step 1: Update `refine()` to accept vocabulary**

In `src-tauri/crates/voxpen-core/src/pipeline/refine.rs`:

```rust
use crate::api::groq::{self, ChatConfig};
use crate::error::AppError;
use crate::pipeline::prompts;
use crate::pipeline::state::Language;
use crate::pipeline::vocabulary;

/// Orchestrate text refinement via LLM.
///
/// If `vocab_words` is non-empty, appends a vocabulary suffix to the system prompt.
pub async fn refine(
    text: &str,
    config: &ChatConfig,
    language: &Language,
    vocab_words: &[String],
) -> Result<String, AppError> {
    if text.is_empty() {
        return Err(AppError::Refinement("no text to refine".to_string()));
    }

    let mut system_prompt = prompts::for_language(language).to_string();
    if let Some(suffix) = vocabulary::build_llm_suffix(vocab_words, language) {
        system_prompt.push_str(&suffix);
    }
    groq::chat_completion(config, &system_prompt, text).await
}
```

Update the test helper:

```rust
#[cfg(test)]
async fn refine_with_base_url(
    text: &str,
    config: &ChatConfig,
    language: &Language,
    base_url: &str,
) -> Result<String, AppError> {
    if text.is_empty() {
        return Err(AppError::Refinement("no text to refine".to_string()));
    }

    let system_prompt = prompts::for_language(language);
    groq::chat_completion_with_base_url(config, system_prompt, text, base_url).await
}
```

**Step 2: Fix call site in `GroqLlmProvider`**

In `src-tauri/src/state.rs`, update line 90:

```rust
            refine::refine(&text, &config, &language, &[]).await
```

**Step 3: Add test for vocabulary in refinement**

Add to the `tests` module in `refine.rs`:

```rust
    #[tokio::test]
    async fn should_reject_empty_text_with_vocabulary() {
        let config = test_config("key");
        let vocab = vec!["語墨".to_string()];
        let result = refine("", &config, &Language::Auto, &vocab).await;
        assert!(matches!(result, Err(AppError::Refinement(_))));
    }
```

Note: The full integration test with vocabulary + wiremock is hard to assert on the prompt content since `chat_completion_with_base_url` is the internal function. The vocabulary builder is already tested in Task 4. The refine function just composes them.

**Step 4: Run tests**

Run: `cargo test -p voxpen-core refine`
Expected: All tests PASS

Run: `cargo build -p voxpen-app`
Expected: BUILD SUCCESS

**Step 5: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/refine.rs src-tauri/src/state.rs
git commit -m "feat: add vocabulary injection to LLM refinement pipeline"
```

---

### Task 7: Thread Vocabulary Through Provider Traits + Pipeline Controller

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/controller.rs:17-23,30-37` — add vocabulary to traits
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/controller.rs:145-200` — pass vocabulary in `on_stop_recording`
- Modify: `src-tauri/src/state.rs` — update trait impls
- Modify: `src-tauri/src/hotkey.rs` — fetch vocabulary and pass through

**Step 1: Update `SttProvider` trait**

In `controller.rs`, change `SttProvider`:

```rust
#[cfg_attr(test, mockall::automock)]
pub trait SttProvider: Send + Sync {
    /// Transcribe PCM audio data to text.
    /// `vocabulary_hint` is the pre-built prompt string with vocabulary words.
    fn transcribe(
        &self,
        pcm_data: Vec<i16>,
        vocabulary_hint: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, AppError>> + Send>>;
}
```

**Step 2: Update `LlmProvider` trait**

```rust
#[cfg_attr(test, mockall::automock)]
pub trait LlmProvider: Send + Sync {
    /// Refine transcribed text via LLM.
    /// `vocabulary` is the list of user vocabulary words.
    fn refine(
        &self,
        text: String,
        language: Language,
        vocabulary: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, AppError>> + Send>>;
}
```

**Step 3: Update `on_stop_recording` to accept vocabulary**

```rust
    pub async fn on_stop_recording(
        &self,
        pcm_data: Vec<i16>,
        vocabulary_hint: Option<String>,
        vocabulary_words: Vec<String>,
    ) -> Result<String, AppError> {
        if !matches!(self.current_state(), PipelineState::Recording) {
            return Err(AppError::Audio("not currently recording".to_string()));
        }

        let _ = self.state_tx.send(PipelineState::Processing);

        let raw_text = match self.stt.transcribe(pcm_data, vocabulary_hint).await {
            Ok(text) => text,
            Err(e) => {
                let _ = self.state_tx.send(PipelineState::Error {
                    message: e.to_string(),
                });
                return Err(e);
            }
        };

        if !self.config.refinement_enabled {
            let _ = self.state_tx.send(PipelineState::Result {
                text: raw_text.clone(),
            });
            return Ok(raw_text);
        }

        let _ = self.state_tx.send(PipelineState::Refining {
            original: raw_text.clone(),
        });

        let refine_result = tokio::time::timeout(
            REFINEMENT_TIMEOUT,
            self.llm.refine(
                raw_text.clone(),
                self.config.language.clone(),
                vocabulary_words,
            ),
        )
        .await;

        match refine_result {
            Ok(Ok(refined_text)) => {
                let _ = self.state_tx.send(PipelineState::Refined {
                    original: raw_text,
                    refined: refined_text.clone(),
                });
                Ok(refined_text)
            }
            Ok(Err(_)) | Err(_) => {
                let _ = self.state_tx.send(PipelineState::Result {
                    text: raw_text.clone(),
                });
                Ok(raw_text)
            }
        }
    }
```

**Step 4: Update `GroqSttProvider` in `state.rs`**

```rust
impl SttProvider for GroqSttProvider {
    fn transcribe(
        &self,
        pcm_data: Vec<i16>,
        vocabulary_hint: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, AppError>> + Send>> {
        let settings = self.settings.clone();
        let app_handle = self.app_handle.clone();
        Box::pin(async move {
            let s = settings.lock().await;
            let api_key = get_api_key(&app_handle, &s.stt_provider)?;
            let config = SttConfig {
                api_key,
                model: s.stt_model.clone(),
                language: s.stt_language.clone(),
                response_format: "verbose_json".to_string(),
                prompt_override: None,
            };
            drop(s);
            transcribe::transcribe(&pcm_data, &config, vocabulary_hint.as_deref()).await
        })
    }
}
```

**Step 5: Update `GroqLlmProvider` in `state.rs`**

```rust
impl LlmProvider for GroqLlmProvider {
    fn refine(
        &self,
        text: String,
        language: Language,
        vocabulary: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, AppError>> + Send>> {
        let settings = self.settings.clone();
        let app_handle = self.app_handle.clone();
        Box::pin(async move {
            let s = settings.lock().await;
            let api_key = get_api_key(&app_handle, &s.refinement_provider)?;
            let config = ChatConfig {
                api_key,
                model: s.refinement_model.clone(),
                temperature: groq::LLM_TEMPERATURE,
                max_tokens: groq::LLM_MAX_TOKENS,
            };
            drop(s);
            refine::refine(&text, &config, &language, &vocabulary).await
        })
    }
}
```

**Step 6: Update hotkey.rs — fetch vocabulary + pass to pipeline**

In `src-tauri/src/hotkey.rs`, in the `Released` handler, after `let pcm_len = pcm_data.len();` and the short-recording guard, add vocabulary fetching:

```rust
                        // Fetch vocabulary for prompt injection
                        let dictionary = state.dictionary.clone();
```

(Add `let dictionary = state.dictionary.clone();` alongside the other `state.xxx.clone()` calls around line 77.)

Then before the `ctrl.on_stop_recording` call, build the hint:

```rust
                        // Build vocabulary hints
                        let vocab_words = dictionary.get_words(500).unwrap_or_default();
                        let stt_lang = {
                            let s = settings.lock().await;
                            s.stt_language.clone()
                        };
                        let vocabulary_hint =
                            voxpen_core::pipeline::vocabulary::build_stt_hint(
                                &vocab_words, &stt_lang,
                            );

                        // Run pipeline: STT + optional LLM refinement
                        let ctrl = controller.lock().await;
                        let result = ctrl
                            .on_stop_recording(pcm_data, vocabulary_hint, vocab_words)
                            .await;
```

**Step 7: Fix all test mock expectations**

In `controller.rs` tests, update all mock builders to match new signatures:

```rust
    fn mock_stt_success(text: &str) -> MockSttProvider {
        let text = text.to_string();
        let mut mock = MockSttProvider::new();
        mock.expect_transcribe()
            .returning(move |_, _| Box::pin(std::future::ready(Ok(text.clone()))));
        mock
    }

    fn mock_stt_failure() -> MockSttProvider {
        let mut mock = MockSttProvider::new();
        mock.expect_transcribe().returning(|_, _| {
            Box::pin(std::future::ready(Err(AppError::Transcription(
                "mock failure".to_string(),
            ))))
        });
        mock
    }

    fn mock_llm_success(text: &str) -> MockLlmProvider {
        let text = text.to_string();
        let mut mock = MockLlmProvider::new();
        mock.expect_refine()
            .returning(move |_, _, _| Box::pin(std::future::ready(Ok(text.clone()))));
        mock
    }

    fn mock_llm_failure() -> MockLlmProvider {
        let mut mock = MockLlmProvider::new();
        mock.expect_refine().returning(|_, _, _| {
            Box::pin(std::future::ready(Err(AppError::Refinement(
                "mock failure".to_string(),
            ))))
        });
        mock
    }

    fn mock_llm_timeout() -> MockLlmProvider {
        let mut mock = MockLlmProvider::new();
        mock.expect_refine().returning(|_, _, _| {
            Box::pin(async {
                tokio::time::sleep(Duration::from_secs(10)).await;
                Ok("should not reach".to_string())
            })
        });
        mock
    }
```

Update all `on_stop_recording` calls in tests:

```rust
        let result = controller.on_stop_recording(vec![100, 200], None, vec![]).await;
```

**Step 8: Run all tests**

Run: `cargo test -p voxpen-core`
Expected: All tests PASS

Run: `cargo build -p voxpen-app`
Expected: BUILD SUCCESS

**Step 9: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/controller.rs src-tauri/src/state.rs src-tauri/src/hotkey.rs
git commit -m "feat: thread vocabulary through provider traits and pipeline controller"
```

---

### Task 8: React — TypeScript Types + Tauri Invoke Wrappers

**Files:**
- Create: `src/types/dictionary.ts`
- Modify: `src/lib/tauri.ts`

**Step 1: Create TypeScript types**

Create `src/types/dictionary.ts`:

```typescript
export interface DictionaryEntry {
  id: number;
  word: string;
  created_at: number;
}
```

**Step 2: Add invoke wrappers**

Add to `src/lib/tauri.ts`:

```typescript
import type { DictionaryEntry } from "../types/dictionary";

export async function getDictionaryEntries(): Promise<DictionaryEntry[]> {
  return invoke<DictionaryEntry[]>("get_dictionary_entries");
}

export async function getDictionaryCount(): Promise<number> {
  return invoke<number>("get_dictionary_count");
}

export async function addDictionaryEntry(word: string): Promise<void> {
  return invoke("add_dictionary_entry", { word });
}

export async function deleteDictionaryEntry(id: number): Promise<void> {
  return invoke("delete_dictionary_entry", { id });
}
```

**Step 3: Verify frontend builds**

Run: `cd /home/scipio/projects/voxpen-desktop && pnpm build`
Expected: BUILD SUCCESS (or `npx tsc --noEmit` passes)

**Step 4: Commit**

```bash
git add src/types/dictionary.ts src/lib/tauri.ts
git commit -m "feat: add dictionary TypeScript types and Tauri invoke wrappers"
```

---

### Task 9: React — VocabularySection Component

**Files:**
- Create: `src/components/Settings/VocabularySection.tsx`

**Step 1: Create the component**

Create `src/components/Settings/VocabularySection.tsx`:

```tsx
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { DictionaryEntry } from "../../types/dictionary";
import {
  addDictionaryEntry,
  deleteDictionaryEntry,
  getDictionaryEntries,
} from "../../lib/tauri";

export default function VocabularySection() {
  const { t } = useTranslation();
  const [entries, setEntries] = useState<DictionaryEntry[]>([]);
  const [inputValue, setInputValue] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const fetchEntries = async () => {
    try {
      const data = await getDictionaryEntries();
      setEntries(data);
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchEntries();
  }, []);

  const handleAdd = async () => {
    const trimmed = inputValue.trim();
    if (!trimmed) return;

    // Check for duplicate locally
    if (entries.some((e) => e.word === trimmed)) {
      setError(t("dictionary.duplicate"));
      setTimeout(() => setError(null), 2000);
      setInputValue("");
      inputRef.current?.focus();
      return;
    }

    try {
      await addDictionaryEntry(trimmed);
      setInputValue("");
      setError(null);
      await fetchEntries();
      inputRef.current?.focus();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDelete = async (id: number) => {
    try {
      await deleteDictionaryEntry(id);
      await fetchEntries();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      handleAdd();
    }
  };

  if (loading) {
    return (
      <div className="text-sm text-gray-500 dark:text-gray-400">
        {t("loadingSettings")}
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-base font-semibold text-gray-900 dark:text-gray-100">
          {t("dictionary.title")}
        </h2>
        <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
          {t("dictionary.description")}
        </p>
      </div>

      {/* Add word input */}
      <div className="flex gap-2">
        <input
          ref={inputRef}
          type="text"
          value={inputValue}
          onChange={(e) => setInputValue(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={t("dictionary.addHint")}
          className="flex-1 rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-900 placeholder-gray-400 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100 dark:placeholder-gray-500"
        />
        <button
          type="button"
          onClick={handleAdd}
          disabled={!inputValue.trim()}
          className="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50 dark:bg-blue-500 dark:hover:bg-blue-600"
        >
          {t("dictionary.addButton")}
        </button>
      </div>

      {/* Error/status message */}
      {error && (
        <p className="text-sm text-amber-600 dark:text-amber-400">{error}</p>
      )}

      {/* Word count */}
      <p className="text-sm text-gray-500 dark:text-gray-400">
        {t("dictionary.count", { count: entries.length })}
      </p>

      {/* Entry list */}
      {entries.length === 0 ? (
        <div className="rounded-lg border border-dashed border-gray-300 p-6 text-center dark:border-gray-600">
          <p className="text-sm text-gray-500 dark:text-gray-400">
            {t("dictionary.empty")}
          </p>
        </div>
      ) : (
        <ul className="divide-y divide-gray-200 rounded-lg border border-gray-200 dark:divide-gray-700 dark:border-gray-700">
          {entries.map((entry) => (
            <li
              key={entry.id}
              className="flex items-center justify-between px-4 py-3"
            >
              <span className="text-sm text-gray-900 dark:text-gray-100">
                {entry.word}
              </span>
              <button
                type="button"
                onClick={() => handleDelete(entry.id)}
                className="rounded p-1 text-gray-400 transition-colors hover:bg-gray-100 hover:text-gray-600 dark:hover:bg-gray-700 dark:hover:text-gray-300"
                aria-label={`${t("delete")} ${entry.word}`}
              >
                <svg
                  className="h-4 w-4"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth={2}
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M6 18L18 6M6 6l12 12"
                  />
                </svg>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
```

**Step 2: Verify frontend builds**

Run: `cd /home/scipio/projects/voxpen-desktop && pnpm build`
Expected: BUILD SUCCESS

**Step 3: Commit**

```bash
git add src/components/Settings/VocabularySection.tsx
git commit -m "feat: add VocabularySection component for dictionary management"
```

---

### Task 10: Wire Vocabulary Tab into SettingsWindow + i18n

**Files:**
- Modify: `src/components/Settings/SettingsWindow.tsx`
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh-TW.json`

**Step 1: Add i18n strings to `en.json`**

Add to `src/locales/en.json` (before the closing `}`):

```json
  "vocabulary": "Vocabulary",
  "vocabularyDescription": "Manage custom vocabulary to improve voice recognition accuracy.",
  "dictionary": {
    "title": "Custom Vocabulary",
    "description": "Add names, places, and terms to improve voice recognition accuracy.",
    "addHint": "Add a word...",
    "addButton": "Add",
    "count": "{{count}} words",
    "empty": "Add names, places, and terms to improve voice recognition accuracy.",
    "duplicate": "Word already exists.",
    "help": "Words you add here help the voice engine recognize names, places, and special terms more accurately."
  }
```

**Step 2: Add i18n strings to `zh-TW.json`**

Add to `src/locales/zh-TW.json` (before the closing `}`):

```json
  "vocabulary": "自定義詞彙",
  "vocabularyDescription": "管理自定義詞彙以提升語音辨識準確度。",
  "dictionary": {
    "title": "自定義詞彙",
    "description": "加入人名、地名、專有名詞，提升語音辨識準確度。",
    "addHint": "輸入新詞彙...",
    "addButton": "新增",
    "count": "{{count}} 個詞彙",
    "empty": "加入人名、地名、專有名詞，提升語音辨識準確度。",
    "duplicate": "此詞彙已存在。",
    "help": "加入的詞彙會協助語音引擎更準確地辨識人名、地名與專有名詞。"
  }
```

**Step 3: Add Vocabulary tab to SettingsWindow**

In `src/components/Settings/SettingsWindow.tsx`:

1. Add import:
```typescript
import VocabularySection from "./VocabularySection";
```

2. Update `Tab` type:
```typescript
type Tab = "general" | "speech" | "refinement" | "vocabulary" | "appearance" | "history";
```

3. Update `TAB_IDS`:
```typescript
const TAB_IDS: Tab[] = [
  "general",
  "speech",
  "refinement",
  "vocabulary",
  "appearance",
  "history",
];
```

4. Add icon case to `TabIcon` (before the `appearance` case):
```typescript
    case "vocabulary":
      return (
        <svg
          className={cls}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          strokeWidth={1.5}
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M12 6.042A8.967 8.967 0 006 3.75c-1.052 0-2.062.18-3 .512v14.25A8.987 8.987 0 016 18c2.305 0 4.408.867 6 2.292m0-14.25a8.966 8.966 0 016-2.292c1.052 0 2.062.18 3 .512v14.25A8.987 8.987 0 0018 18a8.967 8.967 0 00-6 2.292m0-14.25v14.25"
          />
        </svg>
      );
```

5. Add content rendering (after the `refinement` section, before `appearance`):
```typescript
          {activeTab === "vocabulary" && <VocabularySection />}
```

**Step 4: Verify frontend builds**

Run: `cd /home/scipio/projects/voxpen-desktop && pnpm build`
Expected: BUILD SUCCESS

**Step 5: Commit**

```bash
git add src/components/Settings/SettingsWindow.tsx src/locales/en.json src/locales/zh-TW.json
git commit -m "feat: add Vocabulary tab to settings with i18n support"
```

---

### Task 11: Full Build Verification

**Step 1: Run all Rust tests**

Run: `cd /home/scipio/projects/voxpen-desktop && cargo test --workspace`
Expected: All tests PASS

**Step 2: Run frontend build**

Run: `cd /home/scipio/projects/voxpen-desktop && pnpm build`
Expected: BUILD SUCCESS

**Step 3: Run full Tauri dev build**

Run: `cd /home/scipio/projects/voxpen-desktop && cargo build -p voxpen-app`
Expected: BUILD SUCCESS

**Step 4: Commit if any fixups needed**

If any fixes were required, commit them:

```bash
git add -A
git commit -m "fix: address build issues in vocabulary feature"
```

---

## Summary of All Files

| File | Action |
|------|--------|
| `src-tauri/crates/voxpen-core/src/dictionary.rs` | **Create** — entry struct + SQL constants |
| `src-tauri/crates/voxpen-core/src/lib.rs` | **Modify** — add `pub mod dictionary` |
| `src-tauri/crates/voxpen-core/src/pipeline/vocabulary.rs` | **Create** — prompt builder (STT hint + LLM suffix) |
| `src-tauri/crates/voxpen-core/src/pipeline/mod.rs` | **Modify** — add `pub mod vocabulary` |
| `src-tauri/crates/voxpen-core/src/pipeline/refine.rs` | **Modify** — accept `vocab_words` param |
| `src-tauri/crates/voxpen-core/src/pipeline/transcribe.rs` | **Modify** — accept `vocabulary_hint` param |
| `src-tauri/crates/voxpen-core/src/pipeline/controller.rs` | **Modify** — update traits + `on_stop_recording` |
| `src-tauri/crates/voxpen-core/src/api/groq.rs` | **Modify** — add `prompt_override` to `SttConfig` |
| `src-tauri/src/dictionary.rs` | **Create** — `DictionaryDb` wrapper with tests |
| `src-tauri/src/state.rs` | **Modify** — add `dictionary` to `AppState`, update trait impls |
| `src-tauri/src/commands.rs` | **Modify** — add 4 dictionary IPC commands |
| `src-tauri/src/lib.rs` | **Modify** — init `DictionaryDb`, register commands |
| `src-tauri/src/hotkey.rs` | **Modify** — fetch vocab, pass to pipeline |
| `src-tauri/Cargo.toml` | **Modify** — add `tempfile` dev-dep |
| `src/types/dictionary.ts` | **Create** — TypeScript type |
| `src/lib/tauri.ts` | **Modify** — add invoke wrappers |
| `src/components/Settings/VocabularySection.tsx` | **Create** — dictionary management UI |
| `src/components/Settings/SettingsWindow.tsx` | **Modify** — add Vocabulary tab |
| `src/locales/en.json` | **Modify** — add i18n strings |
| `src/locales/zh-TW.json` | **Modify** — add i18n strings |
