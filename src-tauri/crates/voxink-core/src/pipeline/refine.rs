use crate::api::groq::{self, ChatConfig};
use crate::error::AppError;
use crate::pipeline::prompts;
use crate::pipeline::state::{Language, TonePreset};
use crate::pipeline::vocabulary;

/// Orchestrate text refinement via LLM.
///
/// Composes prompt resolution + optional vocabulary suffix + `groq::chat_completion()`.
/// Mirrors Android's `RefineTextUseCase`.
///
/// Prompt resolution priority:
/// - `Custom` tone with non-empty `custom_prompt` → use custom prompt
/// - `Custom` tone with empty `custom_prompt` → fallback to `for_language()` (Casual)
/// - Any other tone → use `for_language_and_tone()`
///
/// If `vocab_words` is non-empty, appends a vocabulary suffix to the system prompt.
pub async fn refine(
    text: &str,
    config: &ChatConfig,
    language: &Language,
    vocab_words: &[String],
    custom_prompt: &str,
    tone_preset: &TonePreset,
) -> Result<String, AppError> {
    if text.is_empty() {
        return Err(AppError::Refinement("no text to refine".to_string()));
    }

    let mut system_prompt = match tone_preset {
        TonePreset::Custom if !custom_prompt.is_empty() => custom_prompt.to_string(),
        TonePreset::Custom => prompts::for_language(language).to_string(),
        _ => prompts::for_language_and_tone(language, tone_preset).to_string(),
    };
    if let Some(suffix) = vocabulary::build_llm_suffix(vocab_words, language) {
        system_prompt.push_str(&suffix);
    }
    groq::chat_completion(config, &system_prompt, text).await
}

/// Internal: refine with configurable base URL (for testing with wiremock).
#[cfg(test)]
async fn refine_with_base_url(
    text: &str,
    config: &ChatConfig,
    language: &Language,
    base_url: &str,
    custom_prompt: &str,
    tone_preset: &TonePreset,
) -> Result<String, AppError> {
    if text.is_empty() {
        return Err(AppError::Refinement("no text to refine".to_string()));
    }

    let system_prompt = match tone_preset {
        TonePreset::Custom if !custom_prompt.is_empty() => custom_prompt.to_string(),
        TonePreset::Custom => prompts::for_language(language).to_string(),
        _ => prompts::for_language_and_tone(language, tone_preset).to_string(),
    };
    groq::chat_completion_with_provider(config, &system_prompt, text, "groq", base_url).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_config(api_key: &str) -> ChatConfig {
        ChatConfig::new(api_key.to_string())
    }

    fn chat_response(content: &str) -> serde_json::Value {
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
    async fn should_refine_text_using_correct_language_prompt() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(chat_response("你好世界！")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let config = test_config("test-key");
        let result = refine_with_base_url(
            "嗯 那個 你好世界",
            &config,
            &Language::Chinese,
            &format!("{}/", server.uri()),
            "",
            &TonePreset::Casual,
        )
        .await;

        assert_eq!(result.unwrap(), "你好世界！");
    }

    #[tokio::test]
    async fn should_reject_empty_text() {
        let config = test_config("key");
        let result = refine("", &config, &Language::Auto, &[], "", &TonePreset::Casual).await;

        match result {
            Err(AppError::Refinement(msg)) => assert_eq!(msg, "no text to refine"),
            other => panic!("expected Refinement error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn should_reject_empty_text_with_vocabulary() {
        let config = test_config("key");
        let vocab = vec!["語墨".to_string()];
        let result = refine("", &config, &Language::Auto, &vocab, "", &TonePreset::Casual).await;
        assert!(matches!(result, Err(AppError::Refinement(_))));
    }

    #[tokio::test]
    async fn should_propagate_api_errors() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let config = test_config("bad-key");
        let result = refine_with_base_url(
            "some text",
            &config,
            &Language::English,
            &format!("{}/", server.uri()),
            "",
            &TonePreset::Casual,
        )
        .await;

        assert!(matches!(result, Err(AppError::ApiKeyMissing(_))));
    }

    #[tokio::test]
    async fn should_use_custom_prompt_for_custom_tone() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/openai/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("refined")))
            .expect(1)
            .mount(&server)
            .await;

        let config = test_config("test-key");
        let result = refine_with_base_url(
            "some text",
            &config,
            &Language::English,
            &format!("{}/", server.uri()),
            "my custom prompt",
            &TonePreset::Custom,
        )
        .await;
        assert_eq!(result.unwrap(), "refined");
    }

    #[tokio::test]
    async fn should_fallback_to_casual_when_custom_tone_empty_prompt() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/openai/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("refined")))
            .expect(1)
            .mount(&server)
            .await;

        let config = test_config("test-key");
        let result = refine_with_base_url(
            "some text",
            &config,
            &Language::English,
            &format!("{}/", server.uri()),
            "", // empty custom prompt
            &TonePreset::Custom,
        )
        .await;
        assert_eq!(result.unwrap(), "refined");
    }
}
