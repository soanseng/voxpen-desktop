use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread;

use tauri::AppHandle;
use tauri::Emitter;
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use voxpen_core::audio::recorder::AudioRecorder;
use voxpen_core::pipeline::state::{PipelineState, RecordingMode};

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
    /// 1-based index into RDEV_KEY_TABLE for PTT key. 0 = disabled.
    ptt_key_index: AtomicU8,
    /// 1-based index into RDEV_KEY_TABLE for Toggle key. 0 = disabled.
    toggle_key_index: AtomicU8,
    /// Whether the listener thread has already been spawned.
    thread_spawned: AtomicBool,
    /// Debounce flag shared with the hotkey event handler.
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

/// Tracks active hotkey registrations for both PTT and Toggle modes.
pub struct HotkeyManager {
    registered_ptt: Option<String>,
    registered_toggle: Option<String>,
    rdev_state: Arc<RdevState>,
}

impl HotkeyManager {
    pub fn new() -> Self {
        Self {
            registered_ptt: None,
            registered_toggle: None,
            rdev_state: Arc::new(RdevState {
                ptt_key_index: AtomicU8::new(0),
                toggle_key_index: AtomicU8::new(0),
                thread_spawned: AtomicBool::new(false),
                processing: Arc::new(AtomicBool::new(false)),
            }),
        }
    }

    /// Register both PTT and Toggle hotkeys. Either can be empty to skip.
    pub fn register_dual(
        &mut self,
        app: &AppHandle,
        ptt_shortcut: &str,
        toggle_shortcut: &str,
    ) -> Result<(), String> {
        self.unregister_all(app);

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

        // Ensure rdev listener thread is running if any single keys are registered
        self.ensure_rdev_thread(app);

        Ok(())
    }

    fn unregister_all(&mut self, app: &AppHandle) {
        if self.registered_ptt.is_some() || self.registered_toggle.is_some() {
            let _ = app.global_shortcut().unregister_all();
        }
        self.rdev_state.ptt_key_index.store(0, Ordering::SeqCst);
        self.rdev_state
            .toggle_key_index
            .store(0, Ordering::SeqCst);
        self.registered_ptt = None;
        self.registered_toggle = None;
    }

    fn register_combo(
        &self,
        app: &AppHandle,
        shortcut: &str,
        mode: RecordingMode,
    ) -> Result<(), String> {
        let app_handle = app.clone();
        let processing = self.rdev_state.processing.clone();

        app.global_shortcut()
            .on_shortcut(shortcut, move |_app, _shortcut, event| {
                let app = app_handle.clone();
                let state: tauri::State<'_, AppState> = app.state();
                handle_hotkey_event(&app, &state, event.state, &processing, true, &mode);
            })
            .map_err(|e| format!("Failed to register shortcut '{}': {}", shortcut, e))?;

        Ok(())
    }

    fn register_single_key(
        &self,
        key_name: &str,
        mode: RecordingMode,
    ) -> Result<(), String> {
        let index =
            rdev_key_index(key_name).ok_or_else(|| format!("Unknown key: {}", key_name))?;

        match mode {
            RecordingMode::HoldToRecord => {
                self.rdev_state
                    .ptt_key_index
                    .store(index, Ordering::SeqCst);
            }
            RecordingMode::Toggle => {
                self.rdev_state
                    .toggle_key_index
                    .store(index, Ordering::SeqCst);
            }
        }

        // Reset processing flag so a fresh registration starts clean
        self.rdev_state.processing.store(false, Ordering::SeqCst);

        Ok(())
    }

    /// Ensure the persistent rdev listener thread is running.
    fn ensure_rdev_thread(&self, app: &AppHandle) {
        if self
            .rdev_state
            .thread_spawned
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let app_handle = app.clone();
            let rdev_state = self.rdev_state.clone();

            thread::spawn(move || {
                let ptt_down = AtomicBool::new(false);
                let toggle_down = AtomicBool::new(false);

                let _ = rdev::listen(move |event| {
                    let ptt_idx = rdev_state.ptt_key_index.load(Ordering::SeqCst);
                    let toggle_idx = rdev_state.toggle_key_index.load(Ordering::SeqCst);

                    match event.event_type {
                        rdev::EventType::KeyPress(k) => {
                            // Check PTT key
                            if ptt_idx > 0
                                && k == RDEV_KEY_TABLE[(ptt_idx - 1) as usize]
                                && !ptt_down.swap(true, Ordering::SeqCst)
                            {
                                let app = app_handle.clone();
                                let s: tauri::State<'_, AppState> = app.state();
                                handle_hotkey_event(
                                    &app,
                                    &s,
                                    ShortcutState::Pressed,
                                    &rdev_state.processing,
                                    false,
                                    &RecordingMode::HoldToRecord,
                                );
                            }
                            // Check Toggle key
                            if toggle_idx > 0
                                && k == RDEV_KEY_TABLE[(toggle_idx - 1) as usize]
                                && !toggle_down.swap(true, Ordering::SeqCst)
                            {
                                let app = app_handle.clone();
                                let s: tauri::State<'_, AppState> = app.state();
                                handle_hotkey_event(
                                    &app,
                                    &s,
                                    ShortcutState::Pressed,
                                    &rdev_state.processing,
                                    false,
                                    &RecordingMode::Toggle,
                                );
                            }
                        }
                        rdev::EventType::KeyRelease(k) => {
                            // Check PTT key
                            if ptt_idx > 0
                                && k == RDEV_KEY_TABLE[(ptt_idx - 1) as usize]
                                && ptt_down.swap(false, Ordering::SeqCst)
                            {
                                let app = app_handle.clone();
                                let s: tauri::State<'_, AppState> = app.state();
                                handle_hotkey_event(
                                    &app,
                                    &s,
                                    ShortcutState::Released,
                                    &rdev_state.processing,
                                    false,
                                    &RecordingMode::HoldToRecord,
                                );
                            }
                            // Check Toggle key
                            if toggle_idx > 0
                                && k == RDEV_KEY_TABLE[(toggle_idx - 1) as usize]
                                && toggle_down.swap(false, Ordering::SeqCst)
                            {
                                let app = app_handle.clone();
                                let s: tauri::State<'_, AppState> = app.state();
                                handle_hotkey_event(
                                    &app,
                                    &s,
                                    ShortcutState::Released,
                                    &rdev_state.processing,
                                    false,
                                    &RecordingMode::Toggle,
                                );
                            }
                        }
                        _ => {}
                    }

                    // Reset key_down flags when respective key is disabled
                    if ptt_idx == 0 {
                        ptt_down.store(false, Ordering::SeqCst);
                    }
                    if toggle_idx == 0 {
                        toggle_down.store(false, Ordering::SeqCst);
                    }
                });
            });
        }
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

/// Resolved action after considering recording mode and platform quirks.
enum HotkeyAction {
    Start,
    Stop,
    Ignore,
}

/// Determine what action to take given the raw shortcut state, the current
/// recording mode, and whether we are currently recording.
///
/// Platform note: on Windows, `tauri-plugin-global-shortcut` combo keys use
/// `RegisterHotKey` which **only fires Pressed** — Released never arrives.
/// Single keys via rdev get real Press/Release events on all platforms.
/// We therefore treat combo keys on Windows the same as Toggle: every Press
/// toggles between Start and Stop.
fn resolve_action(
    shortcut_state: ShortcutState,
    mode: &RecordingMode,
    is_recording: bool,
    is_combo: bool,
) -> HotkeyAction {
    // On Windows, combo shortcuts only fire Pressed — we must toggle on Press
    // regardless of the user's recording mode setting.
    let force_toggle = cfg!(target_os = "windows") && is_combo;

    match mode {
        RecordingMode::Toggle => {
            // Toggle: only act on Press events — ignore Release entirely.
            if shortcut_state == ShortcutState::Pressed {
                if is_recording {
                    HotkeyAction::Stop
                } else {
                    HotkeyAction::Start
                }
            } else {
                HotkeyAction::Ignore
            }
        }
        RecordingMode::HoldToRecord => {
            if force_toggle {
                // Windows combo: emulate hold-to-record as toggle since
                // Release events never arrive.
                if shortcut_state == ShortcutState::Pressed {
                    if is_recording {
                        HotkeyAction::Stop
                    } else {
                        HotkeyAction::Start
                    }
                } else {
                    HotkeyAction::Ignore
                }
            } else {
                // Normal hold: Press=Start, Release=Stop
                match shortcut_state {
                    ShortcutState::Pressed => HotkeyAction::Start,
                    ShortcutState::Released => HotkeyAction::Stop,
                }
            }
        }
    }
}

/// Shared stop logic called by both manual key release and the auto-timeout task.
///
/// `pcm_data` must already be captured from the recorder before calling this.
#[allow(clippy::too_many_arguments)]
async fn do_stop_recording(
    app: tauri::AppHandle,
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
    use voxpen_core::input::paste::paste_text;
    use voxpen_core::pipeline::state::PipelineState;
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
    let vocabulary_hint =
        voxpen_core::pipeline::vocabulary::build_stt_hint(&vocab_words, &stt_lang);

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
        let _ = license_mgr.record_usage(voxpen_core::licensing::UsageCategory::VoiceInput);
        if has_refined {
            let _ = license_mgr.record_usage(voxpen_core::licensing::UsageCategory::Refinement);
        }
        let _ = app.emit("usage-updated", ());

        if auto_paste {
            let text = final_text.clone();
            let cb = clipboard.clone();
            let kb = keyboard.clone();
            match tokio::task::spawn_blocking(move || paste_text(cb.as_ref(), kb.as_ref(), &text))
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

/// Shared press/release handler used by both combo and single-key backends.
/// The recording `mode` is determined by which hotkey was pressed (PTT vs Toggle).
fn handle_hotkey_event(
    app: &AppHandle,
    state: &tauri::State<'_, AppState>,
    shortcut_state: ShortcutState,
    processing: &Arc<AtomicBool>,
    is_combo: bool,
    mode: &RecordingMode,
) {
    let is_recording = state.recording_started.load(Ordering::SeqCst);
    let action = resolve_action(shortcut_state, mode, is_recording, is_combo);

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
            let settings = state.settings.clone();
            let license_mgr = state.license_manager.clone();
            let app_for_err = app.clone();
            let processing_flag = processing.clone();
            let clipboard = state.clipboard.clone();
            let keyboard = state.keyboard.clone();
            let history = state.history.clone();
            let dictionary = state.dictionary.clone();
            let timeout_handle = state.recording_timeout_handle.clone();

            // Reset the signal before starting
            recording_started.store(false, Ordering::SeqCst);

            tauri::async_runtime::spawn(async move {
                // License usage gate — check VoiceInput category specifically
                let voice_status = license_mgr
                    .check_category(voxpen_core::licensing::UsageCategory::VoiceInput);
                match &voice_status {
                    voxpen_core::licensing::UsageStatus::Exhausted => {
                        let _ = app_for_err.emit("usage-exhausted", ());
                        processing_flag.store(false, Ordering::SeqCst);
                        return;
                    }
                    voxpen_core::licensing::UsageStatus::Warning { remaining } => {
                        let _ = app_for_err.emit("usage-warning", remaining);
                    }
                    _ => {}
                }

                // Set preferred microphone device before starting
                let mic_device = {
                    let s = settings.lock().await;
                    s.microphone_device.clone()
                };
                recorder.set_preferred_device(mic_device);

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

                        // Read max duration setting
                        let max_secs = {
                            let s = settings.lock().await;
                            s.max_recording_secs
                        };

                        if max_secs > 0 {
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

                                tokio::time::sleep(std::time::Duration::from_secs(
                                    max_secs as u64,
                                ))
                                .await;

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
                                )
                                .await;
                            });

                            *timeout_handle.lock().await = Some(handle);
                        }
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
                // Without this, the release can race ahead of the press async task.
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
                )
                .await;
            });
        }
    }
}

/// Produce a user-friendly error message for audio failures.
///
/// On Windows, microphone permission denied surfaces as a generic WASAPI error.
/// We detect common patterns and suggest remediation steps.
fn format_audio_error(err: &voxpen_core::error::AppError) -> String {
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
