use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use tokio::sync::Mutex;

use voxpen_core::api::groq::{self, ChatConfig, SttConfig};
use voxpen_core::error::AppError;
use voxpen_core::pipeline::controller::{LlmProvider, PipelineController, SttProvider};
use voxpen_core::pipeline::refine;
use voxpen_core::pipeline::settings::Settings;
use voxpen_core::pipeline::state::{Language, TonePreset};
use voxpen_core::pipeline::transcribe;

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
    local_stt: Arc<voxpen_core::whisper::provider::LocalSttProvider>,
}

impl GroqSttProvider {
    #[cfg(feature = "local-whisper")]
    pub fn new(
        settings: Arc<Mutex<Settings>>,
        app_handle: AppHandle,
        local_stt: Arc<voxpen_core::whisper::provider::LocalSttProvider>,
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

            // Guard: "local" selected but local-whisper feature not compiled in
            #[cfg(not(feature = "local-whisper"))]
            if s.stt_provider == "local" {
                return Err(AppError::Transcription(
                    "local whisper is not available in this build".to_string(),
                ));
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
            let provider = s.stt_provider.clone();
            let custom_base_url = s.custom_base_url.clone();
            drop(s);
            let base_url = if provider == "custom" && !custom_base_url.is_empty() {
                custom_base_url
            } else {
                groq::base_url_for_provider(&provider).to_string()
            };
            transcribe::transcribe(
                &pcm_data,
                &config,
                vocabulary_hint.as_deref(),
                &provider,
                &base_url,
            )
            .await
        })
    }
}

/// Concrete LlmProvider backed by Groq Chat Completion API.
pub struct GroqLlmProvider {
    settings: Arc<Mutex<Settings>>,
    app_handle: AppHandle,
    /// Shared per-session tone override. None = use settings.tone_preset.
    auto_tone_override: Arc<tokio::sync::Mutex<Option<TonePreset>>>,
}

impl GroqLlmProvider {
    pub fn new(
        settings: Arc<Mutex<Settings>>,
        app_handle: AppHandle,
        auto_tone_override: Arc<tokio::sync::Mutex<Option<TonePreset>>>,
    ) -> Self {
        Self {
            settings,
            app_handle,
            auto_tone_override,
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
        let auto_tone_override = self.auto_tone_override.clone();
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
            let base_tone = s.tone_preset.clone();
            let provider = s.refinement_provider.clone();
            let custom_base_url = s.custom_base_url.clone();
            let translation_target = if s.translation_enabled {
                Some(s.translation_target.clone())
            } else {
                None
            };
            drop(s);
            // Use per-session auto-tone override if set, otherwise use settings tone.
            let tone_preset = {
                let guard = auto_tone_override.lock().await;
                guard.clone().unwrap_or(base_tone)
            };
            refine::refine(
                &text,
                &config,
                &language,
                &vocabulary,
                &custom_prompt,
                &tone_preset,
                &provider,
                &custom_base_url,
                translation_target.as_ref(),
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

/// Check if a provider can work without an API key.
///
/// Returns `true` for the `"custom"` provider name and for URL-like provider
/// strings (e.g. `"http://localhost:11434/"`), which typically point at local
/// inference servers that do not require authentication.
fn is_keyless_provider(provider: &str) -> bool {
    provider == "custom" || provider.starts_with("http://") || provider.starts_with("https://")
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

    // Local/custom endpoints (e.g. Ollama) may not require an API key.
    // Return empty string instead of error so the request proceeds with
    // a no-op Bearer header that local servers simply ignore.
    if is_keyless_provider(provider) {
        return Ok(String::new());
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
    /// Handle for the auto-stop timeout task. Aborted on manual stop.
    /// `None` when not recording.
    pub recording_timeout_handle: Arc<tokio::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    /// Captured selected text for voice-edit mode.
    /// Set at edit hotkey press time (after Ctrl+C), consumed at release.
    /// `None` when not in voice-edit mode.
    pub voice_edit_selection: Arc<tokio::sync::Mutex<Option<String>>>,
    /// Per-session tone preset override set by the auto-tone rule engine.
    /// Set at hotkey-press time when an AppToneRule matches the active app.
    /// Cleared after the pipeline completes. None = use settings.tone_preset.
    // TODO(task-4): remove allow(dead_code) once hotkey integration reads this field
    #[allow(dead_code)]
    pub auto_tone_override: Arc<tokio::sync::Mutex<Option<TonePreset>>>,
    pub license_manager: Arc<
        voxpen_core::licensing::LicenseManager<
            voxpen_core::licensing::DirectLemonSqueezy,
            crate::licensing::TauriLicenseStore,
            crate::licensing::SqliteUsageDb,
        >,
    >,
    /// Directory where local whisper model files are stored.
    pub models_dir: PathBuf,
    /// Shared local STT provider — accessible by both GroqSttProvider
    /// (for transcription dispatch) and IPC commands (for model/language updates).
    #[cfg(feature = "local-whisper")]
    pub local_stt: Arc<voxpen_core::whisper::provider::LocalSttProvider>,
}
