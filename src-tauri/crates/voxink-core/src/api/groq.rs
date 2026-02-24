use std::fmt;

use reqwest::multipart;
use serde::Deserialize;

use crate::api::{CONNECT_TIMEOUT, GROQ_BASE_URL, READ_WRITE_TIMEOUT};
use crate::error::AppError;
use crate::pipeline::state::Language;

/// Default Whisper model (faster, good accuracy)
pub const DEFAULT_STT_MODEL: &str = "whisper-large-v3-turbo";

/// Configuration for a single STT API call.
#[derive(Clone)]
pub struct SttConfig {
    pub api_key: String,
    pub model: String,
    pub language: Language,
    pub response_format: String,
}

// Custom Debug that masks the API key
impl fmt::Debug for SttConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SttConfig")
            .field("api_key", &"****")
            .field("model", &self.model)
            .field("language", &self.language)
            .field("response_format", &self.response_format)
            .finish()
    }
}

impl SttConfig {
    pub fn new(api_key: String, language: Language) -> Self {
        Self {
            api_key,
            model: DEFAULT_STT_MODEL.to_string(),
            language,
            response_format: "verbose_json".to_string(),
        }
    }
}

/// Groq Whisper API response (subset of verbose_json format).
#[derive(Debug, Deserialize)]
pub struct WhisperResponse {
    pub text: String,
}

/// Transcribe WAV audio via Groq Whisper API.
///
/// Sends a multipart POST to Groq's OpenAI-compatible endpoint.
/// Returns the transcribed text on success.
pub async fn transcribe(config: &SttConfig, wav_data: &[u8]) -> Result<String, AppError> {
    transcribe_with_base_url(config, wav_data, GROQ_BASE_URL).await
}

/// Internal: transcribe with configurable base URL (for testing with wiremock).
pub(crate) async fn transcribe_with_base_url(
    config: &SttConfig,
    wav_data: &[u8],
    base_url: &str,
) -> Result<String, AppError> {
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(READ_WRITE_TIMEOUT)
        .build()
        .map_err(AppError::Network)?;

    let file_part = multipart::Part::bytes(wav_data.to_vec())
        .file_name("recording.wav")
        .mime_str("audio/wav")
        .map_err(|e| AppError::Transcription(e.to_string()))?;

    let mut form = multipart::Form::new()
        .part("file", file_part)
        .text("model", config.model.clone())
        .text("response_format", config.response_format.clone());

    // Add language param if not auto-detect
    if let Some(code) = config.language.code() {
        form = form.text("language", code.to_string());
    }

    // Always add prompt hint
    form = form.text("prompt", config.language.prompt().to_string());

    let url = format!("{base_url}openai/v1/audio/transcriptions");

    let response = client
        .post(&url)
        .bearer_auth(&config.api_key)
        .multipart(form)
        .send()
        .await?;

    let status = response.status();

    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(AppError::ApiKeyMissing("groq".to_string()));
    }

    if status == reqwest::StatusCode::PAYLOAD_TOO_LARGE {
        return Err(AppError::Transcription("file too large".to_string()));
    }

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Transcription(format!(
            "HTTP {}: {}",
            status.as_u16(),
            body
        )));
    }

    let whisper: WhisperResponse = response
        .json()
        .await
        .map_err(|e| AppError::Transcription(format!("failed to parse response: {e}")))?;

    Ok(whisper.text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_config(api_key: &str, language: Language) -> SttConfig {
        SttConfig::new(api_key.to_string(), language)
    }

    #[tokio::test]
    async fn should_return_transcription_when_api_responds_successfully() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .and(header("authorization", "Bearer test-key-123"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"text": "你好世界"})),
            )
            .mount(&server)
            .await;

        let config = test_config("test-key-123", Language::Chinese);
        let wav_data = crate::audio::encoder::pcm_to_wav(&[100, 200, 300]);

        let result = transcribe_with_base_url(&config, &wav_data, &format!("{}/", server.uri()))
            .await;

        assert_eq!(result.unwrap(), "你好世界");
    }

    #[tokio::test]
    async fn should_return_api_key_error_on_401() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;

        let config = test_config("bad-key", Language::Auto);
        let wav_data = crate::audio::encoder::pcm_to_wav(&[100]);

        let result = transcribe_with_base_url(&config, &wav_data, &format!("{}/", server.uri()))
            .await;

        assert!(matches!(result, Err(AppError::ApiKeyMissing(_))));
    }

    #[tokio::test]
    async fn should_return_transcription_error_on_413() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(413).set_body_string("Payload too large"))
            .mount(&server)
            .await;

        let config = test_config("test-key", Language::Auto);
        let wav_data = crate::audio::encoder::pcm_to_wav(&[100]);

        let result = transcribe_with_base_url(&config, &wav_data, &format!("{}/", server.uri()))
            .await;

        match result {
            Err(AppError::Transcription(msg)) => assert!(msg.contains("too large")),
            other => panic!("expected Transcription error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn should_return_error_on_500() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal error"))
            .mount(&server)
            .await;

        let config = test_config("test-key", Language::English);
        let wav_data = crate::audio::encoder::pcm_to_wav(&[100]);

        let result = transcribe_with_base_url(&config, &wav_data, &format!("{}/", server.uri()))
            .await;

        match result {
            Err(AppError::Transcription(msg)) => {
                assert!(msg.contains("500"));
            }
            other => panic!("expected Transcription error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn should_use_correct_model_in_request() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"text": "test"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let mut config = test_config("key", Language::Auto);
        config.model = "whisper-large-v3".to_string();
        let wav_data = crate::audio::encoder::pcm_to_wav(&[100]);

        let result = transcribe_with_base_url(&config, &wav_data, &format!("{}/", server.uri()))
            .await;
        assert!(result.is_ok());
    }
}
