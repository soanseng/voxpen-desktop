use tauri_plugin_store::StoreExt;

use voxink_core::history::TranscriptionEntry;
use voxink_core::pipeline::settings::Settings;

/// Load settings from Tauri store, falling back to defaults.
#[tauri::command]
pub async fn get_settings(app: tauri::AppHandle) -> Result<Settings, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    match store.get("settings") {
        Some(value) => serde_json::from_value(value.clone()).map_err(|e| e.to_string()),
        None => Ok(Settings::default()),
    }
}

/// Save settings to Tauri store.
#[tauri::command]
pub async fn save_settings(app: tauri::AppHandle, settings: Settings) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let value = serde_json::to_value(&settings).map_err(|e| e.to_string())?;
    store.set("settings", value);
    store.save().map_err(|e| e.to_string())?;
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
    _app: tauri::AppHandle,
    _limit: u32,
    _offset: u32,
) -> Result<Vec<TranscriptionEntry>, String> {
    // TODO: Wire to SQLite via tauri-plugin-sql when DB is initialized
    Ok(vec![])
}

/// Search transcription history by text content.
#[tauri::command]
pub async fn search_history(
    _app: tauri::AppHandle,
    _query: String,
    _limit: u32,
    _offset: u32,
) -> Result<Vec<TranscriptionEntry>, String> {
    // TODO: Wire to SQLite via tauri-plugin-sql
    Ok(vec![])
}

/// Delete a history entry by id.
#[tauri::command]
pub async fn delete_history_entry(_app: tauri::AppHandle, _id: String) -> Result<(), String> {
    // TODO: Wire to SQLite via tauri-plugin-sql
    Ok(())
}
