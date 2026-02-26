use serde::Serialize;

/// Application-wide error type, matching Android's error categories.
/// Uses `thiserror` for ergonomic `Display` + `Error` impls.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("API key not configured for {0}")]
    ApiKeyMissing(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Transcription failed: {0}")]
    Transcription(String),

    #[error("Refinement failed: {0}")]
    Refinement(String),

    #[error("Audio error: {0}")]
    Audio(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Hotkey error: {0}")]
    Hotkey(String),

    #[error("Paste error: {0}")]
    Paste(String),

    #[error("License error: {0}")]
    License(String),

    #[error("Daily usage limit reached")]
    UsageLimitReached,

    #[error("Model not downloaded: {0}")]
    ModelNotDownloaded(String),

    #[error("Model download failed: {0}")]
    ModelDownload(String),

    #[error("Local transcription failed: {0}")]
    LocalTranscription(String),
}

// Tauri commands require errors to be Serialize
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_display_api_key_missing_error() {
        let err = AppError::ApiKeyMissing("groq".to_string());
        assert_eq!(err.to_string(), "API key not configured for groq");
    }

    #[test]
    fn should_display_transcription_error() {
        let err = AppError::Transcription("rate limited".to_string());
        assert_eq!(err.to_string(), "Transcription failed: rate limited");
    }

    #[test]
    fn should_display_refinement_error() {
        let err = AppError::Refinement("timeout".to_string());
        assert_eq!(err.to_string(), "Refinement failed: timeout");
    }

    #[test]
    fn should_display_audio_error() {
        let err = AppError::Audio("no microphone".to_string());
        assert_eq!(err.to_string(), "Audio error: no microphone");
    }

    #[test]
    fn should_display_license_error() {
        let err = AppError::License("key revoked".to_string());
        assert_eq!(err.to_string(), "License error: key revoked");
    }

    #[test]
    fn should_display_usage_limit_error() {
        let err = AppError::UsageLimitReached;
        assert_eq!(err.to_string(), "Daily usage limit reached");
    }

    #[test]
    fn should_serialize_error_as_string() {
        let err = AppError::ApiKeyMissing("groq".to_string());
        let json = serde_json::to_string(&err).unwrap();
        assert_eq!(json, "\"API key not configured for groq\"");
    }

    #[test]
    fn should_display_model_not_downloaded_error() {
        let err = AppError::ModelNotDownloaded("ggml-small-q5_1.bin".to_string());
        assert_eq!(
            err.to_string(),
            "Model not downloaded: ggml-small-q5_1.bin"
        );
    }

    #[test]
    fn should_display_model_download_error() {
        let err = AppError::ModelDownload("connection refused".to_string());
        assert_eq!(
            err.to_string(),
            "Model download failed: connection refused"
        );
    }

    #[test]
    fn should_display_local_transcription_error() {
        let err = AppError::LocalTranscription("model load failed".to_string());
        assert_eq!(
            err.to_string(),
            "Local transcription failed: model load failed"
        );
    }
}
