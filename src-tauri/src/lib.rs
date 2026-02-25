mod audio;
mod clipboard;
mod commands;
mod dictionary;
mod history;
mod hotkey;
mod keyboard;
mod state;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    path::BaseDirectory,
    tray::TrayIconBuilder,
    Emitter, Manager,
};
use tauri_plugin_store::StoreExt;
use tokio::sync::Mutex;

use voxink_core::pipeline::controller::{PipelineConfig, PipelineController};
use voxink_core::pipeline::settings::Settings;
use voxink_core::pipeline::state::{Language, PipelineState};

use state::{AppState, GroqLlmProvider, GroqSttProvider};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            // Initialize shared settings
            let settings = Arc::new(Mutex::new(Settings::default()));

            // Initialize pipeline controller with concrete Groq providers
            let config = PipelineConfig::new(None, Language::Auto);
            let stt = GroqSttProvider::new(settings.clone(), app.handle().clone());
            let llm = GroqLlmProvider::new(settings.clone(), app.handle().clone());
            let controller = PipelineController::new(config, stt, llm);

            // Initialize hardware implementations
            let recorder = audio::CpalRecorder::new();
            let clipboard_mgr =
                clipboard::ArboardClipboard::new().expect("failed to init clipboard");
            let keyboard_mgr =
                keyboard::EnigoKeyboard::new().expect("failed to init keyboard simulator");

            // Initialize SQLite history database
            let app_data_dir = app
                .path()
                .resolve("", BaseDirectory::AppData)
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("failed to create app data dir");
            let db_path = app_data_dir.join("voxink.db");
            let history_db =
                history::HistoryDb::open(db_path.clone()).expect("failed to open history DB");
            let dictionary_db =
                dictionary::DictionaryDb::open(db_path).expect("failed to open dictionary DB");

            // Create shared app state
            let app_state = AppState {
                controller: Arc::new(Mutex::new(controller)),
                settings,
                recorder: Arc::new(recorder),
                clipboard: Arc::new(clipboard_mgr),
                keyboard: Arc::new(keyboard_mgr),
                history: Arc::new(history_db),
                dictionary: Arc::new(dictionary_db),
                hotkey_manager: Arc::new(Mutex::new(hotkey::HotkeyManager::new())),
                recording_started: Arc::new(AtomicBool::new(false)),
            };
            app.manage(app_state);

            // Spawn background task to emit pipeline state changes to React + update tray
            {
                let app_handle = app.handle().clone();
                let state: tauri::State<'_, AppState> = app.state();
                let controller = state.controller.clone();

                // Build system tray menu
                let settings_item =
                    MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
                let quit_item =
                    MenuItem::with_id(app, "quit", "Quit VoxInk", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&settings_item, &quit_item])?;

                let icon = Image::from_bytes(include_bytes!("../icons/icon.png"))
                    .expect("failed to load tray icon");
                let tray = TrayIconBuilder::new()
                    .icon(icon)
                    .icon_as_template(cfg!(target_os = "macos"))
                    .menu(&menu)
                    .show_menu_on_left_click(true)
                    .tooltip("VoxInk — Ready")
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "settings" => {
                            if let Some(window) = app.get_webview_window("settings") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    })
                    .build(app)?;

                tauri::async_runtime::spawn(async move {
                    let ctrl = controller.lock().await;
                    let mut rx = ctrl.subscribe();
                    drop(ctrl);
                    while rx.changed().await.is_ok() {
                        let pipeline_state = rx.borrow().clone();
                        let _ = app_handle.emit("pipeline-state", &pipeline_state);

                        // Update tray tooltip based on pipeline state
                        let tooltip = match &pipeline_state {
                            PipelineState::Idle => "VoxInk — Ready",
                            PipelineState::Recording => "VoxInk — Recording...",
                            PipelineState::Processing
                            | PipelineState::Refining { .. } => "VoxInk — Processing...",
                            PipelineState::Result { .. }
                            | PipelineState::Refined { .. } => "VoxInk — Done",
                            PipelineState::Error { .. } => "VoxInk — Error",
                        };
                        let _ = tray.set_tooltip(Some(tooltip));

                        // Show/hide overlay window
                        if let Some(overlay) = app_handle.get_webview_window("overlay") {
                            match &pipeline_state {
                                PipelineState::Idle => {
                                    let _ = overlay.hide();
                                }
                                _ => {
                                    let _ = overlay.show();
                                }
                            }
                        }
                    }
                });
            }

            // Register global hotkey from saved settings
            {
                let saved_hotkey = app
                    .store("settings.json")
                    .ok()
                    .and_then(|s| s.get("settings"))
                    .and_then(|v: serde_json::Value| {
                        v.get("hotkey")
                            .and_then(|h| h.as_str().map(String::from))
                    })
                    .unwrap_or_else(|| "CommandOrControl+Shift+V".to_string());

                let state: tauri::State<'_, AppState> = app.state();
                let mut mgr = state.hotkey_manager.blocking_lock();
                if let Err(e) = mgr.register(app.handle(), &saved_hotkey) {
                    eprintln!("failed to register hotkey: {e}");
                }
            }

            // Intercept close event on settings window: hide instead of destroy
            // so it can be re-shown from the tray menu.
            if let Some(window) = app.get_webview_window("settings") {
                let win = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                    }
                });
                // Show settings window on launch so users can configure the app
                let _ = window.show();
                let _ = window.set_focus();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::set_hotkey,
            commands::get_settings,
            commands::save_settings,
            commands::save_api_key,
            commands::get_api_key_status,
            commands::check_microphone,
            commands::test_api_key,
            commands::get_history,
            commands::search_history,
            commands::delete_history_entry,
            commands::get_dictionary_entries,
            commands::get_dictionary_count,
            commands::add_dictionary_entry,
            commands::delete_dictionary_entry,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
