use crate::api::groq::{self, SttConfig};
use crate::audio::encoder;
use crate::error::AppError;

/// Orchestrate PCM → WAV → STT transcription.
///
/// Composes `encoder::pcm_to_wav()` + `groq::transcribe()`.
/// Mirrors Android's `TranscribeAudioUseCase`.
pub async fn transcribe(pcm_data: &[i16], config: &SttConfig) -> Result<String, AppError> {
    if pcm_data.is_empty() {
        return Err(AppError::Audio("no audio data".to_string()));
    }

    let wav_data = encoder::pcm_to_wav(pcm_data);
    groq::transcribe(config, &wav_data).await
}

/// Internal: transcribe with configurable base URL (for testing with wiremock).
#[cfg(test)]
async fn transcribe_with_base_url(
    pcm_data: &[i16],
    config: &SttConfig,
    base_url: &str,
) -> Result<String, AppError> {
    if pcm_data.is_empty() {
        return Err(AppError::Audio("no audio data".to_string()));
    }

    let wav_data = encoder::pcm_to_wav(pcm_data);
    groq::transcribe_with_base_url(config, &wav_data, base_url).await
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
        let result = transcribe(&[], &config).await;

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

        let result =
            transcribe_with_base_url(&pcm_data, &config, &format!("{}/", server.uri())).await;

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

        let result =
            transcribe_with_base_url(&pcm_data, &config, &format!("{}/", server.uri())).await;

        assert!(matches!(result, Err(AppError::ApiKeyMissing(_))));
    }
}
