use crate::api::groq::{self, SttConfig};
use crate::audio::encoder;
use crate::error::AppError;

const LIVE_STT_SAMPLE_RATE: usize = 16_000;
const LIVE_STT_CHUNK_SECONDS: usize = 60;
const LIVE_STT_CHUNK_SAMPLES: usize = LIVE_STT_SAMPLE_RATE * LIVE_STT_CHUNK_SECONDS;

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

    transcribe_pcm_chunks(pcm_data, &config, provider, base_url).await
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

    transcribe_pcm_chunks(pcm_data, config, provider, base_url).await
}

async fn transcribe_pcm_chunks(
    pcm_data: &[i16],
    config: &SttConfig,
    provider: &str,
    base_url: &str,
) -> Result<String, AppError> {
    let mut texts = Vec::new();
    for chunk in split_pcm_for_stt(pcm_data) {
        let wav_data = encoder::pcm_to_wav(chunk);
        let text = groq::transcribe_with_base_url(config, &wav_data, provider, base_url).await?;
        texts.push(text);
    }
    Ok(join_chunk_texts(texts))
}

fn split_pcm_for_stt(pcm_data: &[i16]) -> Vec<&[i16]> {
    pcm_data.chunks(LIVE_STT_CHUNK_SAMPLES).collect()
}

fn join_chunk_texts(texts: Vec<String>) -> String {
    texts
        .into_iter()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
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

        let result =
            transcribe_with_base_url(&pcm_data, &config, "groq", &format!("{}/", server.uri()))
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

        let result =
            transcribe_with_base_url(&pcm_data, &config, "groq", &format!("{}/", server.uri()))
                .await;

        assert!(matches!(result, Err(AppError::ApiKeyMissing(_))));
    }

    #[test]
    fn should_split_live_pcm_into_sixty_second_chunks() {
        let pcm_data = vec![1i16; LIVE_STT_CHUNK_SAMPLES + 42];
        let chunks = split_pcm_for_stt(&pcm_data);

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), LIVE_STT_CHUNK_SAMPLES);
        assert_eq!(chunks[1].len(), 42);
    }

    #[test]
    fn should_join_non_empty_chunk_texts_with_spaces() {
        let text = join_chunk_texts(vec![
            " first chunk ".to_string(),
            String::new(),
            "second chunk".to_string(),
        ]);

        assert_eq!(text, "first chunk second chunk");
    }

    #[tokio::test]
    async fn should_transcribe_each_live_chunk_sequentially() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/audio/transcriptions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"text": "chunk text"})),
            )
            .expect(2)
            .mount(&server)
            .await;

        let config = SttConfig::new("test-key".to_string(), Language::English);
        let pcm_data = vec![100i16; LIVE_STT_CHUNK_SAMPLES + 1];

        let result =
            transcribe_with_base_url(&pcm_data, &config, "openai", &format!("{}/", server.uri()))
                .await;

        assert_eq!(result.unwrap(), "chunk text chunk text");
    }
}
