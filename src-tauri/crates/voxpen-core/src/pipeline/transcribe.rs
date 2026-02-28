use crate::api::groq::{self, SttConfig};
use crate::audio::encoder;
use crate::error::AppError;

/// Orchestrate PCM → WAV → STT transcription.
///
/// Composes `encoder::pcm_to_wav()` + `groq::transcribe_with_base_url()`.
/// Mirrors Android's `TranscribeAudioUseCase`.
///
/// If `vocabulary_hint` is provided, it overrides the default language prompt
/// sent to Whisper, allowing custom vocabulary words to bias transcription.
pub async fn transcribe(
    pcm_data: &[i16],
    config: &SttConfig,
    vocabulary_hint: Option<&str>,
    provider: &str,
    base_url: &str,
) -> Result<String, AppError> {
    if pcm_data.is_empty() {
        return Err(AppError::Audio("no audio data".to_string()));
    }

    let mut config = config.clone();
    if let Some(hint) = vocabulary_hint {
        config.prompt_override = Some(hint.to_string());
    }

    let wav_data = encoder::pcm_to_wav(pcm_data);
    groq::transcribe_with_base_url(&config, &wav_data, provider, base_url).await
}

/// Internal: transcribe with configurable base URL (for testing with wiremock).
#[cfg(test)]
async fn transcribe_with_base_url(
    pcm_data: &[i16],
    config: &SttConfig,
    provider: &str,
    base_url: &str,
) -> Result<String, AppError> {
    if pcm_data.is_empty() {
        return Err(AppError::Audio("no audio data".to_string()));
    }

    let wav_data = encoder::pcm_to_wav(pcm_data);
    groq::transcribe_with_base_url(config, &wav_data, provider, base_url).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::state::Language;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn should_reject_empty_pcm_data() {
        let config = SttConfig::new("key".to_string(), Language::Auto);
        let result = transcribe(&[], &config, None, "groq", "https://api.groq.com/").await;

        match result {
            Err(AppError::Audio(msg)) => assert_eq!(msg, "no audio data"),
            other => panic!("expected Audio error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn should_encode_pcm_and_call_stt_api() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"text": "hello world"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let config = SttConfig::new("test-key".to_string(), Language::English);
        let pcm_data = vec![100i16, 200, 300, -100, -200];

        let result = transcribe_with_base_url(
            &pcm_data,
            &config,
            "groq",
            &format!("{}/", server.uri()),
        )
        .await;

        assert_eq!(result.unwrap(), "hello world");
    }

    #[tokio::test]
    async fn should_propagate_api_errors() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let config = SttConfig::new("bad-key".to_string(), Language::Chinese);
        let pcm_data = vec![100i16];

        let result = transcribe_with_base_url(
            &pcm_data,
            &config,
            "groq",
            &format!("{}/", server.uri()),
        )
        .await;

        assert!(matches!(result, Err(AppError::ApiKeyMissing(_))));
    }
}
