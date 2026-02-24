use tauri::AppHandle;
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use voxink_core::audio::recorder::AudioRecorder;
use voxink_core::input::paste::paste_text;

use crate::state::AppState;

/// Register the global hotkey and wire it to the pipeline controller.
///
/// Press: start recording (validate API key → begin audio capture)
/// Release: stop recording → transcribe → (optional refine) → auto-paste
pub fn register_hotkey(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let app_handle = app.clone();

    app.global_shortcut().on_shortcut(
        "CommandOrControl+Shift+V",
        move |_app, _shortcut, event| {
            let app = app_handle.clone();
            let state: tauri::State<'_, AppState> = app.state();

            match event.state {
                ShortcutState::Pressed => {
                    let controller = state.controller.clone();
                    let recorder = state.recorder.clone();
                    tauri::async_runtime::spawn(async move {
                        let ctrl = controller.lock().await;
                        if let Err(e) = ctrl.on_start_recording() {
                            eprintln!("start recording error: {e}");
                            return;
                        }
                        drop(ctrl);
                        if let Err(e) = recorder.start() {
                            eprintln!("audio start error: {e}");
                        }
                    });
                }
                ShortcutState::Released => {
                    let controller = state.controller.clone();
                    let recorder = state.recorder.clone();
                    let clipboard = state.clipboard.clone();
                    let keyboard = state.keyboard.clone();
                    let settings = state.settings.clone();

                    tauri::async_runtime::spawn(async move {
                        // Stop recording and get PCM data
                        let pcm_data = match recorder.stop() {
                            Ok(data) => data,
                            Err(e) => {
                                eprintln!("audio stop error: {e}");
                                return;
                            }
                        };

                        // Skip very short recordings (<0.5s at 16kHz)
                        if pcm_data.len() < 8000 {
                            let ctrl = controller.lock().await;
                            ctrl.reset();
                            return;
                        }

                        // Run pipeline: STT + optional LLM refinement
                        let ctrl = controller.lock().await;
                        let result = ctrl.on_stop_recording(pcm_data).await;
                        drop(ctrl);

                        // Auto-paste result if enabled
                        if let Ok(text) = &result {
                            let auto_paste = settings.lock().await.auto_paste;
                            if auto_paste {
                                let text = text.clone();
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
                    });
                }
            }
        },
    )?;

    Ok(())
}
