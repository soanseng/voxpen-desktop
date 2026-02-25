use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread;

use tauri::AppHandle;
use tauri::Emitter;
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use voxink_core::audio::recorder::AudioRecorder;
use voxink_core::input::paste::paste_text;
use voxink_core::pipeline::state::PipelineState;

use crate::state::AppState;

/// Lookup table mapping index (1-based) to rdev::Key.
/// Index 0 means "disabled / no key active".
const RDEV_KEY_TABLE: &[rdev::Key] = &[
    rdev::Key::AltGr,       // 1  — RAlt
    rdev::Key::Alt,          // 2  — LAlt
    rdev::Key::ControlRight, // 3  — RControl
    rdev::Key::ControlLeft,  // 4  — LControl
    rdev::Key::ShiftRight,   // 5  — RShift
    rdev::Key::ShiftLeft,    // 6  — LShift
    rdev::Key::F1,           // 7
    rdev::Key::F2,           // 8
    rdev::Key::F3,           // 9
    rdev::Key::F4,           // 10
    rdev::Key::F5,           // 11
    rdev::Key::F6,           // 12
    rdev::Key::F7,           // 13
    rdev::Key::F8,           // 14
    rdev::Key::F9,           // 15
    rdev::Key::F10,          // 16
    rdev::Key::F11,          // 17
    rdev::Key::F12,          // 18
];

/// Shared state for the persistent rdev listener thread.
struct RdevState {
    /// 1-based index into RDEV_KEY_TABLE. 0 = disabled (no key active).
    target_key_index: AtomicU8,
    /// Whether the listener thread has already been spawned.
    thread_spawned: AtomicBool,
    /// Debounce flag shared with the hotkey event handler.
    /// Persists across re-registrations so the single thread can reuse it.
    processing: Arc<AtomicBool>,
}

/// Look up a key name and return its 1-based index into RDEV_KEY_TABLE.
fn rdev_key_index(name: &str) -> Option<u8> {
    let key = parse_rdev_key(name)?;
    RDEV_KEY_TABLE
        .iter()
        .position(|k| *k == key)
        .map(|i| (i + 1) as u8)
}

/// Tracks the active hotkey registration so it can be swapped at runtime.
pub struct HotkeyManager {
    /// The currently registered shortcut string.
    current_shortcut: Option<String>,
    /// Shared state for the persistent rdev listener thread.
    rdev_state: Arc<RdevState>,
}

impl HotkeyManager {
    pub fn new() -> Self {
        Self {
            current_shortcut: None,
            rdev_state: Arc::new(RdevState {
                target_key_index: AtomicU8::new(0),
                thread_spawned: AtomicBool::new(false),
                processing: Arc::new(AtomicBool::new(false)),
            }),
        }
    }

    /// Register a hotkey. Unregisters any previous hotkey first.
    pub fn register(&mut self, app: &AppHandle, shortcut: &str) -> Result<(), String> {
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
            // Disable the rdev key — the thread stays alive but ignores all events
            self.rdev_state.target_key_index.store(0, Ordering::SeqCst);
        }
        self.current_shortcut = None;
    }

    fn register_combo(&self, app: &AppHandle, shortcut: &str) -> Result<(), String> {
        let app_handle = app.clone();
        let processing = Arc::new(AtomicBool::new(false));

        app.global_shortcut()
            .on_shortcut(shortcut, move |_app, _shortcut, event| {
                let app = app_handle.clone();
                let state: tauri::State<'_, AppState> = app.state();

                // On Windows, RegisterHotKey only fires WM_HOTKEY on key press —
                // ShortcutState::Released never arrives.  We therefore treat every
                // Pressed event as a toggle: if recording_started is true, this press
                // means "stop"; otherwise it means "start".
                // Non-Windows platforms still get Released events, so for them we
                // pass through the original event.state as-is.
                let effective_state = if cfg!(target_os = "windows") {
                    if event.state == ShortcutState::Pressed {
                        if state.recording_started.load(Ordering::SeqCst) {
                            ShortcutState::Released
                        } else {
                            ShortcutState::Pressed
                        }
                    } else {
                        ShortcutState::Released
                    }
                } else {
                    event.state
                };

                handle_hotkey_event(&app, &state, effective_state, &processing);
            })
            .map_err(|e| format!("Failed to register shortcut '{}': {}", shortcut, e))?;

        Ok(())
    }

    fn register_single_key(&self, app: &AppHandle, key_name: &str) -> Result<(), String> {
        let index =
            rdev_key_index(key_name).ok_or_else(|| format!("Unknown key: {}", key_name))?;

        // Atomically swap the target key — the existing thread picks it up immediately
        self.rdev_state
            .target_key_index
            .store(index, Ordering::SeqCst);

        // Reset processing flag so a fresh registration starts clean
        self.rdev_state.processing.store(false, Ordering::SeqCst);

        // Only spawn the listener thread once for the lifetime of the app
        if self
            .rdev_state
            .thread_spawned
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let app_handle = app.clone();
            let rdev_state = self.rdev_state.clone();

            thread::spawn(move || {
                let key_down = AtomicBool::new(false);

                let _ = rdev::listen(move |event| {
                    let idx = rdev_state.target_key_index.load(Ordering::SeqCst);
                    if idx == 0 {
                        // Disabled — reset key_down so a future activation starts clean
                        key_down.store(false, Ordering::SeqCst);
                        return;
                    }

                    let target = RDEV_KEY_TABLE[(idx - 1) as usize];

                    match event.event_type {
                        rdev::EventType::KeyPress(k) if k == target => {
                            if !key_down.swap(true, Ordering::SeqCst) {
                                let app = app_handle.clone();
                                let app_state: tauri::State<'_, AppState> = app.state();
                                handle_hotkey_event(
                                    &app,
                                    &app_state,
                                    ShortcutState::Pressed,
                                    &rdev_state.processing,
                                );
                            }
                        }
                        rdev::EventType::KeyRelease(k) if k == target => {
                            if key_down.swap(false, Ordering::SeqCst) {
                                let app = app_handle.clone();
                                let app_state: tauri::State<'_, AppState> = app.state();
                                handle_hotkey_event(
                                    &app,
                                    &app_state,
                                    ShortcutState::Released,
                                    &rdev_state.processing,
                                );
                            }
                        }
                        _ => {}
                    }
                });
            });
        }

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
        "F1" => Some(rdev::Key::F1),
        "F2" => Some(rdev::Key::F2),
        "F3" => Some(rdev::Key::F3),
        "F4" => Some(rdev::Key::F4),
        "F5" => Some(rdev::Key::F5),
        "F6" => Some(rdev::Key::F6),
        "F7" => Some(rdev::Key::F7),
        "F8" => Some(rdev::Key::F8),
        "F9" => Some(rdev::Key::F9),
        "F10" => Some(rdev::Key::F10),
        "F11" => Some(rdev::Key::F11),
        "F12" => Some(rdev::Key::F12),
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
            let recording_started = state.recording_started.clone();
            let app_for_err = app.clone();
            let processing_flag = processing.clone();

            // Reset the signal before starting
            recording_started.store(false, Ordering::SeqCst);

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
                match recorder.start() {
                    Ok(()) => {
                        // Signal that recording has actually started
                        recording_started.store(true, Ordering::SeqCst);
                    }
                    Err(e) => {
                        let msg = format_audio_error(&e);
                        eprintln!("audio start error: {e}");
                        let _ = app_for_err.emit(
                            "pipeline-state",
                            &PipelineState::Error { message: msg },
                        );
                        processing_flag.store(false, Ordering::SeqCst);
                    }
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
            let recording_started = state.recording_started.clone();
            let processing_flag = processing.clone();

            tauri::async_runtime::spawn(async move {
                // Wait for recording to actually start before stopping.
                // Without this, the release can race ahead of the press async task,
                // calling recorder.stop() before start() completes, leaving the
                // recorder in a permanent Recording state.
                let deadline =
                    tokio::time::Instant::now() + std::time::Duration::from_secs(3);
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
                // Clear the signal for the next cycle
                recording_started.store(false, Ordering::SeqCst);

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
                let vocabulary_hint = voxink_core::pipeline::vocabulary::build_stt_hint(
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

                    let s = settings.lock().await;
                    let entry = voxink_core::history::TranscriptionEntry {
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

                // Allow the next hotkey press immediately after paste completes.
                // The idle-reset below is cosmetic (clears overlay after a delay).
                processing_flag.store(false, Ordering::SeqCst);

                // Reset to idle after delay (cosmetic — clears overlay)
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                let ctrl = controller.lock().await;
                // Only reset if still in a terminal state (Result/Refined/Error).
                // A new recording may have started during the 2s delay.
                match ctrl.current_state() {
                    PipelineState::Result { .. }
                    | PipelineState::Refined { .. }
                    | PipelineState::Error { .. } => {
                        ctrl.reset();
                    }
                    _ => {} // New session in progress — don't reset
                }
                drop(ctrl);
            });
        }
    }
}

/// Produce a user-friendly error message for audio failures.
///
/// On Windows, microphone permission denied surfaces as a generic WASAPI error.
/// We detect common patterns and suggest remediation steps.
fn format_audio_error(err: &voxink_core::error::AppError) -> String {
    let msg = err.to_string();
    let lower = msg.to_lowercase();

    if lower.contains("access") || lower.contains("denied") || lower.contains("permission") {
        return format!(
            "Microphone access denied. Please enable microphone access in \
             Settings > Privacy & Security > Microphone. ({})",
            msg
        );
    }

    if lower.contains("no input device") {
        return "No microphone found. Please connect a microphone and try again.".to_string();
    }

    // WASAPI errors on Windows when mic permission is off
    if lower.contains("wasapi") || lower.contains("not activated") || lower.contains("0x80070005")
    {
        return format!(
            "Microphone access may be disabled. Please check \
             Settings > Privacy & Security > Microphone. ({})",
            msg
        );
    }

    msg
}
