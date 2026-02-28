# Recording Time Limit Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Auto-stop recording after a configurable max duration (default 6 minutes) to prevent unbounded memory growth and protect against accidental long recordings.

**Architecture:** Add `max_recording_secs: u32` to `Settings`. In `AppState`, store a `JoinHandle` for the auto-stop timeout task. On recording start, spawn a timeout task; on manual stop, abort it. Extract the shared stop logic into a standalone `do_stop_recording()` async fn called by both paths.

**Tech Stack:** `tokio::time::sleep`, `tauri::async_runtime::JoinHandle`, existing pipeline/paste code in `hotkey.rs`.

---

## Files Involved

| Action | File |
|--------|------|
| Modify | `src-tauri/crates/voxpen-core/src/pipeline/settings.rs` |
| Modify | `src-tauri/src/state.rs` |
| Modify | `src-tauri/src/hotkey.rs` |
| Modify | `src-tauri/src/lib.rs` (AppState init) |
| Test | `src-tauri/crates/voxpen-core/src/pipeline/settings.rs` (existing test module) |

---

## Task 1: Add `max_recording_secs` to Settings

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/settings.rs`

### Step 1: Write failing test

Add to the existing `#[cfg(test)] mod tests` block at the bottom of `settings.rs`:

```rust
#[test]
fn should_default_max_recording_secs_to_360() {
    let settings = Settings::default();
    assert_eq!(settings.max_recording_secs, 360);
}

#[test]
fn should_roundtrip_max_recording_secs() {
    let mut settings = Settings::default();
    settings.max_recording_secs = 120;
    let json = serde_json::to_string(&settings).unwrap();
    let deserialized: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.max_recording_secs, 120);
}

#[test]
fn should_deserialize_old_settings_without_max_recording_secs() {
    let json = r#"{"hotkey_ptt":"RAlt","hotkey_toggle":"CommandOrControl+Shift+V","recording_mode":"HoldToRecord","auto_paste":true,"launch_at_login":false,"stt_provider":"groq","stt_language":"Auto","stt_model":"whisper-large-v3-turbo","refinement_enabled":false,"refinement_provider":"groq","refinement_model":"openai/gpt-oss-120b","theme":"system","ui_language":"en"}"#;
    let settings: Settings = serde_json::from_str(json).unwrap();
    assert_eq!(settings.max_recording_secs, 360); // gets default
}
```

### Step 2: Run tests to confirm they fail

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core settings -- --nocapture
```

Expected: FAIL — `no field max_recording_secs`

### Step 3: Add the field to `Settings` struct

In `settings.rs`, add after `microphone_device`:

```rust
/// Maximum recording duration in seconds. Recording auto-stops when exceeded.
/// Default: 360 (6 minutes). Set to 0 to disable the limit.
#[serde(default = "default_max_recording_secs")]
pub max_recording_secs: u32,
```

Add the default function (alongside `default_hotkey_toggle`):

```rust
fn default_max_recording_secs() -> u32 {
    360
}
```

Add to `Default` impl:

```rust
max_recording_secs: default_max_recording_secs(),
```

### Step 4: Run tests to confirm they pass

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core settings -- --nocapture
```

Expected: PASS all settings tests.

### Step 5: Commit

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/settings.rs
git commit -m "feat: add max_recording_secs setting (default 6 min)"
```

---

## Task 2: Add `recording_timeout_handle` to AppState

**Files:**
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/lib.rs`

### Step 1: Add field to `AppState` struct

In `state.rs`, add to `AppState` after `recording_started`:

```rust
/// Handle for the auto-stop timeout task. Aborted on manual stop.
/// `None` when not recording.
pub recording_timeout_handle: Arc<tokio::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
```

### Step 2: Initialize in `lib.rs`

Find where `AppState` is constructed (in `lib.rs`, the `setup` closure). Add:

```rust
recording_timeout_handle: Arc::new(tokio::sync::Mutex::new(None)),
```

### Step 3: Build to confirm no errors

```bash
cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | head -40
```

Expected: Compiles without error (may have unused field warning — that's fine for now).

### Step 4: Commit

```bash
git add src-tauri/src/state.rs src-tauri/src/lib.rs
git commit -m "feat: add recording_timeout_handle to AppState"
```

---

## Task 3: Extract stop logic into `do_stop_recording()`

**Files:**
- Modify: `src-tauri/src/hotkey.rs`

**Context:** The current `HotkeyAction::Stop` arm in `handle_hotkey_event` contains all the stop logic inline. Extract it into a standalone async function so both the timeout task and the manual stop can call it.

### Step 1: Create the helper function

Add this function to `hotkey.rs` **above** `handle_hotkey_event`:

```rust
/// Shared stop logic called by both manual key release and the auto-timeout task.
///
/// `pcm_data` must already be captured from the recorder before calling this.
async fn do_stop_recording(
    app: AppHandle,
    controller: Arc<tokio::sync::Mutex<voxpen_core::pipeline::controller::PipelineController<crate::state::GroqSttProvider, crate::state::GroqLlmProvider>>>,
    clipboard: Arc<crate::clipboard::ArboardClipboard>,
    keyboard: Arc<crate::keyboard::EnigoKeyboard>,
    settings: Arc<tokio::sync::Mutex<voxpen_core::pipeline::settings::Settings>>,
    history: Arc<crate::history::HistoryDb>,
    dictionary: Arc<crate::dictionary::DictionaryDb>,
    license_mgr: Arc<voxpen_core::licensing::LicenseManager<voxpen_core::licensing::DirectLemonSqueezy, crate::licensing::TauriLicenseStore, crate::licensing::SqliteUsageDb>>,
    pcm_data: Vec<i16>,
    processing_flag: Arc<std::sync::atomic::AtomicBool>,
) {
    use std::sync::atomic::Ordering;
    use voxpen_core::pipeline::state::PipelineState;
    use voxpen_core::input::paste::paste_text;
    use tauri::Emitter;

    let pcm_len = pcm_data.len();

    // Skip very short recordings (<0.5s at 16kHz)
    if pcm_len < 8000 {
        let ctrl = controller.lock().await;
        ctrl.reset();
        processing_flag.store(false, Ordering::SeqCst);
        return;
    }

    // Fetch vocabulary for prompt injection
    let vocab_words = dictionary.get_words(500).unwrap_or_default();
    let stt_lang = {
        let s = settings.lock().await;
        s.stt_language.clone()
    };
    let vocabulary_hint = voxpen_core::pipeline::vocabulary::build_stt_hint(
        &vocab_words, &stt_lang,
    );

    // Run pipeline: STT + optional LLM refinement
    let ctrl = controller.lock().await;
    let result = ctrl
        .on_stop_recording(pcm_data, vocabulary_hint, vocab_words)
        .await;
    let final_state = ctrl.current_state();
    drop(ctrl);

    // Save to history and auto-paste result
    if let Ok(final_text) = &result {
        let (original, refined) = match &final_state {
            PipelineState::Refined { original, refined } => {
                (original.clone(), Some(refined.clone()))
            }
            _ => (final_text.clone(), None),
        };
        let has_refined = refined.is_some();

        let s = settings.lock().await;
        let entry = voxpen_core::history::TranscriptionEntry {
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
        let auto_paste = s.auto_paste;
        drop(s);

        if let Err(e) = history.insert(&entry) {
            eprintln!("history insert error: {e}");
        }

        // Record per-category usage for licensing and update tray
        let _ = license_mgr
            .record_usage(voxpen_core::licensing::UsageCategory::VoiceInput);
        if has_refined {
            let _ = license_mgr
                .record_usage(voxpen_core::licensing::UsageCategory::Refinement);
        }
        let _ = app.emit("usage-updated", ());

        if auto_paste {
            let text = final_text.clone();
            let cb = clipboard.clone();
            let kb = keyboard.clone();
            match tokio::task::spawn_blocking(move || {
                paste_text(cb.as_ref(), kb.as_ref(), &text)
            })
            .await
            {
                Ok(Err(e)) => eprintln!("paste failed: {e}"),
                Err(e) => eprintln!("paste task panicked: {e}"),
                Ok(Ok(())) => {}
            }
        }
    }

    // Allow next hotkey press immediately after paste completes.
    processing_flag.store(false, Ordering::SeqCst);

    // Reset to idle after delay (cosmetic — clears overlay)
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let ctrl = controller.lock().await;
    match ctrl.current_state() {
        PipelineState::Result { .. }
        | PipelineState::Refined { .. }
        | PipelineState::Error { .. } => {
            ctrl.reset();
        }
        _ => {}
    }
    drop(ctrl);
}
```

### Step 2: Replace Stop arm with call to helper

In `handle_hotkey_event`, replace everything inside `HotkeyAction::Stop =>` with:

```rust
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
    let timeout_handle = state.recording_timeout_handle.clone();

    tauri::async_runtime::spawn(async move {
        use std::sync::atomic::Ordering;

        // Wait for recording to actually start before stopping.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
        while !recording_started.load(Ordering::SeqCst) {
            if tokio::time::Instant::now() >= deadline {
                eprintln!("recording never started, aborting release handler");
                let ctrl = controller.lock().await;
                ctrl.reset();
                drop(ctrl);
                processing_flag.store(false, Ordering::SeqCst);
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        recording_started.store(false, Ordering::SeqCst);

        // Abort the auto-stop timeout — user stopped manually.
        let handle = timeout_handle.lock().await.take();
        if let Some(h) = handle {
            h.abort();
        }

        let pcm_data = match recorder.stop() {
            Ok(data) => data,
            Err(e) => {
                eprintln!("audio stop error: {e}");
                let ctrl = controller.lock().await;
                ctrl.reset();
                drop(ctrl);
                processing_flag.store(false, Ordering::SeqCst);
                return;
            }
        };

        do_stop_recording(
            app_handle,
            controller,
            clipboard,
            keyboard,
            settings,
            history,
            dictionary,
            license_mgr,
            pcm_data,
            processing_flag,
        ).await;
    });
}
```

### Step 3: Build to check compilation

```bash
cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | head -60
```

Expected: Compiles. Fix any type errors before proceeding.

### Step 4: Commit

```bash
git add src-tauri/src/hotkey.rs
git commit -m "refactor: extract do_stop_recording() helper for reuse by timeout"
```

---

## Task 4: Spawn auto-stop timeout in Start handler

**Files:**
- Modify: `src-tauri/src/hotkey.rs`

### Step 1: Read `max_recording_secs` and spawn timeout after recorder starts

In `handle_hotkey_event`, inside the `HotkeyAction::Start` arm, find the block after `recorder.start()` succeeds. Currently it just sets `recording_started = true`. Replace that block with:

```rust
Ok(()) => {
    // Signal that recording has actually started
    recording_started.store(true, Ordering::SeqCst);

    // Read max duration setting
    let max_secs = {
        let s = settings.lock().await;
        s.max_recording_secs
    };

    if max_secs > 0 {
        // Spawn auto-stop timeout task
        let timeout_controller = controller.clone();
        let timeout_recorder = recorder.clone();
        let timeout_clipboard = clipboard.clone();
        let timeout_keyboard = keyboard.clone();
        let timeout_settings = settings.clone();
        let timeout_history = history.clone();
        let timeout_dictionary = dictionary.clone();
        let timeout_license_mgr = license_mgr.clone();
        let timeout_app = app_for_err.clone();
        let timeout_recording_started = recording_started.clone();
        let timeout_processing = processing_flag.clone();

        let handle = tauri::async_runtime::spawn(async move {
            use std::sync::atomic::Ordering;
            use tauri::Emitter;

            tokio::time::sleep(std::time::Duration::from_secs(max_secs as u64)).await;

            // Only fire if still recording (user may have stopped manually just as timer fired)
            if !timeout_recording_started.load(Ordering::SeqCst) {
                return;
            }
            timeout_recording_started.store(false, Ordering::SeqCst);

            let pcm_data = match timeout_recorder.stop() {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("timeout: audio stop error: {e}");
                    let ctrl = timeout_controller.lock().await;
                    ctrl.reset();
                    timeout_processing.store(false, Ordering::SeqCst);
                    return;
                }
            };

            // Notify frontend that this was an auto-stop
            let _ = timeout_app.emit("recording-timed-out", max_secs);

            do_stop_recording(
                timeout_app,
                timeout_controller,
                timeout_clipboard,
                timeout_keyboard,
                timeout_settings,
                timeout_history,
                timeout_dictionary,
                timeout_license_mgr,
                pcm_data,
                timeout_processing,
            ).await;
        });

        *timeout_handle.lock().await = Some(handle);
    }
}
```

Also: clone the extra state at the top of the Start arm (add `clipboard`, `keyboard`, `history`, `dictionary`, `license_mgr`, `timeout_handle` clones alongside existing ones).

### Step 2: Build

```bash
cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | head -60
```

Expected: Compiles. Fix any move/borrow errors.

### Step 3: Smoke test manually

Build and run. Press PTT, speak for a few seconds, release — should work as before. No regressions.

### Step 4: Commit

```bash
git add src-tauri/src/hotkey.rs
git commit -m "feat: auto-stop recording after max_recording_secs (default 5 min)"
```

---

## Task 5: Frontend — overlay timeout indicator

**Files:**
- Modify: `src/components/Overlay.tsx`

### Step 1: Listen for `recording-timed-out` event

In `Overlay.tsx`, find where `pipeline-state` events are listened to. Add a listener for the new event:

```tsx
import { listen } from '@tauri-apps/api/event';

// In useEffect or alongside existing listeners:
const unlistenTimeout = await listen<number>('recording-timed-out', (_event) => {
  // The overlay will automatically transition via the normal pipeline-state events.
  // Optionally show a brief "Time limit reached" message in the overlay.
  setTimedOut(true);
});
```

Add `timedOut` state and display a small banner beneath the main status when true:

```tsx
const [timedOut, setTimedOut] = useState(false);

// Reset when a new recording starts:
// in the Recording state handler, setTimedOut(false)

// In JSX, below the main status indicator:
{timedOut && (
  <p className="text-xs text-yellow-400 mt-1">錄音時間上限已達</p>
)}
```

### Step 2: Build frontend

```bash
pnpm build
```

Expected: Compiles without TypeScript errors.

### Step 3: Commit

```bash
git add src/components/Overlay.tsx
git commit -m "feat: show timeout indicator in overlay when recording auto-stops"
```

---

## Task 6: Settings UI — expose max recording duration

**Files:**
- Identify existing settings section file (likely `src/components/Settings/`)

### Step 1: Find the right settings section

```bash
ls src/components/Settings/
```

Look for a general or advanced settings section. Add a field for max recording duration.

### Step 2: Add UI control

In the appropriate Settings section component, add a number input for `max_recording_secs`:

```tsx
<label className="text-sm font-medium">
  最長錄音時間 / Max Recording Duration
</label>
<div className="flex items-center gap-2">
  <input
    type="number"
    min={0}
    max={3600}
    step={30}
    value={settings.max_recording_secs}
    onChange={(e) => updateSetting('max_recording_secs', parseInt(e.target.value) || 0)}
    className="w-24 rounded border border-zinc-700 bg-zinc-800 px-2 py-1 text-sm"
  />
  <span className="text-sm text-zinc-400">秒 (0 = 不限制)</span>
</div>
```

### Step 3: Update TypeScript types

In `src/types/settings.ts` (or wherever `Settings` is typed), add:

```ts
max_recording_secs: number;
```

### Step 4: Build and verify

```bash
pnpm build
```

### Step 5: Commit

```bash
git add src/components/Settings/ src/types/settings.ts
git commit -m "feat: settings UI for max recording duration"
```

---

## Task 7: Final verification

### Step 1: Run all Rust tests

```bash
cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | tail -20
```

Expected: All tests pass.

### Step 2: Build release-like

```bash
cargo build --manifest-path src-tauri/Cargo.toml --release 2>&1 | tail -10
```

Expected: Compiles clean.

### Step 3: Manual end-to-end test

1. Set max_recording_secs to 10 via settings UI (or directly in settings JSON)
2. Press PTT hotkey and hold (toggle: press once)
3. Wait 10 seconds without releasing
4. Verify: recording auto-stops, transcription processes normally, "時間上限已達" shows in overlay
5. Press PTT again normally (< 10 seconds): verify normal flow unaffected

### Step 4: Final commit (if any cleanup needed)

```bash
cargo test --manifest-path src-tauri/Cargo.toml
git add -p
git commit -m "chore: recording time limit — final cleanup"
```

---

## Summary

| Task | Scope | Effort |
|------|-------|--------|
| 1. Add `max_recording_secs` to Settings | Rust / Core | Small |
| 2. Add `recording_timeout_handle` to AppState | Rust | Small |
| 3. Extract `do_stop_recording()` | Rust refactor | Medium |
| 4. Spawn timeout in Start handler | Rust | Medium |
| 5. Overlay timeout indicator | React | Small |
| 6. Settings UI control | React | Small |
| 7. Final verification | Testing | Small |
