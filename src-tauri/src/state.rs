use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
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

/// Concrete SttProvider backed by Groq Whisper API (and optionally local whisper).
///
/// Holds a shared reference to the current settings so it always uses
/// the latest API key and model configuration. When the `local-whisper`
/// feature is enabled and `stt_provider == "local"`, transcription is
/// dispatched to the embedded whisper.cpp engine instead of a cloud API.
pub struct GroqSttProvider {
    settings: Arc<Mutex<Settings>>,
    app_handle: AppHandle,
    #[cfg(feature = "local-whisper")]
    local_stt: Arc<voxink_core::whisper::provider::LocalSttProvider>,
}

impl GroqSttProvider {
    #[cfg(feature = "local-whisper")]
    pub fn new(
        settings: Arc<Mutex<Settings>>,
        app_handle: AppHandle,
        local_stt: Arc<voxink_core::whisper::provider::LocalSttProvider>,
    ) -> Self {
        Self {
            settings,
            app_handle,
            local_stt,
        }
    }

    #[cfg(not(feature = "local-whisper"))]
    pub fn new(
        settings: Arc<Mutex<Settings>>,
        app_handle: AppHandle,
        _models_dir: &std::path::Path,
    ) -> Self {
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
        #[cfg(feature = "local-whisper")]
        let local_stt = self.local_stt.clone();

        Box::pin(async move {
            let s = settings.lock().await;

            // Dispatch to local whisper when stt_provider is "local"
            #[cfg(feature = "local-whisper")]
            if s.stt_provider == "local" {
                let lang = s.stt_language.clone();
                drop(s);
                local_stt.set_language(lang);
                return local_stt.transcribe(pcm_data, vocabulary_hint).await;
            }

            // Cloud STT path
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
            let custom_prompt = s.refinement_prompt.clone();
            let tone_preset = s.tone_preset.clone();
            let provider = s.refinement_provider.clone();
            let custom_base_url = s.custom_base_url.clone();
            drop(s);
            refine::refine(
                &text,
                &config,
                &language,
                &vocabulary,
                &custom_prompt,
                &tone_preset,
                &provider,
                &custom_base_url,
            )
            .await
        })
    }
}

/// Resolve the API key for a given provider from the AppHandle.
///
/// Public wrapper for use in Tauri commands.
pub fn get_api_key_from_handle(app: &AppHandle, provider: &str) -> Result<String, AppError> {
    get_api_key(app, provider)
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
    /// Signal set after recorder.start() succeeds so the release handler can
    /// wait for recording to actually begin before calling recorder.stop().
    pub recording_started: Arc<AtomicBool>,
    pub license_manager: Arc<
        voxink_core::licensing::LicenseManager<
            voxink_core::licensing::DirectLemonSqueezy,
            crate::licensing::TauriLicenseStore,
            crate::licensing::SqliteUsageDb,
        >,
    >,
    /// Directory where local whisper model files are stored.
    pub models_dir: PathBuf,
    /// Shared local STT provider — accessible by both GroqSttProvider
    /// (for transcription dispatch) and IPC commands (for model/language updates).
    #[cfg(feature = "local-whisper")]
    pub local_stt: Arc<voxink_core::whisper::provider::LocalSttProvider>,
}
