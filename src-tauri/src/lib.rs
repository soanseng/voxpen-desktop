// TODO(task-4): remove allow(dead_code) once hotkey integration consumes this module
#[allow(dead_code)]
mod active_window;
mod audio;
mod clipboard;
mod commands;
mod dictionary;
mod history;
mod hotkey;
mod keyboard;
mod licensing;
mod state;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    path::BaseDirectory,
    tray::TrayIconBuilder,
    Emitter, Listener, Manager,
};
use tauri_plugin_store::StoreExt;
use tokio::sync::Mutex;

use voxpen_core::pipeline::controller::{PipelineConfig, PipelineController};
use voxpen_core::pipeline::settings::Settings;
use voxpen_core::pipeline::state::{Language, PipelineState, TonePreset};

use state::{AppState, GroqLlmProvider, GroqSttProvider};

/// All supported languages for the tray submenu.
const ALL_LANGUAGES: &[(&str, Language)] = &[
    ("Auto-detect", Language::Auto),
    ("中文", Language::Chinese),
    ("English", Language::English),
    ("日本語", Language::Japanese),
    ("한국어", Language::Korean),
    ("Français", Language::French),
    ("Deutsch", Language::German),
    ("Español", Language::Spanish),
    ("Tiếng Việt", Language::Vietnamese),
    ("Bahasa Indonesia", Language::Indonesian),
    ("ภาษาไทย", Language::Thai),
];

/// All supported tone presets for the tray submenu.
const ALL_TONES: &[(&str, TonePreset)] = &[
    ("Casual", TonePreset::Casual),
    ("Professional", TonePreset::Professional),
    ("Email", TonePreset::Email),
    ("Note", TonePreset::Note),
    ("Social", TonePreset::Social),
    ("Custom", TonePreset::Custom),
];

/// Format the tray usage text based on license tier and categorized usage status.
fn format_usage_text(tier: &voxpen_core::licensing::LicenseTier, status: &voxpen_core::licensing::CategorizedUsageStatus) -> String {
    use voxpen_core::licensing::{LicenseTier, UsageStatus, free_daily_limit, UsageCategory};
    match tier {
        LicenseTier::Pro => "Unlimited (Pro)".to_string(),
        LicenseTier::Free => {
            let voice_limit = free_daily_limit(UsageCategory::VoiceInput);
            let voice_used = match &status.voice_input {
                UsageStatus::Available { remaining } | UsageStatus::Warning { remaining } => {
                    voice_limit - remaining
                }
                UsageStatus::Exhausted => voice_limit,
                UsageStatus::Unlimited => 0,
            };
            format!("Free: {voice_used}/{voice_limit} voice today")
        }
    }
}

/// Build the tray menu with language submenu, tone submenu, and microphone submenu.
///
/// Returns the menu, the usage `MenuItem`, and the upgrade `MenuItem` so the
/// caller can update them dynamically after license changes.
fn build_tray_menu(
    app: &tauri::App,
    current_lang: &Language,
    current_tone: &TonePreset,
    mic_devices: &[String],
    current_mic: &Option<String>,
    usage_text: &str,
    is_pro: bool,
) -> tauri::Result<(Menu<tauri::Wry>, MenuItem<tauri::Wry>, MenuItem<tauri::Wry>)> {
    // Language submenu
    let mut lang_items: Vec<CheckMenuItem<tauri::Wry>> = Vec::new();
    for (label, lang) in ALL_LANGUAGES {
        let id = format!("lang_{}", serde_json::to_string(lang).unwrap_or_default().trim_matches('"'));
        let checked = lang == current_lang;
        let item = CheckMenuItem::with_id(app, &id, *label, true, checked, None::<&str>)?;
        lang_items.push(item);
    }
    let lang_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        lang_items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();
    let lang_submenu = Submenu::with_items(app, "Language", true, &lang_refs)?;

    // Tone submenu
    let mut tone_items: Vec<CheckMenuItem<tauri::Wry>> = Vec::new();
    for (label, tone) in ALL_TONES {
        let id = format!("tone_{}", serde_json::to_string(tone).unwrap_or_default().trim_matches('"'));
        let checked = tone == current_tone;
        let item = CheckMenuItem::with_id(app, &id, *label, true, checked, None::<&str>)?;
        tone_items.push(item);
    }
    let tone_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        tone_items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();
    let tone_submenu = Submenu::with_items(app, "Tone", true, &tone_refs)?;

    // Microphone submenu
    let default_mic = CheckMenuItem::with_id(
        app,
        "mic_default",
        "System Default",
        true,
        current_mic.is_none(),
        None::<&str>,
    )?;
    let mut mic_items: Vec<CheckMenuItem<tauri::Wry>> = vec![default_mic];
    for name in mic_devices {
        let id = format!("mic_{name}");
        let checked = current_mic.as_deref() == Some(name);
        let item = CheckMenuItem::with_id(app, &id, name, true, checked, None::<&str>)?;
        mic_items.push(item);
    }
    let mic_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        mic_items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();
    let mic_submenu = Submenu::with_items(app, "Microphone", true, &mic_refs)?;

    let usage_item = MenuItem::with_id(app, "usage_info", usage_text, false, None::<&str>)?;
    let upgrade_item = MenuItem::with_id(app, "upgrade_pro", "Upgrade to Pro...", !is_pro, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let update_item = MenuItem::with_id(app, "check_update", "Check for Updates", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit VoxPen", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &usage_item,
            &upgrade_item,
            &lang_submenu,
            &tone_submenu,
            &mic_submenu,
            &sep1,
            &update_item,
            &settings_item,
            &sep2,
            &quit_item,
        ],
    )?;
    Ok((menu, usage_item, upgrade_item))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            // Initialize shared settings
            let settings = Arc::new(Mutex::new(Settings::default()));

            // Initialize SQLite history database (needs app_data_dir early for models_dir)
            let app_data_dir = app
                .path()
                .resolve("", BaseDirectory::AppData)
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("failed to create app data dir");

            // Prepare models directory for local whisper model files
            let models_dir = app_data_dir.join("models");

            // Initialize local STT provider (feature-gated)
            #[cfg(feature = "local-whisper")]
            let local_stt = {
                use voxpen_core::whisper::models;
                let default_model = models::default_local_model();
                let model_path = models_dir.join(default_model.filename);
                Arc::new(voxpen_core::whisper::provider::LocalSttProvider::new(
                    model_path,
                    Language::Auto,
                ))
            };

            // Initialize pipeline controller with concrete providers
            let config = PipelineConfig::new(None, Language::Auto);
            #[cfg(feature = "local-whisper")]
            let stt = GroqSttProvider::new(
                settings.clone(),
                app.handle().clone(),
                local_stt.clone(),
            );
            #[cfg(not(feature = "local-whisper"))]
            let stt = GroqSttProvider::new(
                settings.clone(),
                app.handle().clone(),
                &models_dir,
            );
            let llm = GroqLlmProvider::new(settings.clone(), app.handle().clone());
            let controller = PipelineController::new(config, stt, llm);

            // Initialize hardware implementations
            let recorder = audio::CpalRecorder::new();
            let clipboard_mgr =
                clipboard::ArboardClipboard::new().expect("failed to init clipboard");
            let keyboard_mgr =
                keyboard::EnigoKeyboard::new().expect("failed to init keyboard simulator");
            let db_path = app_data_dir.join("voxpen.db");
            let history_db =
                history::HistoryDb::open(db_path.clone()).expect("failed to open history DB");
            let dictionary_db =
                dictionary::DictionaryDb::open(db_path.clone()).expect("failed to open dictionary DB");

            // Initialize licensing
            let license_store = licensing::TauriLicenseStore::new(app.handle().clone());
            let usage_db = licensing::SqliteUsageDb::open(db_path)
                .expect("failed to open usage DB");
            let verifier = voxpen_core::licensing::DirectLemonSqueezy::new();
            let license_mgr = voxpen_core::licensing::LicenseManager::new(
                verifier, license_store, usage_db,
            );

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
                recording_timeout_handle: Arc::new(tokio::sync::Mutex::new(None)),
                voice_edit_selection: Arc::new(tokio::sync::Mutex::new(None)),
                license_manager: Arc::new(license_mgr),
                models_dir,
                #[cfg(feature = "local-whisper")]
                local_stt,
            };
            app.manage(app_state);

            // Spawn background task to emit pipeline state changes to React + update tray
            {
                let app_handle = app.handle().clone();
                let state: tauri::State<'_, AppState> = app.state();
                let controller = state.controller.clone();

                // Query initial license status for tray display
                let state_ref: tauri::State<'_, AppState> = app.state();
                let tier = state_ref.license_manager.current_tier();
                let initial_usage = tauri::async_runtime::block_on(
                    state_ref.license_manager.check_access(),
                );
                let usage_text = format_usage_text(&tier, &initial_usage);
                let is_pro = tier == voxpen_core::licensing::LicenseTier::Pro;

                // Build system tray menu
                let mic_devices = audio::list_input_devices();
                let (menu, usage_item, upgrade_item) = build_tray_menu(app, &Language::Auto, &TonePreset::Casual, &mic_devices, &None, &usage_text, is_pro)?;

                let icon = Image::from_bytes(include_bytes!("../icons/icon.png"))
                    .expect("failed to load tray icon");
                let tray = TrayIconBuilder::new()
                    .icon(icon)
                    .icon_as_template(cfg!(target_os = "macos"))
                    .menu(&menu)
                    .show_menu_on_left_click(true)
                    .tooltip("VoxPen — Ready")
                    .on_menu_event(|app, event| {
                        let id = event.id.as_ref();
                        match id {
                            "settings" => {
                                if let Some(window) = app.get_webview_window("settings") {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                            "quit" => {
                                app.exit(0);
                            }
                            "upgrade_pro" => {
                                let _ = open::that("https://anatomind.lemonsqueezy.com/checkout/buy/299dd747-4424-4b1b-8aa8-2c47a94f7dd1");
                            }
                            "check_update" => {
                                tauri::async_runtime::spawn(async move {
                                    match commands::check_for_update().await {
                                        Ok(info) if info.update_available => {
                                            let _ = open::that(&info.download_url);
                                        }
                                        Ok(_) => {
                                            eprintln!("VoxPen is up to date");
                                        }
                                        Err(e) => {
                                            eprintln!("update check failed: {e}");
                                        }
                                    }
                                });
                            }
                            _ if id.starts_with("lang_") => {
                                let lang_str = id.strip_prefix("lang_").unwrap_or("");
                                let lang: Option<Language> =
                                    serde_json::from_str(&format!("\"{lang_str}\"")).ok();
                                if let Some(lang) = lang {
                                    let app = app.clone();
                                    tauri::async_runtime::spawn(async move {
                                        let state: tauri::State<'_, AppState> = app.state();
                                        let mut s = state.settings.lock().await;
                                        s.stt_language = lang;
                                        let settings_clone = s.clone();
                                        drop(s);

                                        // Sync controller config
                                        let mut ctrl = state.controller.lock().await;
                                        ctrl.update_config(PipelineConfig {
                                            groq_api_key: None,
                                            language: settings_clone.stt_language.clone(),
                                            stt_model: settings_clone.stt_model.clone(),
                                            refinement_enabled: settings_clone.refinement_enabled,
                                            llm_api_key: None,
                                            llm_model: settings_clone.refinement_model.clone(),
                                            voice_commands_enabled: settings_clone.voice_commands_enabled,
                                        });
                                        drop(ctrl);

                                        // Persist
                                        if let Ok(store) = app.store("settings.json") {
                                            if let Ok(value) = serde_json::to_value(&settings_clone) {
                                                store.set("settings", value);
                                                let _ = store.save();
                                            }
                                        }
                                    });
                                }
                            }
                            _ if id.starts_with("tone_") => {
                                let tone_str = id.strip_prefix("tone_").unwrap_or("");
                                let tone: Option<TonePreset> =
                                    serde_json::from_str(&format!("\"{tone_str}\"")).ok();
                                if let Some(tone) = tone {
                                    let app = app.clone();
                                    tauri::async_runtime::spawn(async move {
                                        let state: tauri::State<'_, AppState> = app.state();
                                        let mut s = state.settings.lock().await;
                                        s.tone_preset = tone;
                                        let settings_clone = s.clone();
                                        drop(s);

                                        // Persist
                                        if let Ok(store) = app.store("settings.json") {
                                            if let Ok(value) = serde_json::to_value(&settings_clone) {
                                                store.set("settings", value);
                                                let _ = store.save();
                                            }
                                        }
                                    });
                                }
                            }
                            _ if id.starts_with("mic_") => {
                                let mic_name = if id == "mic_default" {
                                    None
                                } else {
                                    Some(id.strip_prefix("mic_").unwrap_or("").to_string())
                                };
                                let app = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    let state: tauri::State<'_, AppState> = app.state();
                                    let mut s = state.settings.lock().await;
                                    s.microphone_device = mic_name;
                                    let settings_clone = s.clone();
                                    drop(s);

                                    // Persist
                                    if let Ok(store) = app.store("settings.json") {
                                        if let Ok(value) = serde_json::to_value(&settings_clone) {
                                            store.set("settings", value);
                                            let _ = store.save();
                                        }
                                    }
                                });
                            }
                            _ => {}
                        }
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
                            PipelineState::Idle => "VoxPen — Ready",
                            PipelineState::Recording => "VoxPen — Recording...",
                            PipelineState::Processing
                            | PipelineState::Refining { .. } => "VoxPen — Processing...",
                            PipelineState::Result { .. }
                            | PipelineState::Refined { .. } => "VoxPen — Done",
                            PipelineState::Error { .. } => "VoxPen — Error",
                        };
                        let _ = tray.set_tooltip(Some(tooltip));

                        // Show/hide overlay window
                        if let Some(overlay) = app_handle.get_webview_window("overlay") {
                            match &pipeline_state {
                                PipelineState::Idle => {
                                    let _ = overlay.hide();
                                }
                                _ => {
                                    // Position at bottom-center of current monitor
                                    if let Ok(Some(monitor)) = overlay.current_monitor() {
                                        let screen = monitor.size();
                                        let scale = monitor.scale_factor();
                                        let win_w = 280.0;
                                        let win_h = 56.0;
                                        let x = (screen.width as f64 / scale - win_w) / 2.0;
                                        let y = screen.height as f64 / scale - win_h - 48.0;
                                        let _ = overlay.set_position(
                                            tauri::LogicalPosition::new(x, y),
                                        );
                                    }
                                    let _ = overlay.show();
                                    let _ = overlay.set_always_on_top(true);
                                    let _ = overlay.set_ignore_cursor_events(true);
                                }
                            }
                        }
                    }
                });

                // Listen for usage-updated events and refresh the tray menu item
                let license_mgr_for_tray = {
                    let s: tauri::State<'_, AppState> = app.state();
                    s.license_manager.clone()
                };
                app.listen("usage-updated", move |_event| {
                    let tier = license_mgr_for_tray.current_tier();
                    let status = tauri::async_runtime::block_on(
                        license_mgr_for_tray.check_access(),
                    );
                    let text = format_usage_text(&tier, &status);
                    let _ = usage_item.set_text(&text);
                });

                // Listen for license-tier-changed to update the "Upgrade to Pro" item
                app.listen("license-tier-changed", move |event| {
                    let is_now_pro = serde_json::from_str::<String>(event.payload())
                        .map(|t| t == "Pro")
                        .unwrap_or(false);
                    let _ = upgrade_item.set_enabled(!is_now_pro);
                });
            }

            // Register dual hotkeys from saved settings
            {
                let saved_settings: Settings = app
                    .store("settings.json")
                    .ok()
                    .and_then(|s| s.get("settings"))
                    .and_then(|v: serde_json::Value| serde_json::from_value(v).ok())
                    .unwrap_or_default();

                let state: tauri::State<'_, AppState> = app.state();
                let mut mgr = state.hotkey_manager.blocking_lock();
                if let Err(e) = mgr.register_all(
                    app.handle(),
                    &saved_settings.hotkey_ptt,
                    &saved_settings.hotkey_toggle,
                    &saved_settings.hotkey_edit,
                ) {
                    eprintln!("failed to register hotkeys: {e}");
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
            commands::open_url,
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
            commands::get_default_refinement_prompt,
            commands::get_dictionary_entries,
            commands::get_dictionary_count,
            commands::add_dictionary_entry,
            commands::delete_dictionary_entry,
            commands::list_input_devices,
            commands::check_for_update,
            commands::get_whisper_models,
            commands::get_model_status,
            commands::download_whisper_model,
            commands::delete_whisper_model,
            commands::activate_license,
            commands::deactivate_license,
            commands::get_license_info,
            commands::get_usage_status,
            commands::get_license_tier,
            commands::transcribe_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxpen_core::licensing::{CategorizedUsageStatus, LicenseTier, UsageStatus};

    fn all_unlimited() -> CategorizedUsageStatus {
        CategorizedUsageStatus {
            voice_input: UsageStatus::Unlimited,
            refinement: UsageStatus::Unlimited,
            file_transcription: UsageStatus::Unlimited,
        }
    }

    fn free_status(voice: UsageStatus) -> CategorizedUsageStatus {
        CategorizedUsageStatus {
            voice_input: voice,
            refinement: UsageStatus::Available { remaining: 10 },
            file_transcription: UsageStatus::Available { remaining: 2 },
        }
    }

    #[test]
    fn should_format_pro_tier_as_unlimited() {
        let text = format_usage_text(&LicenseTier::Pro, &all_unlimited());
        assert_eq!(text, "Unlimited (Pro)");
    }

    #[test]
    fn should_format_pro_tier_regardless_of_status() {
        let status = free_status(UsageStatus::Available { remaining: 5 });
        let text = format_usage_text(&LicenseTier::Pro, &status);
        assert_eq!(text, "Unlimited (Pro)");
    }

    #[test]
    fn should_format_free_tier_with_zero_usage() {
        let status = free_status(UsageStatus::Available { remaining: 30 });
        let text = format_usage_text(&LicenseTier::Free, &status);
        assert_eq!(text, "Free: 0/30 voice today");
    }

    #[test]
    fn should_format_free_tier_with_partial_usage() {
        let status = free_status(UsageStatus::Available { remaining: 20 });
        let text = format_usage_text(&LicenseTier::Free, &status);
        assert_eq!(text, "Free: 10/30 voice today");
    }

    #[test]
    fn should_format_free_tier_with_warning() {
        let status = free_status(UsageStatus::Warning { remaining: 2 });
        let text = format_usage_text(&LicenseTier::Free, &status);
        assert_eq!(text, "Free: 28/30 voice today");
    }

    #[test]
    fn should_format_free_tier_exhausted() {
        let status = free_status(UsageStatus::Exhausted);
        let text = format_usage_text(&LicenseTier::Free, &status);
        assert_eq!(text, "Free: 30/30 voice today");
    }
}
