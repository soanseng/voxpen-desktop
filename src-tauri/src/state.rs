use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use tokio::sync::Mutex;

use voxink_core::api::groq::{self, ChatConfig, SttConfig};
use voxink_core::error::AppError;
use voxink_core::pipeline::controller::{LlmProvider, PipelineController, SttProvider};
use voxink_core::pipeline::refine;
use voxink_core::pipeline::settings::Settings;
use voxink_core::pipeline::state::Language;
use voxink_core::pipeline::transcribe;

/// Concrete SttProvider backed by Groq Whisper API.
///
/// Holds a shared reference to the current settings so it always uses
/// the latest API key and model configuration.
pub struct GroqSttProvider {
    settings: Arc<Mutex<Settings>>,
    app_handle: AppHandle,
}

impl GroqSttProvider {
    pub fn new(settings: Arc<Mutex<Settings>>, app_handle: AppHandle) -> Self {
        Self {
            settings,
            app_handle,
        }
    }
}

impl SttProvider for GroqSttProvider {
    fn transcribe(
        &self,
        pcm_data: Vec<i16>,
        vocabulary_hint: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, AppError>> + Send>> {
        let settings = self.settings.clone();
        let app_handle = self.app_handle.clone();
        Box::pin(async move {
            let s = settings.lock().await;
            let api_key = get_api_key(&app_handle, &s.stt_provider)?;
            let config = SttConfig {
                api_key,
                model: s.stt_model.clone(),
                language: s.stt_language.clone(),
                response_format: "verbose_json".to_string(),
                prompt_override: None,
            };
            drop(s);
            transcribe::transcribe(&pcm_data, &config, vocabulary_hint.as_deref()).await
        })
    }
}

/// Concrete LlmProvider backed by Groq Chat Completion API.
pub struct GroqLlmProvider {
    settings: Arc<Mutex<Settings>>,
    app_handle: AppHandle,
}

impl GroqLlmProvider {
    pub fn new(settings: Arc<Mutex<Settings>>, app_handle: AppHandle) -> Self {
        Self {
            settings,
            app_handle,
        }
    }
}

impl LlmProvider for GroqLlmProvider {
    fn refine(
        &self,
        text: String,
        language: Language,
        vocabulary: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, AppError>> + Send>> {
        let settings = self.settings.clone();
        let app_handle = self.app_handle.clone();
        Box::pin(async move {
            let s = settings.lock().await;
            let api_key = get_api_key(&app_handle, &s.refinement_provider)?;
            let config = ChatConfig {
                api_key,
                model: s.refinement_model.clone(),
                temperature: groq::LLM_TEMPERATURE,
                max_tokens: groq::LLM_MAX_TOKENS,
            };
            drop(s);
            refine::refine(&text, &config, &language, &vocabulary).await
        })
    }
}

/// Resolve the API key for a given provider.
///
/// 1. Try the encrypted Tauri store (secrets.json) — set via save_api_key command
/// 2. Fallback to environment variable (for development)
fn get_api_key(app: &AppHandle, provider: &str) -> Result<String, AppError> {
    // Try Tauri store first (encrypted secrets)
    if let Ok(store) = app.store("secrets.json") {
        let store_key = format!("{}_api_key", provider);
        if let Some(value) = store.get(&store_key) {
            if let Some(key) = value.as_str() {
                if !key.is_empty() {
                    return Ok(key.to_string());
                }
            }
        }
    }

    // Fallback: environment variable (for development)
    let env_key = format!("{}_API_KEY", provider.to_uppercase());
    if let Ok(key) = std::env::var(&env_key) {
        if !key.is_empty() {
            return Ok(key);
        }
    }

    Err(AppError::ApiKeyMissing(provider.to_string()))
}

/// Shared application state managed by Tauri.
pub struct AppState {
    pub controller: Arc<Mutex<PipelineController<GroqSttProvider, GroqLlmProvider>>>,
    pub settings: Arc<Mutex<Settings>>,
    pub recorder: Arc<crate::audio::CpalRecorder>,
    pub clipboard: Arc<crate::clipboard::ArboardClipboard>,
    pub keyboard: Arc<crate::keyboard::EnigoKeyboard>,
    pub history: Arc<crate::history::HistoryDb>,
    pub dictionary: Arc<crate::dictionary::DictionaryDb>,
    pub hotkey_manager: Arc<Mutex<crate::hotkey::HotkeyManager>>,
}
