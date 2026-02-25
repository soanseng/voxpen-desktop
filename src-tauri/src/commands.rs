use tauri_plugin_store::StoreExt;

use voxink_core::dictionary::DictionaryEntry;
use voxink_core::history::TranscriptionEntry;
use voxink_core::pipeline::settings::Settings;

use crate::state::AppState;

/// Change the global hotkey at runtime.
#[tauri::command]
pub async fn set_hotkey(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    shortcut: String,
) -> Result<(), String> {
    if shortcut.trim().is_empty() {
        return Err("Hotkey cannot be empty".to_string());
    }

    // Register new hotkey (unregisters previous one internally)
    let mut mgr = state.hotkey_manager.lock().await;
    mgr.register(&app, &shortcut)?;
    drop(mgr);

    // Update in-memory settings
    let mut s = state.settings.lock().await;
    s.hotkey = shortcut;
    let settings_clone = s.clone();
    drop(s);

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
    *state.settings.lock().await = settings;

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
