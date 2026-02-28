use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use tokio::sync::watch;

use crate::error::AppError;
use crate::pipeline::state::{Language, PipelineState};

/// Trait abstracting the STT (speech-to-text) provider.
///
/// Allows the controller to be tested without network calls and
/// supports multiple providers (Groq, OpenAI, custom).
/// Takes owned Vec<i16> to avoid lifetime issues with boxed futures and mockall.
#[cfg_attr(test, mockall::automock)]
pub trait SttProvider: Send + Sync {
    /// Transcribe PCM audio data to text.
    fn transcribe(
        &self,
        pcm_data: Vec<i16>,
        vocabulary_hint: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, AppError>> + Send>>;
}

/// Trait abstracting the LLM refinement provider.
///
/// Allows the controller to be tested without network calls and
/// supports multiple providers (Groq, OpenAI, Anthropic, custom).
#[cfg_attr(test, mockall::automock)]
pub trait LlmProvider: Send + Sync {
    /// Refine transcribed text via LLM.
    fn refine(
        &self,
        text: String,
        language: Language,
        vocabulary: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, AppError>> + Send>>;
}

/// Timeout for LLM refinement — if exceeded, fall back to raw text.
const REFINEMENT_TIMEOUT: Duration = Duration::from_secs(5);

/// Configuration for the pipeline controller.
#[derive(Clone)]
pub struct PipelineConfig {
    pub groq_api_key: Option<String>,
    pub language: Language,
    pub stt_model: String,
    pub refinement_enabled: bool,
    pub llm_api_key: Option<String>,
    pub llm_model: String,
    pub voice_commands_enabled: bool,
}

// Custom Debug that masks API keys (S-6)
impl fmt::Debug for PipelineConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PipelineConfig")
            .field("groq_api_key", &self.groq_api_key.as_ref().map(|_| "****"))
            .field("language", &self.language)
            .field("stt_model", &self.stt_model)
            .field("refinement_enabled", &self.refinement_enabled)
            .field("llm_api_key", &self.llm_api_key.as_ref().map(|_| "****"))
            .field("llm_model", &self.llm_model)
            .field("voice_commands_enabled", &self.voice_commands_enabled)
            .finish()
    }
}

impl PipelineConfig {
    pub fn new(groq_api_key: Option<String>, language: Language) -> Self {
        Self {
            groq_api_key,
            language,
            stt_model: crate::api::groq::DEFAULT_STT_MODEL.to_string(),
            refinement_enabled: false,
            llm_api_key: None,
            llm_model: crate::api::groq::DEFAULT_LLM_MODEL.to_string(),
            voice_commands_enabled: false,
        }
    }
}

/// State machine orchestrating the voice dictation pipeline.
///
/// Mirrors Android's `RecordingController`. Manages state transitions
/// and broadcasts state changes via a `tokio::sync::watch` channel.
///
/// Uses `SttProvider` and `LlmProvider` traits for dependency injection
/// and mock-based testing (mirrors Android's Hilt-injected repositories).
pub struct PipelineController<S: SttProvider, L: LlmProvider> {
    state_tx: watch::Sender<PipelineState>,
    state_rx: watch::Receiver<PipelineState>,
    config: PipelineConfig,
    stt: S,
    llm: L,
}

impl<S: SttProvider, L: LlmProvider> PipelineController<S, L> {
    pub fn new(config: PipelineConfig, stt: S, llm: L) -> Self {
        let (state_tx, state_rx) = watch::channel(PipelineState::Idle);
        Self {
            state_tx,
            state_rx,
            config,
            stt,
            llm,
        }
    }

    /// Get the current pipeline state.
    pub fn current_state(&self) -> PipelineState {
        self.state_rx.borrow().clone()
    }

    /// Subscribe to state changes. Returns a receiver that gets notified on each state transition.
    pub fn subscribe(&self) -> watch::Receiver<PipelineState> {
        self.state_rx.clone()
    }

    /// Update the pipeline configuration (e.g., when user changes settings).
    pub fn update_config(&mut self, config: PipelineConfig) {
        self.config = config;
    }

    /// Called when the user presses the hotkey to start recording.
    ///
    /// Transitions to Recording state. API key validation is deferred
    /// to the provider's `transcribe()` call, which reads the key
    /// directly from the encrypted store at call time.
    pub fn on_start_recording(&self) -> Result<(), AppError> {
        let _ = self.state_tx.send(PipelineState::Recording);
        Ok(())
    }

    /// Called when the user releases the hotkey to stop recording.
    ///
    /// Validates current state is Recording (state guard), then transitions through:
    /// - Processing → STT → Result (if refinement disabled)
    /// - Processing → STT → Refining → Refined (if refinement enabled and succeeds)
    /// - Processing → STT → Result (if refinement enabled but fails — graceful fallback)
    ///
    /// LLM refinement has a 5s timeout. If exceeded, falls back to raw text.
    pub async fn on_stop_recording(
        &self,
        pcm_data: Vec<i16>,
        vocabulary_hint: Option<String>,
        vocabulary_words: Vec<String>,
    ) -> Result<String, AppError> {
        // Guard against calling stop when not recording
        if !matches!(self.current_state(), PipelineState::Recording) {
            return Err(AppError::Audio("not currently recording".to_string()));
        }

        let _ = self.state_tx.send(PipelineState::Processing);

        let raw_text = match self.stt.transcribe(pcm_data, vocabulary_hint).await {
            Ok(text) => text,
            Err(e) => {
                let _ = self.state_tx.send(PipelineState::Error {
                    message: e.to_string(),
                });
                return Err(e);
            }
        };

        // If refinement is disabled, emit Result and return
        if !self.config.refinement_enabled {
            let _ = self.state_tx.send(PipelineState::Result {
                text: raw_text.clone(),
            });
            return Ok(raw_text);
        }

        // Refinement enabled — transition to Refining
        let _ = self.state_tx.send(PipelineState::Refining {
            original: raw_text.clone(),
        });

        // Attempt refinement with timeout
        let refine_result = tokio::time::timeout(
            REFINEMENT_TIMEOUT,
            self.llm
                .refine(raw_text.clone(), self.config.language.clone(), vocabulary_words),
        )
        .await;

        match refine_result {
            // Refinement succeeded within timeout
            Ok(Ok(refined_text)) => {
                let _ = self.state_tx.send(PipelineState::Refined {
                    original: raw_text,
                    refined: refined_text.clone(),
                });
                Ok(refined_text)
            }
            // Refinement failed or timed out — graceful fallback to raw text
            Ok(Err(_)) | Err(_) => {
                let _ = self.state_tx.send(PipelineState::Result {
                    text: raw_text.clone(),
                });
                Ok(raw_text)
            }
        }
    }

    /// Reset the pipeline to Idle state.
    pub fn reset(&self) {
        let _ = self.state_tx.send(PipelineState::Idle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_key() -> PipelineConfig {
        PipelineConfig::new(Some("test-api-key".to_string()), Language::Auto)
    }

    fn config_without_key() -> PipelineConfig {
        PipelineConfig::new(None, Language::Auto)
    }

    fn config_with_refinement() -> PipelineConfig {
        let mut config = config_with_key();
        config.refinement_enabled = true;
        config.llm_api_key = Some("llm-key".to_string());
        config
    }

    fn mock_stt_success(text: &str) -> MockSttProvider {
        let text = text.to_string();
        let mut mock = MockSttProvider::new();
        mock.expect_transcribe()
            .returning(move |_, _| Box::pin(std::future::ready(Ok(text.clone()))));
        mock
    }

    fn mock_stt_failure() -> MockSttProvider {
        let mut mock = MockSttProvider::new();
        mock.expect_transcribe().returning(|_, _| {
            Box::pin(std::future::ready(Err(AppError::Transcription(
                "mock failure".to_string(),
            ))))
        });
        mock
    }

    fn mock_llm_success(text: &str) -> MockLlmProvider {
        let text = text.to_string();
        let mut mock = MockLlmProvider::new();
        mock.expect_refine()
            .returning(move |_, _, _| Box::pin(std::future::ready(Ok(text.clone()))));
        mock
    }

    fn mock_llm_failure() -> MockLlmProvider {
        let mut mock = MockLlmProvider::new();
        mock.expect_refine().returning(|_, _, _| {
            Box::pin(std::future::ready(Err(AppError::Refinement(
                "mock failure".to_string(),
            ))))
        });
        mock
    }

    fn mock_llm_timeout() -> MockLlmProvider {
        let mut mock = MockLlmProvider::new();
        mock.expect_refine().returning(|_, _, _| {
            Box::pin(async {
                tokio::time::sleep(Duration::from_secs(10)).await;
                Ok("should not reach".to_string())
            })
        });
        mock
    }

    fn mock_llm_unused() -> MockLlmProvider {
        MockLlmProvider::new()
    }

    #[test]
    fn should_start_in_idle_state() {
        let controller =
            PipelineController::new(config_with_key(), mock_stt_success(""), mock_llm_unused());
        assert_eq!(controller.current_state(), PipelineState::Idle);
    }

    #[test]
    fn should_transition_to_recording_on_start() {
        let controller =
            PipelineController::new(config_with_key(), mock_stt_success(""), mock_llm_unused());
        let result = controller.on_start_recording();
        assert!(result.is_ok());
        assert_eq!(controller.current_state(), PipelineState::Recording);
    }

    #[test]
    fn should_start_recording_even_without_config_key() {
        // API key validation is deferred to the provider's transcribe() call
        let controller = PipelineController::new(
            config_without_key(),
            mock_stt_success(""),
            mock_llm_unused(),
        );
        let result = controller.on_start_recording();

        assert!(result.is_ok());
        assert_eq!(controller.current_state(), PipelineState::Recording);
    }

    #[tokio::test]
    async fn should_transition_recording_to_processing_to_result() {
        let controller = PipelineController::new(
            config_with_key(),
            mock_stt_success("你好世界"),
            mock_llm_unused(),
        );
        controller.on_start_recording().unwrap();

        let result = controller.on_stop_recording(vec![100, 200], None, vec![]).await;

        assert_eq!(result.unwrap(), "你好世界");
        assert_eq!(
            controller.current_state(),
            PipelineState::Result {
                text: "你好世界".to_string()
            }
        );
    }

    #[tokio::test]
    async fn should_emit_error_state_on_transcription_failure() {
        let controller = PipelineController::new(
            config_with_key(),
            mock_stt_failure(),
            mock_llm_unused(),
        );
        controller.on_start_recording().unwrap();

        let result = controller.on_stop_recording(vec![100, 200], None, vec![]).await;

        assert!(matches!(result, Err(AppError::Transcription(_))));
        assert!(matches!(
            controller.current_state(),
            PipelineState::Error { .. }
        ));
    }

    #[tokio::test]
    async fn should_reject_stop_when_not_recording() {
        let controller = PipelineController::new(
            config_with_key(),
            mock_stt_success(""),
            mock_llm_unused(),
        );

        let result = controller.on_stop_recording(vec![100], None, vec![]).await;

        match result {
            Err(AppError::Audio(msg)) => assert_eq!(msg, "not currently recording"),
            other => panic!("expected Audio error, got {:?}", other),
        }
    }

    #[test]
    fn should_reset_to_idle() {
        let controller =
            PipelineController::new(config_with_key(), mock_stt_success(""), mock_llm_unused());
        controller.on_start_recording().unwrap();
        assert_eq!(controller.current_state(), PipelineState::Recording);

        controller.reset();
        assert_eq!(controller.current_state(), PipelineState::Idle);
    }

    #[test]
    fn should_allow_config_update() {
        let mut controller = PipelineController::new(
            config_without_key(),
            mock_stt_success(""),
            mock_llm_unused(),
        );

        // Config starts without refinement
        assert!(!controller.config.refinement_enabled);

        controller.update_config(config_with_refinement());
        assert!(controller.config.refinement_enabled);
    }

    #[test]
    fn should_provide_subscriber() {
        let controller =
            PipelineController::new(config_with_key(), mock_stt_success(""), mock_llm_unused());
        let rx = controller.subscribe();

        assert_eq!(*rx.borrow(), PipelineState::Idle);

        controller.on_start_recording().unwrap();
        assert_eq!(*rx.borrow(), PipelineState::Recording);
    }

    #[test]
    fn should_mask_api_key_in_debug() {
        let config = config_with_key();
        let debug = format!("{:?}", config);
        assert!(!debug.contains("test-api-key"));
        assert!(debug.contains("****"));
    }

    // -- Refinement tests --

    #[tokio::test]
    async fn should_transition_through_refining_to_refined_when_enabled() {
        let controller = PipelineController::new(
            config_with_refinement(),
            mock_stt_success("嗯 那個 你好世界"),
            mock_llm_success("你好世界！"),
        );
        controller.on_start_recording().unwrap();

        let result = controller.on_stop_recording(vec![100, 200], None, vec![]).await;

        assert_eq!(result.unwrap(), "你好世界！");
        assert_eq!(
            controller.current_state(),
            PipelineState::Refined {
                original: "嗯 那個 你好世界".to_string(),
                refined: "你好世界！".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn should_fallback_to_raw_text_when_refinement_fails() {
        let controller = PipelineController::new(
            config_with_refinement(),
            mock_stt_success("raw text"),
            mock_llm_failure(),
        );
        controller.on_start_recording().unwrap();

        let result = controller.on_stop_recording(vec![100, 200], None, vec![]).await;

        // Should return raw text as fallback, not an error
        assert_eq!(result.unwrap(), "raw text");
        assert_eq!(
            controller.current_state(),
            PipelineState::Result {
                text: "raw text".to_string()
            }
        );
    }

    #[tokio::test]
    async fn should_skip_refinement_when_disabled() {
        let controller = PipelineController::new(
            config_with_key(), // refinement_enabled = false by default
            mock_stt_success("hello world"),
            mock_llm_unused(), // LLM should never be called
        );
        controller.on_start_recording().unwrap();

        let result = controller.on_stop_recording(vec![100, 200], None, vec![]).await;

        assert_eq!(result.unwrap(), "hello world");
        assert_eq!(
            controller.current_state(),
            PipelineState::Result {
                text: "hello world".to_string()
            }
        );
    }

    #[tokio::test]
    async fn should_timeout_refinement_and_fallback() {
        let controller = PipelineController::new(
            config_with_refinement(),
            mock_stt_success("raw text"),
            mock_llm_timeout(),
        );
        controller.on_start_recording().unwrap();

        let result = controller.on_stop_recording(vec![100, 200], None, vec![]).await;

        // Should fallback to raw text after timeout
        assert_eq!(result.unwrap(), "raw text");
        assert_eq!(
            controller.current_state(),
            PipelineState::Result {
                text: "raw text".to_string()
            }
        );
    }

    #[test]
    fn should_mask_llm_api_key_in_debug() {
        let config = config_with_refinement();
        let debug = format!("{:?}", config);
        assert!(!debug.contains("llm-key"));
        assert!(debug.contains("****"));
    }
}
