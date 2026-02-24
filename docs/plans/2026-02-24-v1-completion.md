# VoxInk Desktop v1.0 Completion Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Complete all remaining gaps to bring VoxInk Desktop from ~85% to v1.0 — SQLite history, overlay window, tray icon states, debouncing, edge case handling.

**Architecture:** Wire existing stubs to real implementations. The voxink-core crate has all SQL constants and types defined. The Tauri app crate needs: (1) DB init on startup, (2) history insertion in hotkey handler, (3) real SQL queries in commands, (4) overlay as secondary always-on-top window, (5) tray icon/tooltip state updates, (6) hotkey debouncing, (7) API key reading from Tauri store.

**Tech Stack:** Tauri v2, tauri-plugin-sql (SQLite), tokio, cpal, React 19, TypeScript, Tailwind CSS

---

### Task 1: SQLite Database Initialization

**Files:**
- Modify: `src-tauri/src/lib.rs:28-59` (setup block)
- Modify: `src-tauri/src/state.rs:98-105` (add db handle to AppState)

**Step 1: Add DbPool type alias and import to state.rs**

Add a type alias for the database connection handle. tauri-plugin-sql manages the connection pool internally — commands access it via `app.state::<tauri_plugin_sql::DbPool>()`. However, for the hotkey handler (which doesn't run in a command context), we need to store a reference.

In `src-tauri/src/state.rs`, add a `db_path` field to AppState so the hotkey handler knows which DB to target:

```rust
// In state.rs, update AppState:
pub struct AppState {
    pub controller: Arc<Mutex<PipelineController<GroqSttProvider, GroqLlmProvider>>>,
    pub settings: Arc<Mutex<Settings>>,
    pub recorder: Arc<crate::audio::CpalRecorder>,
    pub clipboard: Arc<crate::clipboard::ArboardClipboard>,
    pub keyboard: Arc<crate::keyboard::EnigoKeyboard>,
}
```

No change needed — tauri-plugin-sql handles the pool. Commands access the DB via `app.state()`.

**Step 2: Initialize the database with CREATE TABLE in lib.rs setup**

In `src-tauri/src/lib.rs`, after plugin registration and before managing state, run the CREATE TABLE statement. Use `tauri_plugin_sql` to execute the migration.

```rust
// After .plugin(tauri_plugin_sql::Builder::new().build()) in setup():
// The SQL plugin uses `plugin:sql|execute` internally.
// We need to configure the DB path in tauri.conf.json first.
```

Actually, `tauri-plugin-sql` requires the frontend to call `Database.load()` to initialize. For Rust-side access, we'll use a different approach: use the SQL plugin's Rust API directly.

**Step 3: Update tauri.conf.json to not configure SQL plugin** (it auto-initializes)

No tauri.conf.json change needed for SQL — the plugin discovers DBs when `Database.load("sqlite:voxink.db")` is called from JS. But we need Rust-side DB access for history insertion in the hotkey handler.

**Alternative approach**: Use `tauri_plugin_sql`'s Rust-side API. The plugin exposes `DbPool` as managed state. We can access it from the hotkey handler via `app.state()`.

The simpler approach: have the frontend call `Database.load()` on startup, which initializes the DB. Then Rust commands use `tauri_plugin_sql`'s managed state.

**Step 4: Create database initialization in frontend**

Add DB init to `src/main.tsx`:

```typescript
import Database from "@tauri-apps/plugin-sql";

// Initialize SQLite database
const db = await Database.load("sqlite:voxink.db");
await db.execute(`CREATE TABLE IF NOT EXISTS transcriptions (
    id TEXT PRIMARY KEY NOT NULL,
    timestamp INTEGER NOT NULL,
    original_text TEXT NOT NULL,
    refined_text TEXT,
    language TEXT NOT NULL,
    audio_duration_ms INTEGER NOT NULL,
    provider TEXT NOT NULL
)`);
```

**Step 5: Run test to verify it compiles**

Run: `pnpm build`
Expected: Frontend builds successfully

**Step 6: Commit**

```bash
git add src/main.tsx
git commit -m "feat: initialize SQLite database on frontend startup"
```

---

### Task 2: Wire History Commands to SQLite

**Files:**
- Modify: `src-tauri/src/commands.rs:68-96`

**Step 1: Implement get_history command**

Replace the TODO stub with real SQLite query using `tauri_plugin_sql`:

```rust
use tauri_plugin_sql::{DbInstances, DbPool};

#[tauri::command]
pub async fn get_history(
    app: tauri::AppHandle,
    limit: u32,
    offset: u32,
) -> Result<Vec<TranscriptionEntry>, String> {
    let db_instances = app.state::<DbInstances>();
    let db = db_instances.0.lock().await;
    let pool = db.get("sqlite:voxink.db")
        .ok_or("database not initialized")?;

    let rows = sqlx::query_as::<_, TranscriptionEntry>(
        voxink_core::history::QUERY_SQL
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows)
}
```

Wait — `tauri-plugin-sql` doesn't expose its internal pool for direct Rust use. The plugin's API is designed for JS consumption via IPC. For Rust-side DB access, we need a different approach.

**Better approach**: Create our own SQLite connection in Rust using `rusqlite` (simpler) or `sqlx`. Since `tauri-plugin-sql` already provides SQLite via JS and we need Rust-side inserts from the hotkey handler, let's add `rusqlite` and manage our own DB connection.

Actually, re-reading the `tauri-plugin-sql` docs: the plugin does expose a Rust API. Let me use the simpler approach — manage our own connection with `rusqlite` for Rust-side operations, and have the JS commands also use this.

**Revised approach**: Drop `tauri-plugin-sql` for direct `rusqlite` usage in Rust. This is simpler and gives us full control. The commands use `rusqlite` directly, no JS database init needed.

**Step 1: Add rusqlite dependency**

In `src-tauri/Cargo.toml`, add:
```toml
rusqlite = { version = "0.32", features = ["bundled"] }
```

And optionally remove `tauri-plugin-sql` if we're not using it for anything else.

**Step 2: Create history module in Tauri crate**

Create `src-tauri/src/history.rs`:

```rust
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
        let conn = Connection::open(&path).map_err(|e| format!("failed to open DB: {e}"))?;
        conn.execute_batch(CREATE_TABLE_SQL).map_err(|e| format!("failed to create table: {e}"))?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn insert(&self, entry: &TranscriptionEntry) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
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
        ).map_err(|e| format!("insert failed: {e}"))?;
        Ok(())
    }

    pub fn query(&self, limit: u32, offset: u32) -> Result<Vec<TranscriptionEntry>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(voxink_core::history::QUERY_SQL)
            .map_err(|e| format!("query failed: {e}"))?;
        let rows = stmt.query_map(rusqlite::params![limit, offset], |row| {
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
        }).map_err(|e| format!("query failed: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| format!("row read failed: {e}"))
    }

    pub fn search(&self, query: &str, limit: u32, offset: u32) -> Result<Vec<TranscriptionEntry>, String> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{query}%");
        let mut stmt = conn.prepare(voxink_core::history::SEARCH_SQL)
            .map_err(|e| format!("search failed: {e}"))?;
        let rows = stmt.query_map(rusqlite::params![&pattern, &pattern, limit, offset], |row| {
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
        }).map_err(|e| format!("search failed: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| format!("row read failed: {e}"))
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(voxink_core::history::DELETE_SQL, rusqlite::params![id])
            .map_err(|e| format!("delete failed: {e}"))?;
        Ok(())
    }
}
```

**Step 3: Add HistoryDb to AppState**

Update `src-tauri/src/state.rs` AppState:
```rust
pub struct AppState {
    pub controller: Arc<Mutex<PipelineController<GroqSttProvider, GroqLlmProvider>>>,
    pub settings: Arc<Mutex<Settings>>,
    pub recorder: Arc<crate::audio::CpalRecorder>,
    pub clipboard: Arc<crate::clipboard::ArboardClipboard>,
    pub keyboard: Arc<crate::keyboard::EnigoKeyboard>,
    pub history: Arc<crate::history::HistoryDb>,
}
```

**Step 4: Initialize HistoryDb in lib.rs setup**

```rust
// In setup(), before creating AppState:
let app_data_dir = app.path().app_data_dir().expect("failed to get app data dir");
std::fs::create_dir_all(&app_data_dir).expect("failed to create app data dir");
let db_path = app_data_dir.join("voxink.db");
let history_db = crate::history::HistoryDb::open(db_path).expect("failed to open history DB");
```

**Step 5: Wire commands to use HistoryDb**

Update `src-tauri/src/commands.rs`:

```rust
#[tauri::command]
pub async fn get_history(
    state: tauri::State<'_, AppState>,
    limit: u32,
    offset: u32,
) -> Result<Vec<TranscriptionEntry>, String> {
    state.history.query(limit, offset)
}

#[tauri::command]
pub async fn search_history(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: u32,
    offset: u32,
) -> Result<Vec<TranscriptionEntry>, String> {
    state.history.search(&query, limit, offset)
}

#[tauri::command]
pub async fn delete_history_entry(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.history.delete(&id)
}
```

**Step 6: Run voxink-core tests**

Run: `cargo test -p voxink-core --manifest-path src-tauri/Cargo.toml`
Expected: All 89+ tests pass (existing tests unaffected)

**Step 7: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/history.rs src-tauri/src/state.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: wire SQLite history with rusqlite"
```

---

### Task 3: Save Transcription History in Hotkey Handler

**Files:**
- Modify: `src-tauri/src/hotkey.rs:39-86`

**Step 1: Import history types and generate UUID**

Add `uuid` dependency to `src-tauri/Cargo.toml`:
```toml
uuid = { version = "1", features = ["v4"] }
```

**Step 2: Insert history entry after successful transcription**

In `src-tauri/src/hotkey.rs`, after the pipeline result and before auto-paste:

```rust
// After getting result from on_stop_recording:
if let Ok(text) = &result {
    // Save to history
    let settings = settings.lock().await;
    let entry = voxink_core::history::TranscriptionEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64,
        original_text: text.clone(),
        refined_text: None, // Will be set below if refined
        language: settings.stt_language.clone(),
        audio_duration_ms: (pcm_data_len as u64 * 1000) / 16000,
        provider: settings.stt_provider.clone(),
    };
    drop(settings);

    let _ = history.insert(&entry);
}
```

We need to track the original vs refined text. The pipeline result from `on_stop_recording` returns the final text. We need to distinguish whether refinement happened.

Better approach: check the pipeline state after `on_stop_recording`:

```rust
let ctrl = controller.lock().await;
let result = ctrl.on_stop_recording(pcm_data).await;
let final_state = ctrl.current_state();
drop(ctrl);

if let Ok(final_text) = &result {
    let (original, refined) = match &final_state {
        PipelineState::Refined { original, refined } => {
            (original.clone(), Some(refined.clone()))
        }
        _ => (final_text.clone(), None),
    };

    let s = settings.lock().await;
    let entry = TranscriptionEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64,
        original_text: original,
        refined_text: refined,
        language: s.stt_language.clone(),
        audio_duration_ms: (pcm_len as u64 * 1000) / 16000,
        provider: s.stt_provider.clone(),
    };
    drop(s);

    let _ = history.insert(&entry);
}
```

**Step 3: Run voxink-core tests**

Run: `cargo test -p voxink-core --manifest-path src-tauri/Cargo.toml`
Expected: All tests pass

**Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/hotkey.rs
git commit -m "feat: save transcription history after each dictation"
```

---

### Task 4: Overlay as Tauri Secondary Window

**Files:**
- Modify: `src-tauri/tauri.conf.json:13-22` (add overlay window)
- Modify: `src-tauri/capabilities/desktop.json` (add overlay window)
- Modify: `src/App.tsx` (route based on window label)
- Modify: `src/main.tsx` (handle overlay window)
- Modify: `src-tauri/src/lib.rs` (show/hide overlay on state change)

**Step 1: Add overlay window to tauri.conf.json**

```json
{
  "label": "overlay",
  "title": "VoxInk Overlay",
  "width": 200,
  "height": 80,
  "resizable": false,
  "visible": false,
  "decorations": false,
  "alwaysOnTop": true,
  "skipTaskbar": true,
  "transparent": true,
  "x": 20,
  "y": 20
}
```

**Step 2: Add overlay to capabilities**

Update `src-tauri/capabilities/desktop.json`:
```json
"windows": ["settings", "overlay"]
```

Add `core:event:default` permission for event listening:
```json
"core:event:default"
```

**Step 3: Route based on window label in App.tsx**

```typescript
import { useState, useEffect } from "react";
import { getCurrent } from "@tauri-apps/api/window";

function App() {
  const [windowLabel, setWindowLabel] = useState<string | null>(null);

  useEffect(() => {
    setWindowLabel(getCurrent().label);
  }, []);

  if (windowLabel === "overlay") return <Overlay />;
  return <SettingsWindow />;  // default to settings
}
```

**Step 4: Show/hide overlay window on pipeline state changes**

In `src-tauri/src/lib.rs`, in the state broadcast loop, show the overlay when recording starts and hide it after done/error:

```rust
// In the state broadcast loop:
while rx.changed().await.is_ok() {
    let pipeline_state = rx.borrow().clone();
    let _ = app_handle.emit("pipeline-state", &pipeline_state);

    // Show/hide overlay window
    if let Some(overlay) = app_handle.get_webview_window("overlay") {
        match &pipeline_state {
            PipelineState::Idle => { let _ = overlay.hide(); }
            _ => { let _ = overlay.show(); }
        }
    }
}
```

**Step 5: Build frontend to verify**

Run: `pnpm build`
Expected: Frontend builds successfully

**Step 6: Commit**

```bash
git add src-tauri/tauri.conf.json src-tauri/capabilities/desktop.json src/App.tsx src-tauri/src/lib.rs
git commit -m "feat: add overlay as secondary always-on-top window"
```

---

### Task 5: Tray Icon State Updates

**Files:**
- Modify: `src-tauri/src/lib.rs:62-100` (state broadcast + tray update)

**Step 1: Store tray icon reference for dynamic updates**

The `TrayIconBuilder::build()` returns a `TrayIcon`. We need to clone it into the background task so it can update tooltip on state changes.

```rust
// After building tray icon, clone for the state loop:
let tray = TrayIconBuilder::new()
    .menu(&menu)
    .menu_on_left_click(true)
    .tooltip("VoxInk — Ready")
    .on_menu_event(|app, event| match event.id.as_ref() {
        "settings" => { /* ... */ }
        "quit" => { app.exit(0); }
        _ => {}
    })
    .build(app)?;

// Clone tray for background state update loop
let tray_for_state = tray.clone();
```

**Step 2: Update tooltip in state broadcast loop**

```rust
// In the state emission task:
tauri::async_runtime::spawn(async move {
    let ctrl = controller.lock().await;
    let mut rx = ctrl.subscribe();
    drop(ctrl);
    while rx.changed().await.is_ok() {
        let pipeline_state = rx.borrow().clone();
        let _ = app_handle.emit("pipeline-state", &pipeline_state);

        // Update tray tooltip based on state
        let tooltip = match &pipeline_state {
            PipelineState::Idle => "VoxInk — Ready",
            PipelineState::Recording => "VoxInk — Recording...",
            PipelineState::Processing | PipelineState::Refining { .. } => "VoxInk — Processing...",
            PipelineState::Result { .. } | PipelineState::Refined { .. } => "VoxInk — Done",
            PipelineState::Error { .. } => "VoxInk — Error",
        };
        let _ = tray_for_state.set_tooltip(Some(tooltip));
    }
});
```

**Step 3: Build frontend to verify**

Run: `pnpm build`
Expected: Frontend builds successfully

**Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: update tray tooltip on pipeline state changes"
```

---

### Task 6: Hotkey Debouncing and Edge Cases

**Files:**
- Modify: `src-tauri/src/hotkey.rs` (add debounce guard)

**Step 1: Add debounce via AtomicBool guard**

Prevent concurrent hotkey handlers by using an atomic flag:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn register_hotkey(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let app_handle = app.clone();
    let processing = Arc::new(AtomicBool::new(false));

    app.global_shortcut().on_shortcut(
        "CommandOrControl+Shift+V",
        move |_app, _shortcut, event| {
            let app = app_handle.clone();
            let state: tauri::State<'_, AppState> = app.state();
            let processing = processing.clone();

            match event.state {
                ShortcutState::Pressed => {
                    // Debounce: skip if already processing
                    if processing.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
                        return;
                    }
                    // ... existing press handler
                }
                ShortcutState::Released => {
                    // ... existing release handler, reset processing flag at end
                    // After reset to idle:
                    processing.store(false, Ordering::SeqCst);
                }
            }
        },
    )?;
    Ok(())
}
```

**Step 2: Emit errors to frontend instead of just stderr**

In the press handler, emit errors so the overlay can show them:

```rust
if let Err(e) = ctrl.on_start_recording() {
    let _ = app.emit("pipeline-state", &PipelineState::Error {
        message: e.to_string(),
    });
    processing.store(false, Ordering::SeqCst);
    return;
}
```

**Step 3: Run voxink-core tests**

Run: `cargo test -p voxink-core --manifest-path src-tauri/Cargo.toml`
Expected: All tests pass

**Step 4: Commit**

```bash
git add src-tauri/src/hotkey.rs
git commit -m "feat: add hotkey debouncing and emit errors to frontend"
```

---

### Task 7: Read API Key from Tauri Store

**Files:**
- Modify: `src-tauri/src/state.rs:87-96` (read from store before env fallback)
- Modify: `src-tauri/src/state.rs:19-27` (GroqSttProvider needs AppHandle)
- Modify: `src-tauri/src/lib.rs` (pass app handle to providers)

**Step 1: Update providers to accept AppHandle for store access**

```rust
pub struct GroqSttProvider {
    settings: Arc<Mutex<Settings>>,
    app_handle: tauri::AppHandle,
}

impl GroqSttProvider {
    pub fn new(settings: Arc<Mutex<Settings>>, app_handle: tauri::AppHandle) -> Self {
        Self { settings, app_handle }
    }
}
```

**Step 2: Update get_api_key to read from Tauri store first**

```rust
fn get_api_key_from_store(app: &tauri::AppHandle, provider: &str) -> Result<String, AppError> {
    use tauri_plugin_store::StoreExt;

    // Try Tauri store first (encrypted secrets)
    if let Ok(store) = app.store("secrets.json") {
        let store_key = format!("{}_api_key", provider);
        if let Some(value) = store.get(&store_key) {
            if let Some(key) = value.as_str() {
                if !key.is_empty() {
                    return Ok(key.to_string());
                }
            }
        }
    }

    // Fallback: environment variable (for development)
    let env_key = format!("{}_API_KEY", provider.to_uppercase());
    if let Ok(key) = std::env::var(&env_key) {
        if !key.is_empty() {
            return Ok(key);
        }
    }

    Err(AppError::ApiKeyMissing(provider.to_string()))
}
```

**Step 3: Update lib.rs to pass app handle to providers**

```rust
let stt = GroqSttProvider::new(settings.clone(), app.handle().clone());
let llm = GroqLlmProvider::new(settings.clone(), app.handle().clone());
```

**Step 4: Run voxink-core tests**

Run: `cargo test -p voxink-core --manifest-path src-tauri/Cargo.toml`
Expected: All tests pass (provider changes are in Tauri crate, not core)

**Step 5: Commit**

```bash
git add src-tauri/src/state.rs src-tauri/src/lib.rs
git commit -m "feat: read API keys from Tauri store before env fallback"
```

---

### Task 8: Remove tauri-plugin-sql (replaced by rusqlite)

**Files:**
- Modify: `src-tauri/Cargo.toml` (remove tauri-plugin-sql)
- Modify: `src-tauri/src/lib.rs` (remove sql plugin registration)
- Modify: `src-tauri/capabilities/desktop.json` (remove sql:default)
- Modify: `package.json` (remove @tauri-apps/plugin-sql)

**Step 1: Remove tauri-plugin-sql from Cargo.toml**

Remove line: `tauri-plugin-sql = { version = "2", features = ["sqlite"] }`

**Step 2: Remove sql plugin registration from lib.rs**

Remove: `.plugin(tauri_plugin_sql::Builder::new().build())`

**Step 3: Remove sql:default from capabilities**

Remove `"sql:default"` from desktop.json permissions.

**Step 4: Remove @tauri-apps/plugin-sql from package.json**

Remove: `"@tauri-apps/plugin-sql": "^2"`

**Step 5: Build frontend**

Run: `pnpm build`
Expected: Frontend builds (no JS imports of plugin-sql exist)

**Step 6: Run core tests**

Run: `cargo test -p voxink-core --manifest-path src-tauri/Cargo.toml`
Expected: All tests pass

**Step 7: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/lib.rs src-tauri/capabilities/desktop.json package.json
git commit -m "refactor: replace tauri-plugin-sql with direct rusqlite"
```

---

### Task 9: Settings Sync to Pipeline Controller

**Files:**
- Modify: `src-tauri/src/commands.rs:17-24` (sync settings to controller on save)

**Step 1: Update save_settings to sync controller config**

When the user saves settings, update the pipeline controller's config so changes take effect immediately:

```rust
#[tauri::command]
pub async fn save_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    settings: Settings,
) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let value = serde_json::to_value(&settings).map_err(|e| e.to_string())?;
    store.set("settings", value);
    store.save().map_err(|e| e.to_string())?;

    // Sync to shared settings (used by providers at call time)
    *state.settings.lock().await = settings;

    Ok(())
}
```

Also update `get_settings` to sync the shared settings on load:

```rust
#[tauri::command]
pub async fn get_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Settings, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let settings = match store.get("settings") {
        Some(value) => serde_json::from_value(value.clone()).map_err(|e| e.to_string())?,
        None => Settings::default(),
    };

    // Sync to shared settings
    *state.settings.lock().await = settings.clone();

    Ok(settings)
}
```

**Step 2: Run core tests**

Run: `cargo test -p voxink-core --manifest-path src-tauri/Cargo.toml`
Expected: All tests pass

**Step 3: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat: sync settings to shared state on save/load"
```

---

### Task 10: Update plan.md with v1.0 Completion Status

**Files:**
- Modify: `plan.md`

**Step 1: Mark all completed items**

Update Phase 1.6, 1.7, 3.2, 4.3, 4.4 checklists to reflect completed work.

**Step 2: Update v1.0 readiness assessment**

Change from ~85% to ~95% — remaining: platform testing (needs real hardware).

**Step 3: Commit**

```bash
git add plan.md
git commit -m "docs: update plan.md to reflect v1.0 completion"
```
