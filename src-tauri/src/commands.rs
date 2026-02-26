use serde::Serialize;
use tauri_plugin_store::StoreExt;

use voxink_core::dictionary::DictionaryEntry;
use voxink_core::licensing::types::{LicenseInfo, LicenseTier, UsageStatus};
use voxink_core::pipeline::prompts;
use voxink_core::history::TranscriptionEntry;
use voxink_core::pipeline::controller::PipelineConfig;
use voxink_core::pipeline::settings::Settings;
use voxink_core::pipeline::state::Language;

use crate::state::AppState;

/// Build a PipelineConfig from the current Settings.
/// API keys are resolved at call time by the providers, so they stay None here.
fn config_from_settings(settings: &Settings) -> PipelineConfig {
    PipelineConfig {
        groq_api_key: None, // resolved by provider at call time
        language: settings.stt_language.clone(),
        stt_model: settings.stt_model.clone(),
        refinement_enabled: settings.refinement_enabled,
        llm_api_key: None, // resolved by provider at call time
        llm_model: settings.refinement_model.clone(),
    }
}

/// Change a hotkey at runtime. `kind` is "ptt" or "toggle".
#[tauri::command]
pub async fn set_hotkey(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    shortcut: String,
    kind: String,
) -> Result<(), String> {
    if shortcut.trim().is_empty() {
        return Err("Hotkey cannot be empty".to_string());
    }

    // Update in-memory settings first
    let mut s = state.settings.lock().await;
    match kind.as_str() {
        "ptt" => s.hotkey_ptt = shortcut.clone(),
        "toggle" => s.hotkey_toggle = shortcut.clone(),
        _ => return Err(format!("Unknown hotkey kind: {kind}")),
    }
    let settings_clone = s.clone();
    drop(s);

    // Re-register both hotkeys
    let mut mgr = state.hotkey_manager.lock().await;
    mgr.register_dual(
        &app,
        &settings_clone.hotkey_ptt,
        &settings_clone.hotkey_toggle,
    )?;
    drop(mgr);

    // Persist to store
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let value = serde_json::to_value(&settings_clone).map_err(|e| e.to_string())?;
    store.set("settings", value);
    store.save().map_err(|e| e.to_string())?;

    Ok(())
}

/// Load settings from Tauri store, falling back to defaults.
/// Also syncs the loaded settings to shared in-memory state.
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

    // Sync to shared settings so providers use the latest values
    *state.settings.lock().await = settings.clone();

    // Sync pipeline controller config (refinement_enabled, language, models)
    let mut ctrl = state.controller.lock().await;
    ctrl.update_config(config_from_settings(&settings));
    drop(ctrl);

    Ok(settings)
}

/// Save settings to Tauri store and sync to shared in-memory state.
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

    // Sync to shared settings so providers use the latest values
    *state.settings.lock().await = settings.clone();

    // Sync pipeline controller config (refinement_enabled, language, models)
    let mut ctrl = state.controller.lock().await;
    ctrl.update_config(config_from_settings(&settings));
    drop(ctrl);

    Ok(())
}

/// Save an API key to encrypted store.
#[tauri::command]
pub async fn save_api_key(
    app: tauri::AppHandle,
    provider: String,
    key: String,
) -> Result<(), String> {
    let store = app.store("secrets.json").map_err(|e| e.to_string())?;
    let store_key = format!("{}_api_key", provider);
    store.set(&store_key, serde_json::Value::String(key));
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

/// Return a masked preview of the stored API key, or null if not configured.
///
/// Masking: first 4 chars + "····" + last 4 chars (e.g. "gsk-····abc4").
/// Keys shorter than 12 chars get "****" instead.
#[tauri::command]
pub async fn get_api_key_status(
    app: tauri::AppHandle,
    provider: String,
) -> Result<Option<String>, String> {
    let store = app.store("secrets.json").map_err(|e| e.to_string())?;
    let store_key = format!("{}_api_key", provider);
    let masked = store
        .get(&store_key)
        .and_then(|v| v.as_str().map(String::from))
        .filter(|k| !k.is_empty())
        .map(|k| {
            if k.len() >= 12 {
                format!("{}····{}", &k[..4], &k[k.len() - 4..])
            } else {
                "****".to_string()
            }
        });
    Ok(masked)
}

/// Probe the default input (microphone) device and return its name.
///
/// Returns the device name on success, or an error string describing why
/// the microphone is unavailable (no device, permission denied, etc.).
#[tauri::command]
pub async fn check_microphone() -> Result<String, String> {
    // cpal device enumeration is blocking — run off the async executor
    tokio::task::spawn_blocking(|| {
        use cpal::traits::{DeviceTrait, HostTrait};
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "No microphone found. Please connect a microphone.".to_string())?;
        let name = device.name().unwrap_or_else(|_| "Unknown device".to_string());
        // Use the device's preferred config — avoids "unsupported config"
        // on Windows devices that don't natively support 16 kHz mono.
        let default_cfg = device
            .default_input_config()
            .map_err(|e| format!("Cannot access microphone: {e}"))?;
        let config = cpal::StreamConfig {
            channels: default_cfg.channels(),
            sample_rate: default_cfg.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };
        // Try building a short-lived stream to verify permissions
        let stream = device
            .build_input_stream(
                &config,
                |_data: &[i16], _: &cpal::InputCallbackInfo| {},
                |_err| {},
                None,
            )
            .map_err(|e| {
                let msg = e.to_string().to_lowercase();
                if msg.contains("access") || msg.contains("denied") || msg.contains("0x80070005") || msg.contains("not activated") {
                    format!("Microphone access denied. Please enable in Settings > Privacy > Microphone. ({e})")
                } else {
                    format!("Microphone error: {e}")
                }
            })?;
        drop(stream);
        Ok(name)
    })
    .await
    .map_err(|e| format!("check failed: {e}"))?
}

/// Test an API key by making a minimal transcription request.
#[tauri::command]
pub async fn test_api_key(provider: String, key: String) -> Result<bool, String> {
    use voxink_core::api::groq::{self, SttConfig};
    use voxink_core::pipeline::state::Language;

    if provider != "groq" {
        return Err(format!("unsupported provider: {provider}"));
    }

    // Send a tiny silent WAV to validate the API key
    let silent_pcm: Vec<i16> = vec![0; 16000]; // 1 second of silence
    let wav_data = voxink_core::audio::encoder::pcm_to_wav(&silent_pcm);

    let config = SttConfig {
        api_key: key,
        model: groq::DEFAULT_STT_MODEL.to_string(),
        language: Language::Auto,
        response_format: "verbose_json".to_string(),
        prompt_override: None,
    };

    match groq::transcribe(&config, &wav_data).await {
        Ok(_) => Ok(true),
        Err(voxink_core::error::AppError::ApiKeyMissing(_)) => Ok(false),
        Err(_) => Ok(true), // Non-auth errors mean the key itself is valid
    }
}

/// Get transcription history with pagination.
#[tauri::command]
pub async fn get_history(
    state: tauri::State<'_, AppState>,
    limit: u32,
    offset: u32,
) -> Result<Vec<TranscriptionEntry>, String> {
    state.history.query(limit, offset)
}

/// Search transcription history by text content.
#[tauri::command]
pub async fn search_history(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: u32,
    offset: u32,
) -> Result<Vec<TranscriptionEntry>, String> {
    state.history.search(&query, limit, offset)
}

/// Delete a history entry by id.
#[tauri::command]
pub async fn delete_history_entry(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.history.delete(&id)
}

/// Return the built-in refinement system prompt for a given language.
///
/// Used by the frontend to show the default and to implement the "Reset" button.
#[tauri::command]
pub async fn get_default_refinement_prompt(language: Language) -> String {
    prompts::for_language(&language).to_string()
}

/// Get all dictionary entries.
#[tauri::command]
pub async fn get_dictionary_entries(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<DictionaryEntry>, String> {
    state.dictionary.get_all()
}

/// Get dictionary entry count.
#[tauri::command]
pub async fn get_dictionary_count(
    state: tauri::State<'_, AppState>,
) -> Result<usize, String> {
    state.dictionary.count()
}

/// Add a word to the dictionary.
#[tauri::command]
pub async fn add_dictionary_entry(
    state: tauri::State<'_, AppState>,
    word: String,
) -> Result<(), String> {
    state.dictionary.add(&word)
}

/// Delete a dictionary entry by id.
#[tauri::command]
pub async fn delete_dictionary_entry(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    state.dictionary.delete(id)
}

/// List available audio input devices.
#[tauri::command]
pub async fn list_input_devices() -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(crate::audio::list_input_devices)
        .await
        .map_err(|e| format!("failed to list devices: {e}"))
}

/// Check for app updates by querying the GitHub releases API.
#[derive(Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
    pub download_url: String,
}

#[tauri::command]
pub async fn check_for_update() -> Result<UpdateInfo, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let url = "https://api.github.com/repos/AkashiSN/voxink-desktop/releases/latest";

    let client = reqwest::Client::builder()
        .user_agent("VoxInk-Desktop")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp: serde_json::Value = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Parse error: {e}"))?;

    let tag = resp["tag_name"]
        .as_str()
        .ok_or("No tag_name in release")?
        .trim_start_matches('v')
        .to_string();

    let html_url = resp["html_url"]
        .as_str()
        .unwrap_or("https://github.com/AkashiSN/voxink-desktop/releases")
        .to_string();

    let update_available = version_newer(&tag, &current);

    Ok(UpdateInfo {
        current,
        latest: tag,
        update_available,
        download_url: html_url,
    })
}

// ---------------------------------------------------------------------------
// Licensing IPC commands
// ---------------------------------------------------------------------------

/// Activate a license key and store the result.
#[tauri::command]
pub async fn activate_license(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<LicenseInfo, String> {
    state
        .license_manager
        .activate(&key)
        .await
        .map_err(|e| e.to_string())
}

/// Deactivate the current license and clear local storage.
#[tauri::command]
pub async fn deactivate_license(
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state
        .license_manager
        .deactivate()
        .await
        .map_err(|e| e.to_string())
}

/// Get the stored license information, if any.
#[tauri::command]
pub async fn get_license_info(
    state: tauri::State<'_, AppState>,
) -> Result<Option<LicenseInfo>, String> {
    Ok(state.license_manager.license_info())
}

/// Check usage access: piggyback verify + return current status.
#[tauri::command]
pub async fn get_usage_status(
    state: tauri::State<'_, AppState>,
) -> Result<UsageStatus, String> {
    Ok(state.license_manager.check_access().await)
}

/// Get the current license tier (Free or Pro).
#[tauri::command]
pub async fn get_license_tier(
    state: tauri::State<'_, AppState>,
) -> Result<LicenseTier, String> {
    Ok(state.license_manager.current_tier())
}

/// Simple semver comparison: is `latest` newer than `current`?
fn version_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.split('.')
            .filter_map(|p| p.parse::<u32>().ok())
            .collect()
    };
    let l = parse(latest);
    let c = parse(current);
    l > c
}
