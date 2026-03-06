use std::fmt;

use reqwest::multipart;
use serde::{Deserialize, Serialize};

use crate::api::{CONNECT_TIMEOUT, GROQ_BASE_URL, OPENAI_BASE_URL, OPENROUTER_BASE_URL, READ_WRITE_TIMEOUT};
use crate::error::AppError;
use crate::pipeline::state::Language;

/// Resolve the API base URL for a given provider name.
///
/// Returns the known base URL for built-in providers, or the provider string
/// itself if it's unrecognized (for custom/Ollama setups where the provider
/// string IS the base URL).
pub fn base_url_for_provider(provider: &str) -> &str {
    match provider {
        "groq" => GROQ_BASE_URL,
        "openai" => OPENAI_BASE_URL,
        "openrouter" => OPENROUTER_BASE_URL,
        other => other,
    }
}

/// Default Whisper model (faster, good accuracy)
pub const DEFAULT_STT_MODEL: &str = "whisper-large-v3-turbo";

/// Default LLM model for text refinement
pub const DEFAULT_LLM_MODEL: &str = "openai/gpt-oss-120b";
/// Alternative smaller LLM model
pub const ALT_LLM_MODEL: &str = "openai/gpt-oss-20b";
/// LLM temperature (low for consistent refinement)
pub const LLM_TEMPERATURE: f32 = 0.3;
/// LLM max tokens
pub const LLM_MAX_TOKENS: u32 = 2048;

/// Configuration for a single STT API call.
#[derive(Clone)]
pub struct SttConfig {
    pub api_key: String,
    pub model: String,
    pub language: Language,
    pub response_format: String,
    /// If set, overrides `language.prompt()` for the Whisper `prompt` parameter.
    /// Used to inject vocabulary hints.
    pub prompt_override: Option<String>,
}

// Custom Debug that masks the API key
impl fmt::Debug for SttConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SttConfig")
            .field("api_key", &"****")
            .field("model", &self.model)
            .field("language", &self.language)
            .field("response_format", &self.response_format)
            .field("prompt_override", &self.prompt_override)
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
            prompt_override: None,
        }
    }
}

/// A single segment from Whisper's verbose_json response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// Full Whisper verbose_json response with segments.
#[derive(Debug, Deserialize)]
pub struct WhisperVerboseResponse {
    pub text: String,
    #[serde(default)]
    pub segments: Vec<WhisperSegment>,
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
    transcribe_with_base_url(config, wav_data, "groq", GROQ_BASE_URL).await
}

/// Transcribe an audio file (any format the API supports: wav, mp3, flac, m4a, ogg).
///
/// Sends the raw file bytes with the correct filename and MIME type.
/// For files > 25MB, callers should chunk WAV data first via `audio::chunker`.
pub async fn transcribe_file(
    config: &SttConfig,
    file_data: &[u8],
    filename: &str,
    mime_type: &str,
    provider: &str,
) -> Result<String, AppError> {
    let base_url = base_url_for_provider(provider);
    transcribe_file_with_base_url(config, file_data, filename, mime_type, provider, base_url).await
}

/// Internal: transcribe file with configurable base URL (for testing).
pub(crate) async fn transcribe_file_with_base_url(
    config: &SttConfig,
    file_data: &[u8],
    filename: &str,
    mime_type: &str,
    provider: &str,
    base_url: &str,
) -> Result<String, AppError> {
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(std::time::Duration::from_secs(300)) // 5 min for large files
        .build()
        .map_err(AppError::Network)?;

    let file_part = multipart::Part::bytes(file_data.to_vec())
        .file_name(filename.to_string())
        .mime_str(mime_type)
        .map_err(|e| AppError::Transcription(e.to_string()))?;

    let mut form = multipart::Form::new()
        .part("file", file_part)
        .text("model", config.model.clone())
        .text("response_format", config.response_format.clone());

    if let Some(code) = config.language.code() {
        form = form.text("language", code.to_string());
    }

    let prompt = config
        .prompt_override
        .as_deref()
        .unwrap_or(config.language.prompt());
    form = form.text("prompt", prompt.to_string());

    let path = if provider == "groq" {
        "openai/v1/audio/transcriptions"
    } else {
        "v1/audio/transcriptions"
    };
    let url = format!("{base_url}{path}");

    let response = client
        .post(&url)
        .bearer_auth(&config.api_key)
        .multipart(form)
        .send()
        .await?;

    let status = response.status();

    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(AppError::ApiKeyMissing(provider.to_string()));
    }

    if status == reqwest::StatusCode::PAYLOAD_TOO_LARGE {
        return Err(AppError::Transcription("file too large (max 25MB)".to_string()));
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

/// Transcribe a file and return full verbose_json response with segments.
pub async fn transcribe_file_with_segments(
    config: &SttConfig,
    file_data: &[u8],
    filename: &str,
    mime_type: &str,
    provider: &str,
) -> Result<WhisperVerboseResponse, AppError> {
    let base_url = base_url_for_provider(provider);
    transcribe_file_with_segments_internal(config, file_data, filename, mime_type, provider, base_url).await
}

/// Internal: with configurable base URL for testing.
pub(crate) async fn transcribe_file_with_segments_internal(
    config: &SttConfig,
    file_data: &[u8],
    filename: &str,
    mime_type: &str,
    provider: &str,
    base_url: &str,
) -> Result<WhisperVerboseResponse, AppError> {
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(AppError::Network)?;

    let file_part = multipart::Part::bytes(file_data.to_vec())
        .file_name(filename.to_string())
        .mime_str(mime_type)
        .map_err(|e| AppError::Transcription(e.to_string()))?;

    let mut form = multipart::Form::new()
        .part("file", file_part)
        .text("model", config.model.clone())
        .text("response_format", "verbose_json".to_string());

    if let Some(code) = config.language.code() {
        form = form.text("language", code.to_string());
    }

    let prompt = config.prompt_override.as_deref().unwrap_or(config.language.prompt());
    form = form.text("prompt", prompt.to_string());

    let path = if provider == "groq" {
        "openai/v1/audio/transcriptions"
    } else {
        "v1/audio/transcriptions"
    };
    let url = format!("{base_url}{path}");

    let response = client.post(&url).bearer_auth(&config.api_key).multipart(form).send().await?;
    let status = response.status();

    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(AppError::ApiKeyMissing(provider.to_string()));
    }
    if status == reqwest::StatusCode::PAYLOAD_TOO_LARGE {
        return Err(AppError::Transcription("file too large (max 25MB)".to_string()));
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Transcription(format!("HTTP {}: {}", status.as_u16(), body)));
    }

    let verbose: WhisperVerboseResponse = response
        .json()
        .await
        .map_err(|e| AppError::Transcription(format!("failed to parse response: {e}")))?;

    Ok(verbose)
}

/// Internal: transcribe with configurable base URL (for testing with wiremock).
pub(crate) async fn transcribe_with_base_url(
    config: &SttConfig,
    wav_data: &[u8],
    provider: &str,
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

    // Add prompt hint — use override (with vocabulary) if available
    let prompt = config
        .prompt_override
        .as_deref()
        .unwrap_or(config.language.prompt());
    form = form.text("prompt", prompt.to_string());

    let path = if provider == "groq" {
        "openai/v1/audio/transcriptions"
    } else {
        "v1/audio/transcriptions"
    };
    let url = format!("{base_url}{path}");

    let response = client
        .post(&url)
        .bearer_auth(&config.api_key)
        .multipart(form)
        .send()
        .await?;

    let status = response.status();

    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(AppError::ApiKeyMissing(provider.to_string()));
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

// ---------------------------------------------------------------------------
// Chat Completion (LLM Refinement)
// ---------------------------------------------------------------------------

/// Configuration for a single LLM chat completion call.
#[derive(Clone)]
pub struct ChatConfig {
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

// Custom Debug that masks the API key (same pattern as SttConfig)
impl fmt::Debug for ChatConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChatConfig")
            .field("api_key", &"****")
            .field("model", &self.model)
            .field("temperature", &self.temperature)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

impl ChatConfig {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            model: DEFAULT_LLM_MODEL.to_string(),
            temperature: LLM_TEMPERATURE,
            max_tokens: LLM_MAX_TOKENS,
        }
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
    /// Groq-specific: suppress `<think>` output from reasoning models
    /// (e.g. Qwen3-32B). `"hidden"` returns only the final answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_format: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

/// Refine text via Groq's chat completion API (convenience wrapper).
///
/// Sends a chat request with system prompt + user text to the Groq LLM.
/// Returns the refined text on success.
pub async fn chat_completion(
    config: &ChatConfig,
    system_prompt: &str,
    user_text: &str,
) -> Result<String, AppError> {
    chat_completion_with_provider(config, system_prompt, user_text, "groq", GROQ_BASE_URL).await
}

/// Send a chat completion request to any OpenAI-compatible provider.
///
/// Routes to the correct endpoint path based on provider:
/// - Groq: `{base_url}openai/v1/chat/completions`
/// - OpenAI/OpenRouter/others: `{base_url}v1/chat/completions`
///
/// For OpenRouter, adds required `HTTP-Referer` and `X-Title` headers.
pub(crate) async fn chat_completion_with_provider(
    config: &ChatConfig,
    system_prompt: &str,
    user_text: &str,
    provider: &str,
    base_url: &str,
) -> Result<String, AppError> {
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(READ_WRITE_TIMEOUT)
        .build()
        .map_err(AppError::Network)?;

    // Groq supports `reasoning_format: "hidden"` to suppress `<think>`
    // output from reasoning models (e.g. Qwen3-32B, DeepSeek-R1).
    let reasoning_format = if provider == "groq" {
        Some("hidden".to_string())
    } else {
        None
    };

    let body = ChatRequest {
        model: config.model.clone(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_text.to_string(),
            },
        ],
        temperature: config.temperature,
        max_tokens: config.max_tokens,
        reasoning_format,
    };

    // Groq's OpenAI-compatible endpoint has an extra "openai/" prefix
    let path = if provider == "groq" {
        "openai/v1/chat/completions"
    } else {
        "v1/chat/completions"
    };
    let url = format!("{base_url}{path}");

    let mut req = client
        .post(&url)
        .bearer_auth(&config.api_key)
        .json(&body);

    // OpenRouter requires these headers for their API
    if provider == "openrouter" {
        req = req
            .header("HTTP-Referer", "https://voxpen.app")
            .header("X-Title", "VoxPen");
    }

    let response = req.send().await?;

    let status = response.status();

    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(AppError::ApiKeyMissing(provider.to_string()));
    }

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Refinement(format!(
            "HTTP {}: {}",
            status.as_u16(),
            body
        )));
    }

    let chat: ChatResponse = response
        .json()
        .await
        .map_err(|e| AppError::Refinement(format!("failed to parse response: {e}")))?;

    let text = chat
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .ok_or_else(|| AppError::Refinement("no response from LLM".to_string()))?;

    // Strip <think>…</think> blocks that reasoning models (Qwen3, DeepSeek-R1,
    // etc.) may emit even when reasoning_format is set. This is a universal
    // safety net for all providers including custom endpoints.
    let text = strip_thinking_tags(&text);

    Ok(text)
}

/// Strip `<think>…</think>` blocks from LLM output.
///
/// Reasoning models (Qwen3, DeepSeek-R1, QwQ, etc.) may wrap their chain-of-
/// thought in `<think>` tags. This function removes those blocks and trims the
/// remaining text. Works across all providers as a universal safety net.
fn strip_thinking_tags(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find("<think>") {
        result.push_str(&rest[..start]);
        if let Some(end) = rest[start..].find("</think>") {
            rest = &rest[start + end + "</think>".len()..];
        } else {
            // Unclosed <think> — discard everything after it
            return result.trim().to_string();
        }
    }
    result.push_str(rest);
    result.trim().to_string()
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

        let result =
            transcribe_with_base_url(&config, &wav_data, "groq", &format!("{}/", server.uri()))
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

        let result =
            transcribe_with_base_url(&config, &wav_data, "groq", &format!("{}/", server.uri()))
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

        let result =
            transcribe_with_base_url(&config, &wav_data, "groq", &format!("{}/", server.uri()))
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

        let result =
            transcribe_with_base_url(&config, &wav_data, "groq", &format!("{}/", server.uri()))
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

        let result =
            transcribe_with_base_url(&config, &wav_data, "groq", &format!("{}/", server.uri()))
                .await;
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Chat Completion tests
    // -----------------------------------------------------------------------

    fn test_chat_config(api_key: &str) -> ChatConfig {
        ChatConfig::new(api_key.to_string())
    }

    fn chat_response_json(content: &str) -> serde_json::Value {
        serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": content
                }
            }]
        })
    }

    #[tokio::test]
    async fn should_return_refined_text_when_chat_api_responds_successfully() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/chat/completions"))
            .and(header("authorization", "Bearer test-chat-key"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(chat_response_json("整理後的文字")),
            )
            .mount(&server)
            .await;

        let config = test_chat_config("test-chat-key");

        let result = chat_completion_with_provider(
            &config,
            "你是一個語音轉文字的編輯助手。",
            "嗯那個就是我想說你好",
            "groq",
            &format!("{}/", server.uri()),
        )
        .await;

        assert_eq!(result.unwrap(), "整理後的文字");
    }

    #[tokio::test]
    async fn should_return_api_key_error_on_chat_401() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;

        let config = test_chat_config("bad-key");

        let result = chat_completion_with_provider(
            &config,
            "system prompt",
            "user text",
            "groq",
            &format!("{}/", server.uri()),
        )
        .await;

        assert!(matches!(result, Err(AppError::ApiKeyMissing(_))));
    }

    #[tokio::test]
    async fn should_return_refinement_error_on_chat_500() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal error"))
            .mount(&server)
            .await;

        let config = test_chat_config("test-key");

        let result = chat_completion_with_provider(
            &config,
            "system prompt",
            "user text",
            "groq",
            &format!("{}/", server.uri()),
        )
        .await;

        match result {
            Err(AppError::Refinement(msg)) => assert!(msg.contains("500")),
            other => panic!("expected Refinement error, got {:?}", other),
        }
    }

    #[test]
    fn should_use_default_llm_model() {
        let config = ChatConfig::new("key".to_string());
        assert_eq!(config.model, DEFAULT_LLM_MODEL);
        assert_eq!(config.temperature, LLM_TEMPERATURE);
        assert_eq!(config.max_tokens, LLM_MAX_TOKENS);
    }

    #[tokio::test]
    async fn should_handle_empty_choices_array() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"choices": []})),
            )
            .mount(&server)
            .await;

        let config = test_chat_config("test-key");

        let result = chat_completion_with_provider(
            &config,
            "system prompt",
            "user text",
            "groq",
            &format!("{}/", server.uri()),
        )
        .await;

        match result {
            Err(AppError::Refinement(msg)) => {
                assert_eq!(msg, "no response from LLM");
            }
            other => panic!("expected Refinement error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn should_add_openrouter_headers_when_provider_is_openrouter() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("HTTP-Referer", "https://voxpen.app"))
            .and(header("X-Title", "VoxPen"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(chat_response_json("refined")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let config = test_chat_config("test-key");
        let result = chat_completion_with_provider(
            &config,
            "prompt",
            "text",
            "openrouter",
            &format!("{}/", server.uri()),
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "refined");
    }

    #[tokio::test]
    async fn should_use_v1_path_for_openai_provider() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(chat_response_json("refined")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let config = test_chat_config("test-key");
        let result = chat_completion_with_provider(
            &config,
            "prompt",
            "text",
            "openai",
            &format!("{}/", server.uri()),
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn should_include_provider_name_in_401_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;

        let config = test_chat_config("bad-key");
        let result = chat_completion_with_provider(
            &config,
            "prompt",
            "text",
            "openrouter",
            &format!("{}/", server.uri()),
        )
        .await;

        match result {
            Err(AppError::ApiKeyMissing(provider)) => assert_eq!(provider, "openrouter"),
            other => panic!("expected ApiKeyMissing(openrouter), got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // transcribe_file tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn should_transcribe_file_with_mp3_mime_type() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .and(header("authorization", "Bearer file-key"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"text": "file transcription"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let config = test_config("file-key", Language::Auto);
        let fake_mp3 = vec![0xFF, 0xFB, 0x90, 0x00]; // fake MP3 header bytes

        let result = transcribe_file_with_base_url(
            &config,
            &fake_mp3,
            "recording.mp3",
            "audio/mpeg",
            "groq",
            &format!("{}/", server.uri()),
        )
        .await;

        assert_eq!(result.unwrap(), "file transcription");
    }

    #[tokio::test]
    async fn should_transcribe_file_with_openai_provider_path() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/audio/transcriptions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"text": "openai file result"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let config = test_config("openai-key", Language::English);
        let fake_data = vec![0u8; 100];

        let result = transcribe_file_with_base_url(
            &config,
            &fake_data,
            "audio.wav",
            "audio/wav",
            "openai",
            &format!("{}/", server.uri()),
        )
        .await;

        assert_eq!(result.unwrap(), "openai file result");
    }

    #[tokio::test]
    async fn should_return_error_on_file_too_large_413() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(413).set_body_string("too large"))
            .mount(&server)
            .await;

        let config = test_config("key", Language::Auto);
        let result = transcribe_file_with_base_url(
            &config,
            &[0u8; 100],
            "big.wav",
            "audio/wav",
            "groq",
            &format!("{}/", server.uri()),
        )
        .await;

        match result {
            Err(AppError::Transcription(msg)) => assert!(msg.contains("too large")),
            other => panic!("expected Transcription error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn should_return_api_key_error_on_file_transcribe_401() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let config = test_config("bad-key", Language::Auto);
        let result = transcribe_file_with_base_url(
            &config,
            &[0u8; 10],
            "test.mp3",
            "audio/mpeg",
            "groq",
            &format!("{}/", server.uri()),
        )
        .await;

        assert!(matches!(result, Err(AppError::ApiKeyMissing(_))));
    }

    // -----------------------------------------------------------------------
    // base_url_for_provider tests
    // -----------------------------------------------------------------------

    #[test]
    fn should_resolve_base_url_for_known_providers() {
        assert_eq!(base_url_for_provider("groq"), "https://api.groq.com/");
        assert_eq!(base_url_for_provider("openai"), "https://api.openai.com/");
        assert_eq!(
            base_url_for_provider("openrouter"),
            "https://openrouter.ai/api/"
        );
    }

    #[test]
    fn should_return_provider_string_as_url_for_custom() {
        assert_eq!(
            base_url_for_provider("http://localhost:11434/"),
            "http://localhost:11434/"
        );
        assert_eq!(
            base_url_for_provider("https://my-server.example.com/"),
            "https://my-server.example.com/"
        );
    }

    // ── strip_thinking_tags tests ─────────────────────────────────────

    #[test]
    fn should_pass_through_text_without_think_tags() {
        assert_eq!(strip_thinking_tags("Hello world"), "Hello world");
    }

    #[test]
    fn should_strip_think_block_at_start() {
        let input = "<think>reasoning here</think>Final answer";
        assert_eq!(strip_thinking_tags(input), "Final answer");
    }

    #[test]
    fn should_strip_multiline_think_block() {
        let input = "<think>\nStep 1: ...\nStep 2: ...\n</think>\nThe answer is 42.";
        assert_eq!(strip_thinking_tags(input), "The answer is 42.");
    }

    #[test]
    fn should_strip_multiple_think_blocks() {
        let input = "<think>first</think>Hello <think>second</think>world";
        assert_eq!(strip_thinking_tags(input), "Hello world");
    }

    #[test]
    fn should_handle_unclosed_think_tag() {
        let input = "Good start<think>reasoning without end";
        assert_eq!(strip_thinking_tags(input), "Good start");
    }

    #[test]
    fn should_handle_empty_think_block() {
        let input = "<think></think>Just the answer";
        assert_eq!(strip_thinking_tags(input), "Just the answer");
    }

    // -----------------------------------------------------------------------
    // WhisperVerboseResponse / segment tests
    // -----------------------------------------------------------------------

    #[test]
    fn should_deserialize_verbose_json_segments() {
        let json = serde_json::json!({
            "text": "Hello world. How are you?",
            "segments": [
                {
                    "id": 0,
                    "start": 0.0,
                    "end": 1.5,
                    "text": "Hello world."
                },
                {
                    "id": 1,
                    "start": 1.5,
                    "end": 3.0,
                    "text": " How are you?"
                }
            ]
        });
        let resp: WhisperVerboseResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.text, "Hello world. How are you?");
        assert_eq!(resp.segments.len(), 2);
        assert_eq!(resp.segments[0].start, 0.0);
        assert_eq!(resp.segments[0].end, 1.5);
        assert_eq!(resp.segments[0].text, "Hello world.");
        assert_eq!(resp.segments[1].start, 1.5);
        assert_eq!(resp.segments[1].text, " How are you?");
    }

    #[tokio::test]
    async fn should_return_segments_from_transcribe_file_with_segments() {
        let server = MockServer::start().await;

        let verbose_response = serde_json::json!({
            "text": "Hello world.",
            "segments": [
                {"id": 0, "start": 0.0, "end": 1.5, "text": "Hello world."}
            ]
        });

        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(verbose_response))
            .expect(1)
            .mount(&server)
            .await;

        let config = test_config("key", Language::Auto);
        let fake_data = vec![0u8; 100];

        let result = transcribe_file_with_segments_internal(
            &config, &fake_data, "test.wav", "audio/wav", "groq",
            &format!("{}/", server.uri()),
        ).await.unwrap();

        assert_eq!(result.text, "Hello world.");
        assert_eq!(result.segments.len(), 1);
        assert_eq!(result.segments[0].start, 0.0);
        assert_eq!(result.segments[0].end, 1.5);
    }
}
