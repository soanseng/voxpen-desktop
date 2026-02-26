# Hotkey Configuration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Allow users to customize the global hotkey, supporting both combo keys (Ctrl+Shift+V) and single-key activation (Right Alt) like Typeless, with full i18n for en/zh-TW.

**Architecture:** Hybrid hotkey system — Tauri `global-shortcut` plugin handles combo shortcuts (modifier+key), while `rdev` crate handles single-key shortcuts (Right Alt, F-keys). A `HotkeyManager` abstraction in `hotkey.rs` encapsulates both backends, with dynamic register/unregister support. Frontend adds a hotkey recorder widget to GeneralSection.

**Tech Stack:** Tauri v2, `tauri-plugin-global-shortcut`, `rdev` crate, React + TypeScript, i18next

---

### Task 1: Add `rdev` dependency

**Files:**
- Modify: `src-tauri/Cargo.toml`

**Step 1: Add rdev to dependencies**

In `src-tauri/Cargo.toml`, add to `[dependencies]`:

```toml
rdev = "0.5"
```

**Step 2: Verify it compiles**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: Compiles without errors

**Step 3: Commit**

```
chore: add rdev dependency for single-key hotkey support
```

---

### Task 2: Refactor `hotkey.rs` — Hotkey parsing and HotkeyManager

**Files:**
- Modify: `src-tauri/src/hotkey.rs`

**Step 1: Add hotkey type detection and HotkeyManager**

Replace the current `register_hotkey` function with a `HotkeyManager` that supports both backends. The key insight: if the hotkey string contains `+`, it's a combo (use global-shortcut); otherwise it's a single key (use rdev).

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use tauri::AppHandle;
use tauri::Emitter;
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tokio::sync::Mutex;

use voxpen_core::audio::recorder::AudioRecorder;
use voxpen_core::input::paste::paste_text;
use voxpen_core::pipeline::state::PipelineState;

use crate::state::AppState;

/// Tracks the active hotkey registration so it can be swapped at runtime.
pub struct HotkeyManager {
    /// The currently registered shortcut string.
    current_shortcut: Option<String>,
    /// Stop signal for rdev listener thread.
    rdev_stop: Arc<AtomicBool>,
}

impl HotkeyManager {
    pub fn new() -> Self {
        Self {
            current_shortcut: None,
            rdev_stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Register a hotkey. Unregisters any previous hotkey first.
    pub fn register(&mut self, app: &AppHandle, shortcut: &str) -> Result<(), String> {
        // Unregister previous
        self.unregister(app);

        if is_combo_shortcut(shortcut) {
            self.register_combo(app, shortcut)?;
        } else {
            self.register_single_key(app, shortcut)?;
        }

        self.current_shortcut = Some(shortcut.to_string());
        Ok(())
    }

    /// Unregister the current hotkey.
    pub fn unregister(&mut self, app: &AppHandle) {
        if let Some(ref shortcut) = self.current_shortcut {
            if is_combo_shortcut(shortcut) {
                let _ = app.global_shortcut().unregister_all();
            }
            // Signal rdev thread to stop
            self.rdev_stop.store(true, Ordering::SeqCst);
            // Create fresh stop signal for next registration
            self.rdev_stop = Arc::new(AtomicBool::new(false));
        }
        self.current_shortcut = None;
    }

    pub fn current(&self) -> Option<&str> {
        self.current_shortcut.as_deref()
    }

    fn register_combo(&self, app: &AppHandle, shortcut: &str) -> Result<(), String> {
        let app_handle = app.clone();
        let processing = Arc::new(AtomicBool::new(false));

        app.global_shortcut()
            .on_shortcut(shortcut, move |_app, _shortcut, event| {
                let app = app_handle.clone();
                let state: tauri::State<'_, AppState> = app.state();
                handle_hotkey_event(&app, &state, event.state, &processing);
            })
            .map_err(|e| format!("Failed to register shortcut '{}': {}", shortcut, e))?;

        Ok(())
    }

    fn register_single_key(&self, app: &AppHandle, key_name: &str) -> Result<(), String> {
        let rdev_key = parse_rdev_key(key_name)
            .ok_or_else(|| format!("Unknown key: {}", key_name))?;

        let app_handle = app.clone();
        let stop = self.rdev_stop.clone();
        let processing = Arc::new(AtomicBool::new(false));

        thread::spawn(move || {
            let processing_press = processing.clone();
            let processing_release = processing.clone();
            let app_press = app_handle.clone();
            let app_release = app_handle.clone();
            let stop_clone = stop.clone();

            // Track key state to emit press/release
            let key_down = Arc::new(AtomicBool::new(false));
            let key_down_press = key_down.clone();
            let key_down_release = key_down.clone();

            let _ = rdev::listen(move |event| {
                if stop_clone.load(Ordering::SeqCst) {
                    // Cannot stop rdev::listen from inside callback,
                    // but we can ignore events
                    return;
                }

                match event.event_type {
                    rdev::EventType::KeyPress(k) if k == rdev_key => {
                        if !key_down_press.swap(true, Ordering::SeqCst) {
                            let app = app_press.clone();
                            let state: tauri::State<'_, AppState> = app.state();
                            handle_hotkey_event(
                                &app,
                                &state,
                                ShortcutState::Pressed,
                                &processing_press,
                            );
                        }
                    }
                    rdev::EventType::KeyRelease(k) if k == rdev_key => {
                        if key_down_release.swap(false, Ordering::SeqCst) {
                            let app = app_release.clone();
                            let state: tauri::State<'_, AppState> = app.state();
                            handle_hotkey_event(
                                &app,
                                &state,
                                ShortcutState::Released,
                                &processing_release,
                            );
                        }
                    }
                    _ => {}
                }
            });
        });

        Ok(())
    }
}

/// Returns true if shortcut string is a combo (contains '+').
fn is_combo_shortcut(s: &str) -> bool {
    s.contains('+')
}

/// Map a single-key name to an rdev::Key.
fn parse_rdev_key(name: &str) -> Option<rdev::Key> {
    match name {
        "RAlt" => Some(rdev::Key::AltGr),
        "LAlt" => Some(rdev::Key::Alt),
        "RControl" => Some(rdev::Key::ControlRight),
        "LControl" => Some(rdev::Key::ControlLeft),
        "RShift" => Some(rdev::Key::ShiftRight),
        "LShift" => Some(rdev::Key::ShiftLeft),
        "F13" => Some(rdev::Key::F13),
        "F14" => Some(rdev::Key::F14),
        "F15" => Some(rdev::Key::F15),
        "F16" => Some(rdev::Key::F16),
        "F17" => Some(rdev::Key::F17),
        "F18" => Some(rdev::Key::F18),
        "F19" => Some(rdev::Key::F19),
        "F20" => Some(rdev::Key::F20),
        _ => None,
    }
}

/// Shared press/release handler used by both combo and single-key backends.
fn handle_hotkey_event(
    app: &AppHandle,
    state: &tauri::State<'_, AppState>,
    shortcut_state: ShortcutState,
    processing: &Arc<AtomicBool>,
) {
    match shortcut_state {
        ShortcutState::Pressed => {
            if processing
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                return;
            }

            let controller = state.controller.clone();
            let recorder = state.recorder.clone();
            let app_for_err = app.clone();
            let processing_flag = processing.clone();

            tauri::async_runtime::spawn(async move {
                let ctrl = controller.lock().await;
                if let Err(e) = ctrl.on_start_recording() {
                    let _ = app_for_err.emit(
                        "pipeline-state",
                        &PipelineState::Error {
                            message: e.to_string(),
                        },
                    );
                    processing_flag.store(false, Ordering::SeqCst);
                    return;
                }
                drop(ctrl);
                if let Err(e) = recorder.start() {
                    eprintln!("audio start error: {e}");
                    let _ = app_for_err.emit(
                        "pipeline-state",
                        &PipelineState::Error {
                            message: e.to_string(),
                        },
                    );
                    processing_flag.store(false, Ordering::SeqCst);
                }
            });
        }
        ShortcutState::Released => {
            let controller = state.controller.clone();
            let recorder = state.recorder.clone();
            let clipboard = state.clipboard.clone();
            let keyboard = state.keyboard.clone();
            let settings = state.settings.clone();
            let history = state.history.clone();
            let dictionary = state.dictionary.clone();
            let processing_flag = processing.clone();

            tauri::async_runtime::spawn(async move {
                let pcm_data = match recorder.stop() {
                    Ok(data) => data,
                    Err(e) => {
                        eprintln!("audio stop error: {e}");
                        processing_flag.store(false, Ordering::SeqCst);
                        return;
                    }
                };

                let pcm_len = pcm_data.len();

                if pcm_len < 8000 {
                    let ctrl = controller.lock().await;
                    ctrl.reset();
                    processing_flag.store(false, Ordering::SeqCst);
                    return;
                }

                let vocab_words = dictionary.get_words(500).unwrap_or_default();
                let stt_lang = {
                    let s = settings.lock().await;
                    s.stt_language.clone()
                };
                let vocabulary_hint =
                    voxpen_core::pipeline::vocabulary::build_stt_hint(
                        &vocab_words, &stt_lang,
                    );

                let ctrl = controller.lock().await;
                let result = ctrl
                    .on_stop_recording(pcm_data, vocabulary_hint, vocab_words)
                    .await;
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
                    let entry = voxpen_core::history::TranscriptionEntry {
                        id: uuid::Uuid::new_v4().to_string(),
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs()
                            as i64,
                        original_text: original,
                        refined_text: refined,
                        language: s.stt_language.clone(),
                        audio_duration_ms: (pcm_len as u64 * 1000) / 16000,
                        provider: s.stt_provider.clone(),
                    };
                    let auto_paste = s.auto_paste;
                    drop(s);

                    if let Err(e) = history.insert(&entry) {
                        eprintln!("history insert error: {e}");
                    }

                    if auto_paste {
                        let text = final_text.clone();
                        let cb = clipboard.clone();
                        let kb = keyboard.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            paste_text(cb.as_ref(), kb.as_ref(), &text)
                        })
                        .await;
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                let ctrl = controller.lock().await;
                ctrl.reset();
                drop(ctrl);

                processing_flag.store(false, Ordering::SeqCst);
            });
        }
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

**Step 3: Commit**

```
refactor: extract HotkeyManager with combo + single-key support
```

---

### Task 3: Add HotkeyManager to AppState and wire up in lib.rs

**Files:**
- Modify: `src-tauri/src/state.rs` — Add `hotkey_manager: Arc<Mutex<HotkeyManager>>`
- Modify: `src-tauri/src/lib.rs` — Initialize HotkeyManager, register from settings

**Step 1: Add hotkey_manager to AppState**

In `state.rs`, add field:

```rust
pub hotkey_manager: Arc<Mutex<crate::hotkey::HotkeyManager>>,
```

**Step 2: Initialize in lib.rs**

In `lib.rs` setup:
- Create `HotkeyManager::new()`
- Add to `AppState`
- Read saved hotkey from settings
- Call `hotkey_manager.register(app, &settings.hotkey)`

Replace the current `hotkey::register_hotkey(app.handle())` call with:

```rust
// Initialize hotkey manager
let hotkey_mgr = hotkey::HotkeyManager::new();
// ... add to AppState ...

// Register hotkey from saved settings
{
    let state: tauri::State<'_, AppState> = app.state();
    let settings = state.settings.lock().await; // Can't await in sync setup
    // Actually read from store directly:
    let store = app.store("settings.json").ok();
    let saved_hotkey = store
        .and_then(|s| s.get("settings"))
        .and_then(|v| v.get("hotkey").and_then(|h| h.as_str().map(String::from)))
        .unwrap_or_else(|| "CommandOrControl+Shift+V".to_string());

    let mut mgr = state.hotkey_manager.blocking_lock();
    if let Err(e) = mgr.register(app.handle(), &saved_hotkey) {
        eprintln!("failed to register hotkey: {e}");
    }
}
```

**Step 3: Verify it compiles**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

**Step 4: Commit**

```
feat: wire HotkeyManager into AppState and setup
```

---

### Task 4: Add `set_hotkey` Tauri command

**Files:**
- Modify: `src-tauri/src/commands.rs` — Add `set_hotkey` command
- Modify: `src-tauri/src/lib.rs` — Register the new command

**Step 1: Add the command**

```rust
/// Change the global hotkey at runtime.
#[tauri::command]
pub async fn set_hotkey(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    shortcut: String,
) -> Result<(), String> {
    // Validate non-empty
    if shortcut.trim().is_empty() {
        return Err("Hotkey cannot be empty".to_string());
    }

    // Register new hotkey
    let mut mgr = state.hotkey_manager.lock().await;
    mgr.register(&app, &shortcut)?;
    drop(mgr);

    // Update settings
    let mut s = state.settings.lock().await;
    s.hotkey = shortcut;
    let settings_clone = s.clone();
    drop(s);

    // Persist
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let value = serde_json::to_value(&settings_clone).map_err(|e| e.to_string())?;
    store.set("settings", value);
    store.save().map_err(|e| e.to_string())?;

    Ok(())
}
```

**Step 2: Register in lib.rs invoke_handler**

Add `commands::set_hotkey` to the `generate_handler!` macro.

**Step 3: Verify it compiles**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

**Step 4: Commit**

```
feat: add set_hotkey Tauri command for runtime hotkey changes
```

---

### Task 5: Update i18n strings

**Files:**
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh-TW.json`

**Step 1: Add hotkey configuration strings to en.json**

Replace `hotkeyHint` and add new keys:

```json
"hotkeyHint": "Click Change to set a new shortcut.",
"hotkeyChange": "Change",
"hotkeyRecording": "Press your desired key or key combination...",
"hotkeySave": "Save",
"hotkeyCancel": "Cancel",
"hotkeyPresetAlt": "Right Alt (Recommended)",
"hotkeyPresetCombo": "Ctrl+Shift+V",
"hotkeyError": "Failed to register hotkey: {{error}}",
"hotkeySuccess": "Hotkey updated successfully.",
```

**Step 2: Add corresponding zh-TW strings**

```json
"hotkeyHint": "點擊「更改」設定新的快捷鍵。",
"hotkeyChange": "更改",
"hotkeyRecording": "請按下您想要的按鍵或組合鍵...",
"hotkeySave": "儲存",
"hotkeyCancel": "取消",
"hotkeyPresetAlt": "右 Alt（推薦）",
"hotkeyPresetCombo": "Ctrl+Shift+V",
"hotkeyError": "快捷鍵註冊失敗：{{error}}",
"hotkeySuccess": "快捷鍵更新成功。",
```

**Step 3: Update version string**

In both files, update `"version": "VoxPen v0.3.0"`.

**Step 4: Commit**

```
feat: add hotkey configuration i18n strings for en and zh-TW
```

---

### Task 6: Build hotkey recorder widget in GeneralSection

**Files:**
- Modify: `src/components/Settings/GeneralSection.tsx`

**Step 1: Replace the static hotkey display with a recorder widget**

The widget has three states:
1. **Display mode** — Shows current hotkey + "Change" button
2. **Recording mode** — Listens for key press, shows "Press your desired key..."
3. **Saving** — Calls `set_hotkey` command, shows result

Key behavior:
- In recording mode, listen for `keydown` event on a focused div
- Map `event.code` to our shortcut format:
  - Single key: `event.code` without modifiers → map to our format (e.g., `AltRight` → `RAlt`)
  - Combo: build from modifiers + key (e.g., `Control+Shift+KeyV` → `CommandOrControl+Shift+V`)
- Preset buttons for common choices
- Call `invoke("set_hotkey", { shortcut })` on save

```tsx
import { useState, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

// Inside GeneralSection, replace the hotkey section:

const [hotkeyMode, setHotkeyMode] = useState<"display" | "recording">("display");
const [pendingHotkey, setPendingHotkey] = useState("");
const [hotkeyError, setHotkeyError] = useState("");
const recorderRef = useRef<HTMLDivElement>(null);

function formatKeyEvent(e: React.KeyboardEvent): string {
  e.preventDefault();
  e.stopPropagation();

  const parts: string[] = [];

  // Check if it's a standalone modifier key
  const standaloneModifiers: Record<string, string> = {
    AltRight: "RAlt",
    AltLeft: "LAlt",
    ControlRight: "RControl",
    ControlLeft: "LControl",
    ShiftRight: "RShift",
    ShiftLeft: "LShift",
  };

  if (standaloneModifiers[e.code]) {
    return standaloneModifiers[e.code];
  }

  // Build combo
  if (e.ctrlKey || e.metaKey) parts.push("CommandOrControl");
  if (e.shiftKey) parts.push("Shift");
  if (e.altKey) parts.push("Alt");

  // Map key code to Tauri format
  const keyMap: Record<string, string> = {
    KeyA: "A", KeyB: "B", KeyC: "C", KeyD: "D", KeyE: "E",
    KeyF: "F", KeyG: "G", KeyH: "H", KeyI: "I", KeyJ: "J",
    KeyK: "K", KeyL: "L", KeyM: "M", KeyN: "N", KeyO: "O",
    KeyP: "P", KeyQ: "Q", KeyR: "R", KeyS: "S", KeyT: "T",
    KeyU: "U", KeyV: "V", KeyW: "W", KeyX: "X", KeyY: "Y",
    KeyZ: "Z",
    Digit0: "0", Digit1: "1", Digit2: "2", Digit3: "3",
    Digit4: "4", Digit5: "5", Digit6: "6", Digit7: "7",
    Digit8: "8", Digit9: "9",
    Space: "Space", Enter: "Enter", Escape: "Escape",
    F1: "F1", F2: "F2", F3: "F3", F4: "F4", F5: "F5",
    F6: "F6", F7: "F7", F8: "F8", F9: "F9", F10: "F10",
    F11: "F11", F12: "F12", F13: "F13", F14: "F14",
    F15: "F15", F16: "F16", F17: "F17", F18: "F18",
    F19: "F19", F20: "F20",
  };

  const key = keyMap[e.code];
  if (key) {
    parts.push(key);
  }

  return parts.join("+");
}

async function saveHotkey(shortcut: string) {
  try {
    await invoke("set_hotkey", { shortcut });
    onUpdate("hotkey", shortcut);
    setHotkeyMode("display");
    setHotkeyError("");
  } catch (err) {
    setHotkeyError(String(err));
  }
}

async function selectPreset(shortcut: string) {
  setPendingHotkey(shortcut);
  await saveHotkey(shortcut);
}
```

**Step 2: Render the hotkey section**

```tsx
{/* Hotkey */}
<div className="space-y-2">
  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
    {t("hotkey")}
  </label>

  {hotkeyMode === "display" ? (
    <div className="flex items-center gap-3">
      <div className="inline-block rounded-lg border border-gray-300 bg-gray-50 px-4 py-2 font-mono text-sm text-gray-700 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-300">
        {settings.hotkey}
      </div>
      <button
        type="button"
        onClick={() => {
          setHotkeyMode("recording");
          setPendingHotkey("");
          setHotkeyError("");
          setTimeout(() => recorderRef.current?.focus(), 50);
        }}
        className="rounded-lg bg-blue-500 px-3 py-2 text-sm font-medium text-white hover:bg-blue-600"
      >
        {t("hotkeyChange")}
      </button>
    </div>
  ) : (
    <div className="space-y-3">
      <div
        ref={recorderRef}
        tabIndex={0}
        onKeyDown={(e) => {
          const shortcut = formatKeyEvent(e);
          if (shortcut) setPendingHotkey(shortcut);
        }}
        className="flex h-12 items-center rounded-lg border-2 border-blue-500 bg-blue-50 px-4 font-mono text-sm text-gray-700 outline-none animate-pulse dark:bg-blue-900/20 dark:text-gray-300"
      >
        {pendingHotkey || t("hotkeyRecording")}
      </div>

      {/* Presets */}
      <div className="flex gap-2">
        <button
          type="button"
          onClick={() => selectPreset("RAlt")}
          className="rounded-md border border-gray-300 px-3 py-1.5 text-xs text-gray-600 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-400 dark:hover:bg-gray-700"
        >
          {t("hotkeyPresetAlt")}
        </button>
        <button
          type="button"
          onClick={() => selectPreset("CommandOrControl+Shift+V")}
          className="rounded-md border border-gray-300 px-3 py-1.5 text-xs text-gray-600 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-400 dark:hover:bg-gray-700"
        >
          {t("hotkeyPresetCombo")}
        </button>
      </div>

      {/* Actions */}
      <div className="flex gap-2">
        <button
          type="button"
          onClick={() => pendingHotkey && saveHotkey(pendingHotkey)}
          disabled={!pendingHotkey}
          className="rounded-lg bg-blue-500 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-600 disabled:opacity-50"
        >
          {t("hotkeySave")}
        </button>
        <button
          type="button"
          onClick={() => {
            setHotkeyMode("display");
            setHotkeyError("");
          }}
          className="rounded-lg border border-gray-300 px-3 py-1.5 text-sm text-gray-600 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-400 dark:hover:bg-gray-700"
        >
          {t("hotkeyCancel")}
        </button>
      </div>

      {hotkeyError && (
        <p className="text-xs text-red-500">{t("hotkeyError", { error: hotkeyError })}</p>
      )}
    </div>
  )}

  <p className="text-xs text-gray-400 dark:text-gray-500">
    {t("hotkeyHint")}
  </p>
</div>
```

**Step 3: Verify frontend builds**

Run: `cd /home/scipio/projects/voxpen-desktop && pnpm build`

**Step 4: Commit**

```
feat: add hotkey recorder widget to settings UI
```

---

### Task 7: Bump version to v0.3.0

**Files:**
- Modify: `src-tauri/Cargo.toml` — version = "0.3.0"
- Modify: `src-tauri/tauri.conf.json` — version: "0.3.0"
- Modify: `package.json` — version: "0.3.0"
- Modify: `src/locales/en.json` — version string
- Modify: `src/locales/zh-TW.json` — version string

**Step 1: Update all version references to 0.3.0**

**Step 2: Commit**

```
chore: bump version to 0.3.0
```

---

### Task 8: Final verification and release

**Step 1: Run core tests**

Run: `cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml`

**Step 2: Run clippy**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`

**Step 3: Build frontend**

Run: `pnpm build`

**Step 4: Push and tag**

```bash
git push origin main
git tag v0.3.0
git push origin v0.3.0
```

This triggers the release workflow to build Windows x64 installers and create a GitHub release draft.
