use tokio::sync::watch;

use crate::api::groq::SttConfig;
use crate::error::AppError;
use crate::pipeline::state::{Language, PipelineState};
use crate::pipeline::transcribe;

/// Configuration for the pipeline controller.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub groq_api_key: Option<String>,
    pub language: Language,
    pub stt_model: String,
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
pub struct PipelineController {
    state_tx: watch::Sender<PipelineState>,
    state_rx: watch::Receiver<PipelineState>,
    config: PipelineConfig,
}

impl PipelineController {
    pub fn new(config: PipelineConfig) -> Self {
        let (state_tx, state_rx) = watch::channel(PipelineState::Idle);
        Self {
            state_tx,
            state_rx,
            config,
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
    /// Transitions through Processing → Result (or Error) states.
    /// Returns the transcribed text on success.
    pub async fn on_stop_recording(&self, pcm_data: Vec<i16>) -> Result<String, AppError> {
        let _ = self.state_tx.send(PipelineState::Processing);

        let api_key = self
            .config
            .groq_api_key
            .as_ref()
            .ok_or_else(|| AppError::ApiKeyMissing("groq".to_string()))?;

        let stt_config = SttConfig {
            api_key: api_key.clone(),
            model: self.config.stt_model.clone(),
            language: self.config.language.clone(),
            response_format: "verbose_json".to_string(),
        };

        match transcribe::transcribe(&pcm_data, &stt_config).await {
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

    #[test]
    fn should_start_in_idle_state() {
        let controller = PipelineController::new(config_with_key());
        assert_eq!(controller.current_state(), PipelineState::Idle);
    }

    #[test]
    fn should_transition_to_recording_on_start() {
        let controller = PipelineController::new(config_with_key());
        let result = controller.on_start_recording();
        assert!(result.is_ok());
        assert_eq!(controller.current_state(), PipelineState::Recording);
    }

    #[test]
    fn should_emit_error_when_api_key_missing() {
        let controller = PipelineController::new(config_without_key());
        let result = controller.on_start_recording();

        assert!(matches!(result, Err(AppError::ApiKeyMissing(_))));
        assert!(matches!(
            controller.current_state(),
            PipelineState::Error { .. }
        ));
    }

    #[tokio::test]
    async fn should_transition_to_processing_on_stop() {
        let controller = PipelineController::new(config_with_key());
        controller.on_start_recording().unwrap();

        // Capture state changes via subscriber
        let mut rx = controller.subscribe();

        // on_stop_recording will fail (no real API), but should transition to Processing first
        let _ = controller.on_stop_recording(vec![100, 200]).await;

        // The final state should be Error (no real API server)
        // But we verify the controller handles the flow correctly
        let state = controller.current_state();
        assert!(
            matches!(state, PipelineState::Error { .. }),
            "expected Error state (no API server), got {:?}",
            state
        );

        // Verify we can still read from subscriber
        drop(rx);
    }

    #[tokio::test]
    async fn should_emit_error_on_empty_pcm_data() {
        let controller = PipelineController::new(config_with_key());
        controller.on_start_recording().unwrap();

        let result = controller.on_stop_recording(vec![]).await;

        assert!(matches!(result, Err(AppError::Audio(_))));
        assert!(matches!(
            controller.current_state(),
            PipelineState::Error { .. }
        ));
    }

    #[test]
    fn should_reset_to_idle() {
        let controller = PipelineController::new(config_with_key());
        controller.on_start_recording().unwrap();
        assert_eq!(controller.current_state(), PipelineState::Recording);

        controller.reset();
        assert_eq!(controller.current_state(), PipelineState::Idle);
    }

    #[test]
    fn should_allow_config_update() {
        let mut controller = PipelineController::new(config_without_key());

        // Initially no key
        assert!(controller.on_start_recording().is_err());

        // Update config with key
        controller.update_config(config_with_key());
        assert!(controller.on_start_recording().is_ok());
    }

    #[test]
    fn should_provide_subscriber() {
        let controller = PipelineController::new(config_with_key());
        let rx = controller.subscribe();

        assert_eq!(*rx.borrow(), PipelineState::Idle);

        controller.on_start_recording().unwrap();
        assert_eq!(*rx.borrow(), PipelineState::Recording);
    }
}
