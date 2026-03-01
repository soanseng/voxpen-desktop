# Voice Edit (Select Text → Speak to Edit) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** User selects text in any app, presses a dedicated "Voice Edit" hotkey, speaks an edit command, and VoxPen replaces the selected text with the LLM-edited result.

**Architecture:** Introduce a new `hotkey_edit` setting (combo shortcuts only, e.g. `Ctrl+Shift+E`). On press: simulate Ctrl+C to capture the selection, then start recording. On release: STT the edit command, call LLM with `for_voice_edit(selected_text, edit_command)` prompt, paste the result to replace the selection. The voice edit pipeline bypasses the normal dictation pipeline (no voice commands, no translation). State transitions go through the existing `PipelineController` watch channel so the overlay/tray work unchanged.

**Tech Stack:** Rust (`tokio`, `serde`, `enigo`, `arboard`, `thiserror`), TypeScript + React, TDD with `mockall` + `#[tokio::test]`

---

## Context: Key Files

| File | Role |
|------|------|
| `src-tauri/crates/voxpen-core/src/pipeline/prompts.rs` | LLM system prompts |
| `src-tauri/crates/voxpen-core/src/input/paste.rs` | `KeySimulator` trait + `paste_text` |
| `src-tauri/crates/voxpen-core/src/pipeline/controller.rs` | `PipelineController` state machine |
| `src-tauri/crates/voxpen-core/src/pipeline/settings.rs` | `Settings` struct |
| `src-tauri/src/keyboard.rs` | `EnigoKeyboard` implements `KeySimulator` |
| `src-tauri/src/state.rs` | `AppState` + `GroqLlmProvider` |
| `src-tauri/src/hotkey.rs` | Hotkey registration + `do_stop_recording` |
| `src-tauri/src/lib.rs` | App setup, `AppState` construction |
| `src-tauri/src/commands.rs` | `set_hotkey`, `save_settings` Tauri commands |
| `src/types/settings.ts` | TypeScript `Settings` interface |
| `src/components/Settings/GeneralSection.tsx` | Hotkey pickers in Settings UI |
| `src/locales/en.json` + `zh-TW.json` | i18n strings |

---

## Task 1: Voice Edit Prompt Functions

**Goal:** Add `VOICE_EDIT_SYSTEM_PROMPT` and `voice_edit_user_message()` to `prompts.rs`.

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/prompts.rs`

**Step 1: Write the failing tests**

Add to the `#[cfg(test)]` block at the bottom of `prompts.rs`:

```rust
#[cfg(test)]
mod voice_edit_tests {
    use super::*;

    #[test]
    fn should_have_non_empty_voice_edit_system_prompt() {
        assert!(!VOICE_EDIT_SYSTEM_PROMPT.is_empty());
    }

    #[test]
    fn should_include_selected_text_in_user_message() {
        let msg = voice_edit_user_message("hello world", "make it formal");
        assert!(msg.contains("hello world"));
    }

    #[test]
    fn should_include_edit_command_in_user_message() {
        let msg = voice_edit_user_message("hello world", "make it formal");
        assert!(msg.contains("make it formal"));
    }

    #[test]
    fn should_differ_for_different_selected_texts() {
        let a = voice_edit_user_message("text one", "command");
        let b = voice_edit_user_message("text two", "command");
        assert_ne!(a, b);
    }

    #[test]
    fn should_differ_for_different_commands() {
        let a = voice_edit_user_message("same text", "make formal");
        let b = voice_edit_user_message("same text", "translate to French");
        assert_ne!(a, b);
    }
}
```

**Step 2: Run tests to confirm they fail**

```bash
cargo test -p voxpen-core voice_edit_tests -- --nocapture 2>&1 | head -30
```
Expected: FAIL — "unresolved import" or "cannot find function"

**Step 3: Add the implementation**

Add after the `translation_tests` block at the top of `prompts.rs` (before the professional prompts section):

```rust
// ---------------------------------------------------------------------------
// Voice Edit prompt
// ---------------------------------------------------------------------------

/// System prompt for voice-edit mode.
///
/// Instructs the LLM to act as a text editor: apply the user's voice command
/// to the selected text and output ONLY the edited version.
pub const VOICE_EDIT_SYSTEM_PROMPT: &str = "\
You are a text editor. The user will provide selected text and a voice command.
Edit the selected text according to the voice command.
Rules:
1. Output ONLY the edited version of the selected text — no explanation, no preamble
2. Apply only what the command asks — do not add or remove content beyond the instruction
3. Preserve the original language and style unless the command explicitly asks to change them
4. If the command asks to translate, translate faithfully";

/// Format the user message for voice-edit mode.
///
/// Combines the captured selected text with the transcribed voice command
/// into the user-turn message sent to the LLM.
pub fn voice_edit_user_message(selected_text: &str, command: &str) -> String {
    format!(
        "Selected text:\n{selected}\n\nVoice command: {command}",
        selected = selected_text,
        command = command,
    )
}
```

**Step 4: Run tests to confirm they pass**

```bash
cargo test -p voxpen-core voice_edit_tests -- --nocapture
```
Expected: all 5 tests PASS

**Step 5: Run full voxpen-core test suite**

```bash
cargo test -p voxpen-core
```
Expected: all tests pass (no regressions)

**Step 6: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/prompts.rs
git commit -m "feat: add voice edit prompt functions to prompts module"
```

---

## Task 2: Add `copy()` to KeySimulator Trait + EnigoKeyboard

**Goal:** The voice edit pipeline needs to simulate Ctrl+C (Cmd+C on macOS) to capture the user's selection. Add a `copy()` method to the `KeySimulator` trait.

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/input/paste.rs`
- Modify: `src-tauri/src/keyboard.rs`

**Step 1: Write the failing test**

Add to the `#[cfg(test)]` block at the bottom of `paste.rs` (inside `mod tests`):

```rust
    #[test]
    fn should_call_copy_on_keyboard() {
        let clipboard = MockClipboardManager::new();
        let mut keys = MockKeySimulator::new();
        keys.expect_copy().times(1).returning(|| Ok(()));
        let result = keys.copy();
        assert!(result.is_ok());
    }
```

**Step 2: Run test to confirm it fails**

```bash
cargo test -p voxpen-core -- paste::tests::should_call_copy_on_keyboard --nocapture
```
Expected: FAIL — no `copy` method on `KeySimulator`

**Step 3: Add `copy()` to the `KeySimulator` trait in `paste.rs`**

Modify the trait (the existing trait block):

```rust
#[cfg_attr(test, mockall::automock)]
pub trait KeySimulator: Send + Sync {
    /// Simulate Cmd+V (macOS) or Ctrl+V (Windows/Linux).
    fn paste(&self) -> Result<(), AppError>;
    /// Simulate Cmd+C (macOS) or Ctrl+C (Windows/Linux) to copy selection.
    fn copy(&self) -> Result<(), AppError>;
}
```

**Step 4: Implement `copy()` on `EnigoKeyboard` in `keyboard.rs`**

Add to the existing `impl KeySimulator for EnigoKeyboard` block, after the existing `paste()` method:

```rust
    fn copy(&self) -> Result<(), AppError> {
        let mut enigo = self.enigo.lock().unwrap_or_else(|e| e.into_inner());

        #[cfg(target_os = "macos")]
        let modifier = Key::Meta;
        #[cfg(not(target_os = "macos"))]
        let modifier = Key::Control;

        enigo
            .key(modifier, Direction::Press)
            .map_err(|e| AppError::Paste(format!("key press failed: {e}")))?;
        enigo
            .key(Key::Unicode('c'), Direction::Click)
            .map_err(|e| AppError::Paste(format!("key click failed: {e}")))?;
        enigo
            .key(modifier, Direction::Release)
            .map_err(|e| AppError::Paste(format!("key release failed: {e}")))?;

        Ok(())
    }
```

**Step 5: Run test to confirm it passes**

```bash
cargo test -p voxpen-core -- paste::tests::should_call_copy_on_keyboard --nocapture
```
Expected: PASS

**Step 6: Build to confirm no compile errors**

```bash
cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "^error"
```
Expected: no errors

**Step 7: Run full test suite**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```
Expected: all tests pass

**Step 8: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/input/paste.rs src-tauri/src/keyboard.rs
git commit -m "feat: add copy() to KeySimulator trait for voice edit selection capture"
```

---

## Task 3: Add `hotkey_edit` Setting Field

**Goal:** Add `hotkey_edit: String` to `Settings` with `#[serde(default)]` so old settings files still deserialize (backwards compat).

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/settings.rs`

**Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `settings.rs`:

```rust
    // -- hotkey_edit tests --

    #[test]
    fn should_default_hotkey_edit_to_empty() {
        let s = Settings::default();
        assert_eq!(s.hotkey_edit, "");
    }

    #[test]
    fn should_roundtrip_hotkey_edit() {
        let mut s = Settings::default();
        s.hotkey_edit = "CommandOrControl+Shift+E".to_string();
        let json = serde_json::to_string(&s).unwrap();
        let s2: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s2.hotkey_edit, "CommandOrControl+Shift+E");
    }

    #[test]
    fn should_deserialize_old_settings_without_hotkey_edit() {
        // Old JSON without hotkey_edit should get empty string default
        let json = r#"{"hotkey_ptt":"RAlt","hotkey_toggle":"CommandOrControl+Shift+V","recording_mode":"HoldToRecord","auto_paste":true,"launch_at_login":false,"stt_provider":"groq","stt_language":"Auto","stt_model":"whisper-large-v3-turbo","refinement_enabled":false,"refinement_provider":"groq","refinement_model":"openai/gpt-oss-120b","theme":"system","ui_language":"en"}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.hotkey_edit, "");
    }
```

**Step 2: Run tests to confirm they fail**

```bash
cargo test -p voxpen-core -- settings::tests::should_default_hotkey_edit --nocapture
```
Expected: FAIL — no `hotkey_edit` field

**Step 3: Add the field to `Settings`**

After the `voice_commands_enabled` field in the `Settings` struct:

```rust
    /// Hotkey for "Voice Edit" mode: user selects text, presses this hotkey,
    /// speaks an edit command, and the selected text is replaced with the
    /// LLM-edited result. Only combo shortcuts supported (e.g. "CommandOrControl+Shift+E").
    /// Default: empty string (feature disabled).
    #[serde(default)]
    pub hotkey_edit: String,
```

And add to `impl Default for Settings`:

```rust
            hotkey_edit: String::new(),
```

**Step 4: Run tests to confirm they pass**

```bash
cargo test -p voxpen-core -- settings::tests::should_default_hotkey_edit settings::tests::should_roundtrip_hotkey_edit settings::tests::should_deserialize_old_settings_without_hotkey_edit --nocapture
```
Expected: all 3 PASS

**Step 5: Build and verify no regressions**

```bash
cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "^error"
cargo test --manifest-path src-tauri/Cargo.toml
```
Expected: clean build, all tests pass

**Step 6: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/settings.rs
git commit -m "feat: add hotkey_edit setting field for voice edit mode"
```

---

## Task 4: `on_stop_recording_stt_only()` + State Emitters in PipelineController

**Goal:** Add a method that performs only STT (no voice commands, no refinement) and two public state-emitting helpers for voice edit completion.

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/controller.rs`

**Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `controller.rs`:

```rust
    // -- voice edit helpers --

    #[tokio::test]
    async fn should_transcribe_only_without_refinement_or_voice_commands() {
        let mut config = config_with_refinement(); // refinement_enabled = true
        config.voice_commands_enabled = true;

        let controller = PipelineController::new(
            config,
            mock_stt_success("hello comma world"),
            mock_llm_unused(), // LLM must NOT be called
        );
        controller.on_start_recording().unwrap();

        let result = controller.on_stop_recording_stt_only(vec![100, 200], None).await;

        // Raw STT text, no voice commands applied, no LLM refinement
        assert_eq!(result.unwrap(), "hello comma world");
        // State should be Processing after STT-only
        assert_eq!(controller.current_state(), PipelineState::Processing);
    }

    #[tokio::test]
    async fn should_emit_refined_state_via_emit_refined() {
        let controller = PipelineController::new(
            config_with_key(),
            mock_stt_success(""),
            mock_llm_unused(),
        );
        controller.emit_refined("original text".to_string(), "edited text".to_string());
        assert_eq!(
            controller.current_state(),
            PipelineState::Refined {
                original: "original text".to_string(),
                refined: "edited text".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn should_emit_error_state_via_emit_error() {
        let controller = PipelineController::new(
            config_with_key(),
            mock_stt_success(""),
            mock_llm_unused(),
        );
        controller.emit_error("something went wrong".to_string());
        assert!(matches!(
            controller.current_state(),
            PipelineState::Error { message } if message == "something went wrong"
        ));
    }

    #[tokio::test]
    async fn should_stt_only_reject_if_not_recording() {
        let controller = PipelineController::new(
            config_with_key(),
            mock_stt_success("text"),
            mock_llm_unused(),
        );
        // Do NOT call on_start_recording
        let result = controller.on_stop_recording_stt_only(vec![100], None).await;
        assert!(matches!(result, Err(AppError::Audio(_))));
    }
```

**Step 2: Run tests to confirm they fail**

```bash
cargo test -p voxpen-core -- tests::should_transcribe_only tests::should_emit_refined tests::should_emit_error --nocapture 2>&1 | head -20
```
Expected: FAIL

**Step 3: Add `on_stop_recording_stt_only()`, `emit_refined()`, `emit_error()` to `PipelineController`**

Add these methods inside `impl<S: SttProvider, L: LlmProvider> PipelineController<S, L>`, after the existing `reset()` method:

```rust
    /// STT-only stop — used by voice-edit pipeline to transcribe the edit command
    /// without applying voice commands or LLM refinement.
    ///
    /// Leaves state as `Processing` so the caller can emit the final state
    /// (e.g. `Refined`) after running the LLM externally.
    pub async fn on_stop_recording_stt_only(
        &self,
        pcm_data: Vec<i16>,
        vocabulary_hint: Option<String>,
    ) -> Result<String, AppError> {
        if !matches!(self.current_state(), PipelineState::Recording) {
            return Err(AppError::Audio("not currently recording".to_string()));
        }
        let _ = self.state_tx.send(PipelineState::Processing);

        match self.stt.transcribe(pcm_data, vocabulary_hint).await {
            Ok(text) => Ok(text),
            Err(e) => {
                let _ = self.state_tx.send(PipelineState::Error {
                    message: e.to_string(),
                });
                Err(e)
            }
        }
    }

    /// Emit a `Refined` state (voice-edit completion).
    pub fn emit_refined(&self, original: String, refined: String) {
        let _ = self.state_tx.send(PipelineState::Refined { original, refined });
    }

    /// Emit an `Error` state.
    pub fn emit_error(&self, message: String) {
        let _ = self.state_tx.send(PipelineState::Error { message });
    }
```

**Step 4: Run tests to confirm they pass**

```bash
cargo test -p voxpen-core -- tests::should_transcribe_only tests::should_emit_refined tests::should_emit_error tests::should_stt_only_reject --nocapture
```
Expected: all 4 PASS

**Step 5: Full test suite**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```
Expected: all pass

**Step 6: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/controller.rs
git commit -m "feat: add on_stop_recording_stt_only and emit helpers to PipelineController"
```

---

## Task 5: AppState + `register_all()` Refactor

**Goal:** Add `voice_edit_selection` to `AppState`, extend `HotkeyManager::register_dual` → `register_all()`, and update all callers.

**Files:**
- Modify: `src-tauri/src/state.rs` (add `voice_edit_selection`)
- Modify: `src-tauri/src/hotkey.rs` (add `registered_edit`, extend `unregister_all`, add `register_all`)
- Modify: `src-tauri/src/lib.rs` (initialize field, update caller)
- Modify: `src-tauri/src/commands.rs` (update `set_hotkey` + `register_dual` → `register_all`)

**Step 1: Add `voice_edit_selection` to `AppState` in `state.rs`**

Add after `recording_timeout_handle` in the `AppState` struct:

```rust
    /// Captured selected text for voice-edit mode.
    /// Set at edit hotkey press time (after Ctrl+C), consumed at release.
    /// `None` when not in voice-edit mode.
    pub voice_edit_selection: Arc<tokio::sync::Mutex<Option<String>>>,
```

**Step 2: Initialize `voice_edit_selection` in `lib.rs`**

In `lib.rs`, inside `let app_state = AppState { ... }`:

```rust
voice_edit_selection: Arc::new(tokio::sync::Mutex::new(None)),
```

**Step 3: Rename `register_dual` → `register_all` in `hotkey.rs`**

In the `HotkeyManager` struct, add `registered_edit: Option<String>`.

In `HotkeyManager::new()`, add `registered_edit: None`.

Rename `register_dual` to `register_all` and add `edit_shortcut: &str` parameter:

```rust
pub fn register_all(
    &mut self,
    app: &AppHandle,
    ptt_shortcut: &str,
    toggle_shortcut: &str,
    edit_shortcut: &str,
) -> Result<(), String> {
    self.unregister_all(app);

    // Register PTT and Toggle (existing logic, unchanged)
    if !ptt_shortcut.is_empty() {
        if is_combo_shortcut(ptt_shortcut) {
            self.register_combo(app, ptt_shortcut, RecordingMode::HoldToRecord)?;
        } else {
            self.register_single_key(ptt_shortcut, RecordingMode::HoldToRecord)?;
        }
        self.registered_ptt = Some(ptt_shortcut.to_string());
    }

    if !toggle_shortcut.is_empty() {
        if is_combo_shortcut(toggle_shortcut) {
            self.register_combo(app, toggle_shortcut, RecordingMode::Toggle)?;
        } else {
            self.register_single_key(toggle_shortcut, RecordingMode::Toggle)?;
        }
        self.registered_toggle = Some(toggle_shortcut.to_string());
    }

    // Register Voice Edit hotkey (combo only for now)
    if !edit_shortcut.is_empty() {
        if is_combo_shortcut(edit_shortcut) {
            self.register_edit_combo(app, edit_shortcut)?;
        }
        // Note: single-key edit hotkeys not yet supported (requires rdev extension)
        self.registered_edit = Some(edit_shortcut.to_string());
    }

    self.ensure_rdev_thread(app);
    Ok(())
}
```

In `unregister_all`, also reset `registered_edit`:

```rust
fn unregister_all(&mut self, app: &AppHandle) {
    if self.registered_ptt.is_some()
        || self.registered_toggle.is_some()
        || self.registered_edit.is_some()
    {
        let _ = app.global_shortcut().unregister_all();
    }
    self.rdev_state.ptt_key_index.store(0, Ordering::SeqCst);
    self.rdev_state.toggle_key_index.store(0, Ordering::SeqCst);
    self.registered_ptt = None;
    self.registered_toggle = None;
    self.registered_edit = None;
}
```

**Step 4: Update callers of `register_dual` → `register_all`**

In `lib.rs`, find `mgr.register_dual(...)` and change to:

```rust
if let Err(e) = mgr.register_all(
    app.handle(),
    &saved_settings.hotkey_ptt,
    &saved_settings.hotkey_toggle,
    &saved_settings.hotkey_edit,
) {
    eprintln!("failed to register hotkeys: {e}");
}
```

In `commands.rs`, find `mgr.register_dual(...)` and change to:

```rust
mgr.register_all(
    &app,
    &settings_clone.hotkey_ptt,
    &settings_clone.hotkey_toggle,
    &settings_clone.hotkey_edit,
)?;
```

Also extend the `set_hotkey` command to handle `kind = "edit"`:

```rust
match kind.as_str() {
    "ptt" => s.hotkey_ptt = shortcut.clone(),
    "toggle" => s.hotkey_toggle = shortcut.clone(),
    "edit" => s.hotkey_edit = shortcut.clone(),
    _ => return Err(format!("Unknown hotkey kind: {kind}")),
}
```

**Step 5: Build to confirm no errors**

```bash
cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "^error"
```
Expected: no errors

**Step 6: Run tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```
Expected: all pass

**Step 7: Commit**

```bash
git add src-tauri/src/state.rs src-tauri/src/hotkey.rs src-tauri/src/lib.rs src-tauri/src/commands.rs
git commit -m "feat: add voice_edit_selection to AppState; refactor register_all hotkey manager"
```

---

## Task 6: `do_voice_edit_stop()` + Edit Hotkey Handlers

**Goal:** Add the core voice edit logic: capture selection on press, run the STT + LLM pipeline on release, paste result.

**Files:**
- Modify: `src-tauri/src/hotkey.rs`

All additions go into `hotkey.rs`.

**Step 1: Add `register_edit_combo()` method to `HotkeyManager`**

Add after the existing `register_combo()` method:

```rust
fn register_edit_combo(
    &self,
    app: &AppHandle,
    shortcut: &str,
) -> Result<(), String> {
    let app_handle = app.clone();
    let processing = self.rdev_state.processing.clone();

    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            let app = app_handle.clone();
            let state: tauri::State<'_, AppState> = app.state();
            handle_edit_hotkey_event(&app, &state, event.state, &processing, true);
        })
        .map_err(|e| format!("Failed to register edit shortcut '{}': {}", shortcut, e))?;

    Ok(())
}
```

**Step 2: Add `handle_edit_hotkey_event()`**

Add this function after the existing `handle_hotkey_event` function:

```rust
/// Press/release handler for the Voice Edit hotkey.
///
/// Press: simulate Ctrl+C to capture selection → start recording.
/// Release: stop recording → run STT + LLM edit pipeline → paste result.
fn handle_edit_hotkey_event(
    app: &AppHandle,
    state: &tauri::State<'_, AppState>,
    shortcut_state: ShortcutState,
    processing: &Arc<AtomicBool>,
    is_combo: bool,
) {
    let is_recording = state.recording_started.load(Ordering::SeqCst);
    let action = resolve_action(shortcut_state, &RecordingMode::HoldToRecord, is_recording, is_combo);

    match action {
        HotkeyAction::Ignore => {}

        HotkeyAction::Start => {
            if processing
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                return;
            }

            let controller = state.controller.clone();
            let recorder = state.recorder.clone();
            let recording_started = state.recording_started.clone();
            let clipboard = state.clipboard.clone();
            let keyboard = state.keyboard.clone();
            let voice_edit_selection = state.voice_edit_selection.clone();
            let processing_flag = processing.clone();
            let app_for_err = app.clone();

            recording_started.store(false, Ordering::SeqCst);

            tauri::async_runtime::spawn(async move {
                use std::sync::atomic::Ordering;
                use voxpen_core::pipeline::state::PipelineState;
                use tauri::Emitter;

                // 1. Simulate Ctrl+C / Cmd+C to copy the selection to clipboard
                let kb = keyboard.clone();
                if let Err(e) = tokio::task::spawn_blocking(move || {
                    use voxpen_core::input::paste::KeySimulator;
                    kb.copy()
                }).await.unwrap_or_else(|e| Err(voxpen_core::error::AppError::Paste(e.to_string()))) {
                    let _ = app_for_err.emit("pipeline-state", &PipelineState::Error {
                        message: format!("Failed to copy selection: {e}"),
                    });
                    processing_flag.store(false, Ordering::SeqCst);
                    return;
                }

                // 2. Wait for OS to update clipboard after Ctrl+C
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;

                // 3. Read the selected text from clipboard
                let cb = clipboard.clone();
                let selected = match tokio::task::spawn_blocking(move || {
                    use voxpen_core::input::clipboard::ClipboardManager;
                    cb.get_text()
                }).await {
                    Ok(Ok(Some(text))) if !text.is_empty() => text,
                    _ => {
                        let _ = app_for_err.emit("pipeline-state", &PipelineState::Error {
                            message: "No text selected. Please select text before using Voice Edit.".to_string(),
                        });
                        processing_flag.store(false, Ordering::SeqCst);
                        return;
                    }
                };

                // 4. Store the selected text for use by the release handler
                *voice_edit_selection.lock().await = Some(selected);

                // 5. Start the recording pipeline
                let ctrl = controller.lock().await;
                if let Err(e) = ctrl.on_start_recording() {
                    let _ = app_for_err.emit("pipeline-state", &PipelineState::Error {
                        message: e.to_string(),
                    });
                    processing_flag.store(false, Ordering::SeqCst);
                    return;
                }
                drop(ctrl);

                match recorder.start() {
                    Ok(()) => {
                        recording_started.store(true, Ordering::SeqCst);
                    }
                    Err(e) => {
                        let msg = format_audio_error(&e);
                        let _ = app_for_err.emit("pipeline-state", &PipelineState::Error { message: msg });
                        processing_flag.store(false, Ordering::SeqCst);
                    }
                }
            });
        }

        HotkeyAction::Stop => {
            let controller = state.controller.clone();
            let recorder = state.recorder.clone();
            let clipboard = state.clipboard.clone();
            let keyboard = state.keyboard.clone();
            let settings = state.settings.clone();
            let history = state.history.clone();
            let dictionary = state.dictionary.clone();
            let license_mgr = state.license_manager.clone();
            let app_handle = app.clone();
            let recording_started = state.recording_started.clone();
            let processing_flag = processing.clone();
            let voice_edit_selection = state.voice_edit_selection.clone();

            tauri::async_runtime::spawn(async move {
                use std::sync::atomic::Ordering;

                // Wait for recording to actually start
                let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
                while !recording_started.load(Ordering::SeqCst) {
                    if tokio::time::Instant::now() >= deadline {
                        eprintln!("voice edit: recording never started, aborting");
                        let ctrl = controller.lock().await;
                        ctrl.reset();
                        drop(ctrl);
                        processing_flag.store(false, Ordering::SeqCst);
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                recording_started.store(false, Ordering::SeqCst);

                let pcm_data = match recorder.stop() {
                    Ok(data) => data,
                    Err(e) => {
                        eprintln!("voice edit: audio stop error: {e}");
                        let ctrl = controller.lock().await;
                        ctrl.reset();
                        drop(ctrl);
                        processing_flag.store(false, Ordering::SeqCst);
                        return;
                    }
                };

                // Retrieve selected text captured at press time
                let selected_text = voice_edit_selection.lock().await.take()
                    .unwrap_or_default();
                if selected_text.is_empty() {
                    let ctrl = controller.lock().await;
                    ctrl.reset();
                    drop(ctrl);
                    processing_flag.store(false, Ordering::SeqCst);
                    return;
                }

                do_voice_edit_stop(
                    app_handle,
                    controller,
                    clipboard,
                    keyboard,
                    settings,
                    history,
                    dictionary,
                    license_mgr,
                    pcm_data,
                    selected_text,
                    processing_flag,
                )
                .await;
            });
        }
    }
}
```

**Step 3: Add `do_voice_edit_stop()`**

Add this async function after the existing `do_stop_recording` function:

```rust
/// Voice edit stop: STT the edit command, run LLM with voice-edit prompt,
/// paste the result to replace the original selection.
#[allow(clippy::too_many_arguments)]
async fn do_voice_edit_stop(
    app: tauri::AppHandle,
    controller: Arc<tokio::sync::Mutex<voxpen_core::pipeline::controller::PipelineController<crate::state::GroqSttProvider, crate::state::GroqLlmProvider>>>,
    clipboard: Arc<crate::clipboard::ArboardClipboard>,
    keyboard: Arc<crate::keyboard::EnigoKeyboard>,
    settings: Arc<tokio::sync::Mutex<voxpen_core::pipeline::settings::Settings>>,
    history: Arc<crate::history::HistoryDb>,
    dictionary: Arc<crate::dictionary::DictionaryDb>,
    license_mgr: Arc<voxpen_core::licensing::LicenseManager<voxpen_core::licensing::DirectLemonSqueezy, crate::licensing::TauriLicenseStore, crate::licensing::SqliteUsageDb>>,
    pcm_data: Vec<i16>,
    selected_text: String,
    processing_flag: Arc<std::sync::atomic::AtomicBool>,
) {
    use std::sync::atomic::Ordering;
    use voxpen_core::api::groq::{self, ChatConfig};
    use voxpen_core::input::paste::paste_text;
    use voxpen_core::pipeline::{prompts, refine};
    use voxpen_core::pipeline::state::{Language, TonePreset};
    use tauri::Emitter;

    let pcm_len = pcm_data.len();

    // Skip very short recordings (<0.5s at 16kHz)
    if pcm_len < 8000 {
        let ctrl = controller.lock().await;
        ctrl.reset();
        processing_flag.store(false, Ordering::SeqCst);
        return;
    }

    // Build vocabulary hint for STT
    let vocab_words = dictionary.get_words(500).unwrap_or_default();
    let stt_lang = {
        let s = settings.lock().await;
        s.stt_language.clone()
    };
    let vocabulary_hint = voxpen_core::pipeline::vocabulary::build_stt_hint(&vocab_words, &stt_lang);

    // STT-only: transcribe the edit command
    let ctrl = controller.lock().await;
    let edit_command = match ctrl.on_stop_recording_stt_only(pcm_data, vocabulary_hint).await {
        Ok(text) => text,
        Err(e) => {
            eprintln!("voice edit: STT failed: {e}");
            ctrl.reset();
            drop(ctrl);
            processing_flag.store(false, Ordering::SeqCst);
            return;
        }
    };
    drop(ctrl);

    // Check VoiceInput usage quota (same as dictation)
    let voice_status = license_mgr.check_category(voxpen_core::licensing::UsageCategory::VoiceInput);
    if voice_status == voxpen_core::licensing::UsageStatus::Exhausted {
        let ctrl = controller.lock().await;
        ctrl.emit_error("Daily voice limit reached. Upgrade to Pro for unlimited access.".to_string());
        drop(ctrl);
        let _ = app.emit("usage-exhausted", ());
        processing_flag.store(false, Ordering::SeqCst);
        return;
    }

    // LLM: apply voice edit
    let timeout_duration = std::time::Duration::from_secs(30);
    let s = settings.lock().await;
    let refinement_provider = s.refinement_provider.clone();
    let refinement_model = s.refinement_model.clone();
    let custom_base_url = s.custom_base_url.clone();
    drop(s);

    let llm_key = match crate::state::get_api_key_from_handle(&app, &refinement_provider) {
        Ok(k) => k,
        Err(e) => {
            let ctrl = controller.lock().await;
            ctrl.emit_error(format!("LLM API key not configured: {e}"));
            drop(ctrl);
            processing_flag.store(false, Ordering::SeqCst);
            return;
        }
    };

    let chat_config = ChatConfig {
        api_key: llm_key,
        model: refinement_model,
        temperature: groq::LLM_TEMPERATURE,
        max_tokens: groq::LLM_MAX_TOKENS,
    };

    // The user message bundles both the selected text and the spoken command
    let user_message = prompts::voice_edit_user_message(&selected_text, &edit_command);

    let refine_result = tokio::time::timeout(
        timeout_duration,
        refine::refine(
            &user_message,
            &chat_config,
            &Language::Auto,
            &[],
            prompts::VOICE_EDIT_SYSTEM_PROMPT,
            &TonePreset::Custom,
            &refinement_provider,
            &custom_base_url,
            None,
        ),
    )
    .await;

    let edited_text = match refine_result {
        Ok(Ok(text)) => text,
        Ok(Err(e)) => {
            eprintln!("voice edit: LLM failed: {e}");
            let ctrl = controller.lock().await;
            ctrl.emit_error(format!("Voice edit failed: {e}"));
            drop(ctrl);
            processing_flag.store(false, Ordering::SeqCst);
            return;
        }
        Err(_) => {
            eprintln!("voice edit: LLM timed out");
            let ctrl = controller.lock().await;
            ctrl.emit_error("Voice edit timed out after 30s".to_string());
            drop(ctrl);
            processing_flag.store(false, Ordering::SeqCst);
            return;
        }
    };

    // Emit Refined state (original = selected text, refined = edited result)
    {
        let ctrl = controller.lock().await;
        ctrl.emit_refined(selected_text.clone(), edited_text.clone());
        drop(ctrl);
    }

    // Save to history
    let s = settings.lock().await;
    let entry = voxpen_core::history::TranscriptionEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64,
        original_text: selected_text.clone(),
        refined_text: Some(edited_text.clone()),
        language: s.stt_language.clone(),
        audio_duration_ms: (pcm_len as u64 * 1000) / 16000,
        provider: s.stt_provider.clone(),
    };
    let auto_paste = s.auto_paste;
    drop(s);

    if let Err(e) = history.insert(&entry) {
        eprintln!("voice edit history insert error: {e}");
    }

    // Record usage
    let _ = license_mgr.record_usage(voxpen_core::licensing::UsageCategory::VoiceInput);
    let _ = license_mgr.record_usage(voxpen_core::licensing::UsageCategory::Refinement);
    let _ = app.emit("usage-updated", ());

    // Paste to replace selection
    if auto_paste {
        let text = edited_text.clone();
        let cb = clipboard.clone();
        let kb = keyboard.clone();
        match tokio::task::spawn_blocking(move || paste_text(cb.as_ref(), kb.as_ref(), &text)).await {
            Ok(Err(e)) => eprintln!("voice edit: paste failed: {e}"),
            Err(e) => eprintln!("voice edit: paste task panicked: {e}"),
            Ok(Ok(())) => {}
        }
    }

    processing_flag.store(false, Ordering::SeqCst);

    // Reset to idle after delay
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let ctrl = controller.lock().await;
    match ctrl.current_state() {
        voxpen_core::pipeline::state::PipelineState::Refined { .. }
        | voxpen_core::pipeline::state::PipelineState::Error { .. } => {
            ctrl.reset();
        }
        _ => {}
    }
    drop(ctrl);
}
```

**Step 4: Build to confirm no errors**

```bash
cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "^error"
```

If there are import errors, add missing `use` statements at the top of `hotkey.rs`:
- `use voxpen_core::api::groq::{self, ChatConfig};` may conflict with existing imports
- The `groq` import may already exist indirectly; add only what's missing

**Step 5: Run test suite**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```
Expected: all tests pass

**Step 6: Commit**

```bash
git add src-tauri/src/hotkey.rs
git commit -m "feat: add voice edit hotkey handlers and do_voice_edit_stop pipeline"
```

---

## Task 7: TypeScript Types + Settings UI + i18n

**Goal:** Add `hotkey_edit` to the TypeScript `Settings` interface and add a `HotkeyPicker` for it in `GeneralSection.tsx`.

**Files:**
- Modify: `src/types/settings.ts`
- Modify: `src/components/Settings/GeneralSection.tsx`
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh-TW.json`

**Step 1: Add `hotkey_edit` to `src/types/settings.ts`**

In the `Settings` interface, add after `voice_commands_enabled`:

```typescript
  hotkey_edit: string;
```

In `defaultSettings`, add after `voice_commands_enabled: false,`:

```typescript
  hotkey_edit: "",
```

**Step 2: Add i18n strings to `src/locales/en.json`**

Add before the closing `}`:

```json
  "hotkeyEdit": "Voice Edit Hotkey",
  "hotkeyEditHint": "Select text in any app, hold this key, speak an edit command, and the selection is replaced with the edited result. Requires LLM Refinement to be enabled. Combo shortcuts only (e.g. Ctrl+Shift+E).",
  "hotkeyEditDisabled": "Disabled (leave empty)"
```

**Step 3: Add i18n strings to `src/locales/zh-TW.json`**

Read the file first to find the correct location, then add after `voiceCommandsEnabledHint`:

```json
  "hotkeyEdit": "語音編輯快捷鍵",
  "hotkeyEditHint": "在任何應用程式中選取文字，按住此快捷鍵，說出編輯指令，選取的文字將被替換為編輯結果。需啟用 LLM 潤色。僅支援組合快捷鍵（例如 Ctrl+Shift+E）。",
  "hotkeyEditDisabled": "停用（留空）"
```

**Step 4: Add `HotkeyPicker` for Voice Edit in `GeneralSection.tsx`**

Add after the existing Voice Commands toggle block (at the bottom of the returned JSX, before the closing `</div>`):

```tsx
      {/* Voice Edit Hotkey */}
      <HotkeyPicker
        label={t("hotkeyEdit")}
        hint={t("hotkeyEditHint")}
        currentValue={settings.hotkey_edit}
        kind="edit"
        onSaved={(s) => onUpdate("hotkey_edit", s)}
        t={t}
      />
```

**Step 5: Build the frontend to confirm no TypeScript errors**

```bash
pnpm build 2>&1 | grep -E "error|Error" | head -20
```
Expected: no type errors

**Step 6: Commit**

```bash
git add src/types/settings.ts src/components/Settings/GeneralSection.tsx src/locales/en.json src/locales/zh-TW.json
git commit -m "feat: add voice edit hotkey to settings UI and TypeScript types"
```

---

## Task 8: ROADMAP Update

**Goal:** Mark "Select Text → Voice Edit" as shipped in `docs/ROADMAP.md`.

**Files:**
- Modify: `docs/ROADMAP.md`

**Step 1: Update the Shipped table**

Add to the Shipped (v0.x) table:

```markdown
| ✅ Select Text → Voice Edit | Select text in any app → hold hotkey → speak edit command → replaced |
```

**Step 2: Update the P2 Select Text → Voice Edit section**

Change the status line:

```markdown
**Status:** ✅ Shipped — Plan: `docs/plans/2026-03-01-voice-edit.md`
```

**Step 3: Update the Typeless comparison table**

Change:

```markdown
| Select text + voice edit | ✅ | ✅ Shipped |
```

**Step 4: Commit**

```bash
git add docs/ROADMAP.md
git commit -m "docs: mark Select Text → Voice Edit as shipped in ROADMAP"
```

---

## Verification Checklist

After all tasks are complete:

1. `cargo test --manifest-path src-tauri/Cargo.toml` — all tests pass
2. `pnpm build` — no TypeScript errors
3. Manual smoke test:
   - Open any text editor, type some text, select it
   - Press the Voice Edit combo (e.g. `Ctrl+Shift+E`)
   - Overlay shows "Recording..."
   - Speak: "make this more formal"
   - Release hotkey
   - Overlay shows "Processing..." then "Done"
   - Selected text is replaced with the edited version
4. Test: press edit hotkey without selecting anything → overlay shows error "No text selected"
5. Test: LLM disabled in settings → error about API key (edit always requires LLM)

## Known Limitations (by Design)

- **Combo shortcuts only** for `hotkey_edit` — single-key support (e.g. F8) requires extending the rdev thread and can be added in a follow-up.
- **LLM required** — unlike dictation which works without refinement, voice edit always needs an LLM call.
- **No Wayland paste** — on Linux Wayland, paste simulation may fail; same limitation as dictation.
