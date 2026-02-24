use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::AppHandle;
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use voxink_core::audio::recorder::AudioRecorder;
use voxink_core::input::paste::paste_text;
use voxink_core::pipeline::state::PipelineState;

use crate::state::AppState;

/// Register the global hotkey and wire it to the pipeline controller.
///
/// Press: start recording (validate API key -> begin audio capture)
/// Release: stop recording -> transcribe -> (optional refine) -> save history -> auto-paste
///
/// Includes debounce guard to prevent concurrent pipeline runs from rapid key presses.
pub fn register_hotkey(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let app_handle = app.clone();
    let processing = Arc::new(AtomicBool::new(false));

    app.global_shortcut().on_shortcut(
        "CommandOrControl+Shift+V",
        move |_app, _shortcut, event| {
            let app = app_handle.clone();
            let state: tauri::State<'_, AppState> = app.state();

            match event.state {
                ShortcutState::Pressed => {
                    // Debounce: skip if already processing a pipeline run
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
                            // Emit error to frontend overlay
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
                    let processing_flag = processing.clone();

                    tauri::async_runtime::spawn(async move {
                        // Stop recording and get PCM data
                        let pcm_data = match recorder.stop() {
                            Ok(data) => data,
                            Err(e) => {
                                eprintln!("audio stop error: {e}");
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

                        // Run pipeline: STT + optional LLM refinement
                        let ctrl = controller.lock().await;
                        let result = ctrl.on_stop_recording(pcm_data).await;
                        let final_state = ctrl.current_state();
                        drop(ctrl);

                        // Save to history and auto-paste result
                        if let Ok(final_text) = &result {
                            // Determine original vs refined text from pipeline state
                            let (original, refined) = match &final_state {
                                PipelineState::Refined { original, refined } => {
                                    (original.clone(), Some(refined.clone()))
                                }
                                _ => (final_text.clone(), None),
                            };

                            // Save to SQLite history
                            let s = settings.lock().await;
                            let entry = voxink_core::history::TranscriptionEntry {
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

                            // Auto-paste if enabled
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

                        // Reset to idle after delay
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        let ctrl = controller.lock().await;
                        ctrl.reset();
                        drop(ctrl);

                        // Release debounce lock
                        processing_flag.store(false, Ordering::SeqCst);
                    });
                }
            }
        },
    )?;

    Ok(())
}
