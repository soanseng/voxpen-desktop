use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::Mutex;

use voxink_core::api::groq::{self, ChatConfig, SttConfig};
use voxink_core::error::AppError;
use voxink_core::pipeline::controller::{LlmProvider, PipelineConfig, PipelineController, SttProvider};
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
}

impl GroqSttProvider {
    pub fn new(settings: Arc<Mutex<Settings>>) -> Self {
        Self { settings }
    }
}

impl SttProvider for GroqSttProvider {
    fn transcribe(
        &self,
        pcm_data: Vec<i16>,
    ) -> Pin<Box<dyn Future<Output = Result<String, AppError>> + Send>> {
        let settings = self.settings.clone();
        Box::pin(async move {
            let s = settings.lock().await;
            let api_key = get_api_key_from_provider(&s.stt_provider)?;
            let config = SttConfig {
                api_key,
                model: s.stt_model.clone(),
                language: s.stt_language.clone(),
                response_format: "verbose_json".to_string(),
            };
            drop(s);
            transcribe::transcribe(&pcm_data, &config).await
        })
    }
}

/// Concrete LlmProvider backed by Groq Chat Completion API.
pub struct GroqLlmProvider {
    settings: Arc<Mutex<Settings>>,
}

impl GroqLlmProvider {
    pub fn new(settings: Arc<Mutex<Settings>>) -> Self {
        Self { settings }
    }
}

impl LlmProvider for GroqLlmProvider {
    fn refine(
        &self,
        text: String,
        language: Language,
    ) -> Pin<Box<dyn Future<Output = Result<String, AppError>> + Send>> {
        let settings = self.settings.clone();
        Box::pin(async move {
            let s = settings.lock().await;
            let api_key = get_api_key_from_provider(&s.refinement_provider)?;
            let config = ChatConfig {
                api_key,
                model: s.refinement_model.clone(),
                temperature: groq::LLM_TEMPERATURE,
                max_tokens: groq::LLM_MAX_TOKENS,
            };
            drop(s);
            refine::refine(&text, &config, &language).await
        })
    }
}

/// Resolve the API key for a given provider.
///
/// In a real implementation this reads from the encrypted Tauri store.
/// For now, reads from the GROQ_API_KEY environment variable as a fallback.
fn get_api_key_from_provider(provider: &str) -> Result<String, AppError> {
    // Try environment variable first (for development)
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
}
