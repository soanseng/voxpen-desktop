use serde::Serialize;
use tauri::Emitter;
use tauri_plugin_store::StoreExt;

use voxpen_core::dictionary::DictionaryEntry;
use voxpen_core::licensing::types::{
    CategorizedUsageStatus, LicenseInfo, LicenseTier, UsageCategory,
};
use voxpen_core::pipeline::prompts;
use voxpen_core::history::TranscriptionEntry;
use voxpen_core::pipeline::controller::PipelineConfig;
use voxpen_core::pipeline::settings::Settings;
use voxpen_core::pipeline::state::{Language, TonePreset};

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

/// Open a URL in the system's default browser.
///
/// Tauri's webview CSP blocks `window.open()` for external URLs,
/// so the frontend must delegate to this Rust command instead.
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| e.to_string())
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

    // Update local whisper model path and language when provider is "local"
    #[cfg(feature = "local-whisper")]
    if settings.stt_provider == "local" {
        if let Some(model) = voxpen_core::whisper::models::model_by_id(&settings.stt_model) {
            let path = voxpen_core::whisper::models::model_path(
                &state.models_dir,
                model.filename,
            );
            state.local_stt.set_model_path(path);
        }
        state.local_stt.set_language(settings.stt_language.clone());
    }

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
    use voxpen_core::api::groq::{self, SttConfig};
    use voxpen_core::pipeline::state::Language;

    if provider != "groq" {
        return Err(format!("unsupported provider: {provider}"));
    }

    // Send a tiny silent WAV to validate the API key
    let silent_pcm: Vec<i16> = vec![0; 16000]; // 1 second of silence
    let wav_data = voxpen_core::audio::encoder::pcm_to_wav(&silent_pcm);

    let config = SttConfig {
        api_key: key,
        model: groq::DEFAULT_STT_MODEL.to_string(),
        language: Language::Auto,
        response_format: "verbose_json".to_string(),
        prompt_override: None,
    };

    match groq::transcribe(&config, &wav_data).await {
        Ok(_) => Ok(true),
        Err(voxpen_core::error::AppError::ApiKeyMissing(_)) => Ok(false),
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

/// Return the built-in refinement system prompt for a given language and tone.
///
/// Used by the frontend to show the default and to implement the "Reset" button.
#[tauri::command]
pub async fn get_default_refinement_prompt(language: Language, tone: TonePreset) -> String {
    prompts::for_language_and_tone(&language, &tone).to_string()
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
    let url = "https://api.github.com/repos/AkashiSN/voxpen-desktop/releases/latest";

    let client = reqwest::Client::builder()
        .user_agent("VoxPen-Desktop")
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
        .unwrap_or("https://github.com/AkashiSN/voxpen-desktop/releases")
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
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<LicenseInfo, String> {
    let info = state
        .license_manager
        .activate(&key)
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit("usage-updated", ());
    let _ = app.emit("license-tier-changed", info.tier.clone());
    Ok(info)
}

/// Deactivate the current license and clear local storage.
#[tauri::command]
pub async fn deactivate_license(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state
        .license_manager
        .deactivate()
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit("usage-updated", ());
    let _ = app.emit("license-tier-changed", LicenseTier::Free);
    Ok(())
}

/// Get the stored license information, if any.
#[tauri::command]
pub async fn get_license_info(
    state: tauri::State<'_, AppState>,
) -> Result<Option<LicenseInfo>, String> {
    Ok(state.license_manager.license_info())
}

/// Check usage access: piggyback verify + return per-category status.
#[tauri::command]
pub async fn get_usage_status(
    state: tauri::State<'_, AppState>,
) -> Result<CategorizedUsageStatus, String> {
    Ok(state.license_manager.check_access().await)
}

/// Get the current license tier (Free or Pro).
#[tauri::command]
pub async fn get_license_tier(
    state: tauri::State<'_, AppState>,
) -> Result<LicenseTier, String> {
    Ok(state.license_manager.current_tier())
}

// ---------------------------------------------------------------------------
// Audio file transcription (Pro feature)
// ---------------------------------------------------------------------------

/// Result of a file transcription.
#[derive(Debug, Clone, Serialize)]
pub struct FileTranscriptionResult {
    pub text: String,
    pub refined: Option<String>,
}

/// Supported audio MIME types for file transcription.
fn mime_for_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "wav" => Some("audio/wav"),
        "mp3" => Some("audio/mpeg"),
        "flac" => Some("audio/flac"),
        "m4a" => Some("audio/mp4"),
        "ogg" | "oga" => Some("audio/ogg"),
        "webm" => Some("audio/webm"),
        _ => None,
    }
}

/// Transcribe an audio file.
///
/// Free users get 2/day; Pro users are unlimited.
/// Reads the file, validates format and size, sends to the STT API,
/// optionally refines with LLM, records usage, and saves to history.
#[tauri::command]
pub async fn transcribe_file(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    file_path: String,
) -> Result<FileTranscriptionResult, String> {
    use voxpen_core::api::groq::{self, SttConfig, ChatConfig};
    use voxpen_core::pipeline::refine;
    use voxpen_core::licensing::UsageStatus;

    // Check FileTranscription quota
    let file_status = state
        .license_manager
        .check_category(UsageCategory::FileTranscription);
    if file_status == UsageStatus::Exhausted {
        return Err("Daily file transcription limit reached. Upgrade to Pro for unlimited access.".to_string());
    }

    // Validate file extension
    let path = std::path::Path::new(&file_path);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    let mime = mime_for_extension(&ext)
        .ok_or_else(|| format!("Unsupported format: .{ext}. Supported: wav, mp3, flac, m4a, ogg, webm"))?;
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("audio.wav")
        .to_string();

    // Read file
    let file_data = std::fs::read(&file_path)
        .map_err(|e| format!("Failed to read file: {e}"))?;

    // Size check (25MB API limit)
    if file_data.len() > 25 * 1024 * 1024 {
        return Err("File too large (max 25 MB). Please use a shorter audio clip.".to_string());
    }

    // Build STT config from settings
    let s = state.settings.lock().await;
    let stt_provider = s.stt_provider.clone();
    let api_key = crate::state::get_api_key_from_handle(&app, &stt_provider)
        .map_err(|e| e.to_string())?;
    let config = SttConfig {
        api_key,
        model: s.stt_model.clone(),
        language: s.stt_language.clone(),
        response_format: "verbose_json".to_string(),
        prompt_override: None,
    };
    let refinement_enabled = s.refinement_enabled;
    let refinement_provider = s.refinement_provider.clone();
    let refinement_model = s.refinement_model.clone();
    let custom_prompt = s.refinement_prompt.clone();
    let tone_preset = s.tone_preset.clone();
    let custom_base_url = s.custom_base_url.clone();
    let language = s.stt_language.clone();
    drop(s);

    // Transcribe
    let text = groq::transcribe_file(&config, &file_data, &filename, mime, &stt_provider)
        .await
        .map_err(|e| e.to_string())?;

    // Optional LLM refinement
    let refined = if refinement_enabled {
        let llm_key = crate::state::get_api_key_from_handle(&app, &refinement_provider)
            .map_err(|e| e.to_string())?;
        let chat_config = ChatConfig {
            api_key: llm_key,
            model: refinement_model,
            temperature: groq::LLM_TEMPERATURE,
            max_tokens: groq::LLM_MAX_TOKENS,
        };
        match refine::refine(
            &text, &chat_config, &language, &[], &custom_prompt,
            &tone_preset, &refinement_provider, &custom_base_url,
            None, // translation_target — wired in next task (TM-4)
        ).await {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!("refinement failed for file transcription: {e}");
                None
            }
        }
    } else {
        None
    };

    // Record per-category usage
    let _ = state
        .license_manager
        .record_usage(UsageCategory::FileTranscription);
    if refined.is_some() {
        let _ = state
            .license_manager
            .record_usage(UsageCategory::Refinement);
    }
    let _ = app.emit("usage-updated", ());

    // Save to history
    let entry = voxpen_core::history::TranscriptionEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64,
        original_text: text.clone(),
        refined_text: refined.clone(),
        language: language.clone(),
        audio_duration_ms: 0, // unknown for file uploads
        provider: stt_provider,
    };
    if let Err(e) = state.history.insert(&entry) {
        eprintln!("history insert error: {e}");
    }

    Ok(FileTranscriptionResult {
        text,
        refined,
    })
}

// ---------------------------------------------------------------------------
// Local whisper model management IPC commands
// ---------------------------------------------------------------------------

/// List available local whisper models from the built-in catalog.
#[tauri::command]
pub async fn get_whisper_models() -> &'static [voxpen_core::whisper::models::WhisperModel] {
    voxpen_core::whisper::models::MODEL_CATALOG
}

/// Check the download / readiness status of a local whisper model.
#[tauri::command]
pub async fn get_model_status(
    state: tauri::State<'_, AppState>,
    model_id: String,
) -> Result<voxpen_core::whisper::models::ModelStatus, String> {
    let model = voxpen_core::whisper::models::model_by_id(&model_id)
        .ok_or_else(|| format!("unknown model: {model_id}"))?;
    Ok(voxpen_core::whisper::models::get_model_status(
        &state.models_dir,
        model,
    ))
}

/// Download a whisper model file with streaming progress events.
///
/// Emits `model-download-progress` events with `{ model_id, downloaded, total, progress }`
/// so the frontend can display a progress bar.
#[tauri::command]
pub async fn download_whisper_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    model_id: String,
) -> Result<(), String> {
    use tauri::Emitter;

    let model = voxpen_core::whisper::models::model_by_id(&model_id)
        .ok_or_else(|| format!("unknown model: {model_id}"))?;
    let models_dir = state.models_dir.clone();
    let model_id_for_cb = model_id.clone();

    voxpen_core::whisper::download::download_model(
        model.url,
        model.filename,
        &models_dir,
        move |downloaded, total| {
            let progress = if total > 0 {
                downloaded as f32 / total as f32
            } else {
                0.0
            };
            let _ = app.emit(
                "model-download-progress",
                serde_json::json!({
                    "model_id": model_id_for_cb,
                    "downloaded": downloaded,
                    "total": total,
                    "progress": progress,
                }),
            );
        },
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Delete a downloaded whisper model file and any in-progress .part file.
#[tauri::command]
pub async fn delete_whisper_model(
    state: tauri::State<'_, AppState>,
    model_id: String,
) -> Result<(), String> {
    let model = voxpen_core::whisper::models::model_by_id(&model_id)
        .ok_or_else(|| format!("unknown model: {model_id}"))?;
    voxpen_core::whisper::models::delete_model(&state.models_dir, model)
        .map_err(|e| e.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_return_correct_mime_for_supported_extensions() {
        assert_eq!(mime_for_extension("wav"), Some("audio/wav"));
        assert_eq!(mime_for_extension("mp3"), Some("audio/mpeg"));
        assert_eq!(mime_for_extension("flac"), Some("audio/flac"));
        assert_eq!(mime_for_extension("m4a"), Some("audio/mp4"));
        assert_eq!(mime_for_extension("ogg"), Some("audio/ogg"));
        assert_eq!(mime_for_extension("oga"), Some("audio/ogg"));
        assert_eq!(mime_for_extension("webm"), Some("audio/webm"));
    }

    #[test]
    fn should_return_none_for_unsupported_extensions() {
        assert_eq!(mime_for_extension("txt"), None);
        assert_eq!(mime_for_extension("pdf"), None);
        assert_eq!(mime_for_extension("mp4"), None);
        assert_eq!(mime_for_extension(""), None);
    }

    #[test]
    fn should_compare_versions_correctly() {
        assert!(version_newer("1.1.0", "1.0.0"));
        assert!(version_newer("2.0.0", "1.9.9"));
        assert!(!version_newer("1.0.0", "1.0.0"));
        assert!(!version_newer("0.9.0", "1.0.0"));
    }
}
