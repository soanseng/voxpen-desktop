use std::fmt;
use std::future::Future;
use std::pin::Pin;

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
    ) -> Pin<Box<dyn Future<Output = Result<String, AppError>> + Send>>;
}

/// Configuration for the pipeline controller.
#[derive(Clone)]
pub struct PipelineConfig {
    pub groq_api_key: Option<String>,
    pub language: Language,
    pub stt_model: String,
}

// Custom Debug that masks API keys (S-6)
impl fmt::Debug for PipelineConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PipelineConfig")
            .field("groq_api_key", &self.groq_api_key.as_ref().map(|_| "****"))
            .field("language", &self.language)
            .field("stt_model", &self.stt_model)
            .finish()
    }
}

impl PipelineConfig {
    pub fn new(groq_api_key: Option<String>, language: Language) -> Self {
        Self {
            groq_api_key,
            language,
            stt_model: crate::api::groq::DEFAULT_STT_MODEL.to_string(),
        }
    }
}

/// State machine orchestrating the voice dictation pipeline.
///
/// Mirrors Android's `RecordingController`. Manages state transitions
/// and broadcasts state changes via a `tokio::sync::watch` channel.
///
/// Uses the `SttProvider` trait for STT calls, allowing dependency injection
/// and mock-based testing (mirrors Android's Hilt-injected `SttRepository`).
pub struct PipelineController<S: SttProvider> {
    state_tx: watch::Sender<PipelineState>,
    state_rx: watch::Receiver<PipelineState>,
    config: PipelineConfig,
    stt: S,
}

impl<S: SttProvider> PipelineController<S> {
    pub fn new(config: PipelineConfig, stt: S) -> Self {
        let (state_tx, state_rx) = watch::channel(PipelineState::Idle);
        Self {
            state_tx,
            state_rx,
            config,
            stt,
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
    /// Validates that an API key is configured, then transitions to Recording state.
    pub fn on_start_recording(&self) -> Result<(), AppError> {
        if self.config.groq_api_key.is_none() {
            // Send returns Err only when all receivers are dropped; we hold state_rx
            let _ = self.state_tx.send(PipelineState::Error {
                message: "API key not configured for groq".to_string(),
            });
            return Err(AppError::ApiKeyMissing("groq".to_string()));
        }

        let _ = self.state_tx.send(PipelineState::Recording);
        Ok(())
    }

    /// Called when the user releases the hotkey to stop recording.
    ///
    /// Validates current state is Recording (I-3: state guard), then transitions
    /// through Processing → Result (or Error) states.
    /// Returns the transcribed text on success.
    pub async fn on_stop_recording(&self, pcm_data: Vec<i16>) -> Result<String, AppError> {
        // I-3: Guard against calling stop when not recording
        if !matches!(self.current_state(), PipelineState::Recording) {
            return Err(AppError::Audio("not currently recording".to_string()));
        }

        let _ = self.state_tx.send(PipelineState::Processing);

        match self.stt.transcribe(pcm_data).await {
            Ok(text) => {
                let _ = self.state_tx.send(PipelineState::Result { text: text.clone() });
                Ok(text)
            }
            Err(e) => {
                let _ = self.state_tx.send(PipelineState::Error {
                    message: e.to_string(),
                });
                Err(e)
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

    fn mock_stt_success(text: &str) -> MockSttProvider {
        let text = text.to_string();
        let mut mock = MockSttProvider::new();
        mock.expect_transcribe()
            .returning(move |_| Box::pin(std::future::ready(Ok(text.clone()))));
        mock
    }

    fn mock_stt_failure() -> MockSttProvider {
        let mut mock = MockSttProvider::new();
        mock.expect_transcribe().returning(|_| {
            Box::pin(std::future::ready(Err(AppError::Transcription(
                "mock failure".to_string(),
            ))))
        });
        mock
    }

    #[test]
    fn should_start_in_idle_state() {
        let controller = PipelineController::new(config_with_key(), mock_stt_success(""));
        assert_eq!(controller.current_state(), PipelineState::Idle);
    }

    #[test]
    fn should_transition_to_recording_on_start() {
        let controller = PipelineController::new(config_with_key(), mock_stt_success(""));
        let result = controller.on_start_recording();
        assert!(result.is_ok());
        assert_eq!(controller.current_state(), PipelineState::Recording);
    }

    #[test]
    fn should_emit_error_when_api_key_missing() {
        let controller = PipelineController::new(config_without_key(), mock_stt_success(""));
        let result = controller.on_start_recording();

        assert!(matches!(result, Err(AppError::ApiKeyMissing(_))));
        assert!(matches!(
            controller.current_state(),
            PipelineState::Error { .. }
        ));
    }

    #[tokio::test]
    async fn should_transition_recording_to_processing_to_result() {
        let controller =
            PipelineController::new(config_with_key(), mock_stt_success("你好世界"));
        controller.on_start_recording().unwrap();

        let result = controller.on_stop_recording(vec![100, 200]).await;

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
        let controller = PipelineController::new(config_with_key(), mock_stt_failure());
        controller.on_start_recording().unwrap();

        let result = controller.on_stop_recording(vec![100, 200]).await;

        assert!(matches!(result, Err(AppError::Transcription(_))));
        assert!(matches!(
            controller.current_state(),
            PipelineState::Error { .. }
        ));
    }

    #[tokio::test]
    async fn should_reject_stop_when_not_recording() {
        let controller = PipelineController::new(config_with_key(), mock_stt_success(""));
        // Don't call on_start_recording — state is Idle

        let result = controller.on_stop_recording(vec![100]).await;

        match result {
            Err(AppError::Audio(msg)) => assert_eq!(msg, "not currently recording"),
            other => panic!("expected Audio error, got {:?}", other),
        }
    }

    #[test]
    fn should_reset_to_idle() {
        let controller = PipelineController::new(config_with_key(), mock_stt_success(""));
        controller.on_start_recording().unwrap();
        assert_eq!(controller.current_state(), PipelineState::Recording);

        controller.reset();
        assert_eq!(controller.current_state(), PipelineState::Idle);
    }

    #[test]
    fn should_allow_config_update() {
        let mut controller = PipelineController::new(config_without_key(), mock_stt_success(""));

        assert!(controller.on_start_recording().is_err());

        controller.update_config(config_with_key());
        assert!(controller.on_start_recording().is_ok());
    }

    #[test]
    fn should_provide_subscriber() {
        let controller = PipelineController::new(config_with_key(), mock_stt_success(""));
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
}
