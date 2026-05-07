use crate::api::groq::{self, ChatConfig};
use crate::error::AppError;

pub const LISTEN_COMMAND_SYSTEM_PROMPT: &str = "\
You are VoxPen's Listen to My Command engine. Turn the user's spoken instruction \
into a useful pasteable artifact for the app they are currently using.

Output only the requested artifact or concise answer. Do not wrap it in labels \
unless the user asked for labels.

Safety boundary: VoxPen does not execute actions. You must not claim that you \
ran shell commands, edited files, edit files, sent messages, clicked buttons, opened apps, \
or changed system state. If the user asks you to execute a system action, produce \
safe pasteable instructions, a draft, or a command snippet instead.";

pub async fn generate(
    command: &str,
    active_app: Option<&str>,
    config: &ChatConfig,
    provider: &str,
    custom_base_url: &str,
) -> Result<String, AppError> {
    let base_url = if provider == "custom" && !custom_base_url.is_empty() {
        custom_base_url
    } else {
        groq::base_url_for_provider(provider)
    };
    generate_with_base_url(command, active_app, config, provider, base_url).await
}

async fn generate_with_base_url(
    command: &str,
    active_app: Option<&str>,
    config: &ChatConfig,
    provider: &str,
    base_url: &str,
) -> Result<String, AppError> {
    let command = command.trim();
    if command.is_empty() {
        return Err(AppError::Command("no command to run".to_string()));
    }

    let active_app = active_app
        .map(str::trim)
        .filter(|app| !app.is_empty())
        .unwrap_or("Unknown");
    let user_content = format!("Active app: {active_app}\n\nInstruction:\n{command}");

    let text = groq::chat_completion_with_provider(
        config,
        LISTEN_COMMAND_SYSTEM_PROMPT,
        &user_content,
        provider,
        base_url,
    )
    .await
    .map_err(|e| match e {
        AppError::Refinement(msg) => AppError::Command(msg),
        other => other,
    })?;

    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(AppError::Command("empty command output".to_string()));
    }

    Ok(text)
}

#[cfg(test)]
mod tests {
    use crate::api::groq::ChatConfig;
    use crate::error::AppError;
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

    #[test]
    fn system_prompt_sets_pasteable_safety_boundary() {
        assert!(super::LISTEN_COMMAND_SYSTEM_PROMPT.contains("pasteable"));
        assert!(super::LISTEN_COMMAND_SYSTEM_PROMPT.contains("does not execute"));
        assert!(super::LISTEN_COMMAND_SYSTEM_PROMPT.contains("shell commands"));
        assert!(super::LISTEN_COMMAND_SYSTEM_PROMPT.contains("edit files"));
    }

    #[tokio::test]
    async fn should_generate_command_output_using_groq_path() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response(
                "function debounce(fn, wait) { return fn; }",
            )))
            .expect(1)
            .mount(&server)
            .await;

        let mut config = test_config("test-key");
        config.model = "openai/gpt-oss-120b".to_string();
        let result = super::generate_with_base_url(
            "write a debounce function",
            Some("Code"),
            &config,
            "groq",
            &format!("{}/", server.uri()),
        )
        .await;

        assert_eq!(
            result.unwrap(),
            "function debounce(fn, wait) { return fn; }"
        );
    }

    #[tokio::test]
    async fn should_generate_command_output_using_custom_openai_compatible_path() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response(
                "Custom provider output",
            )))
            .expect(1)
            .mount(&server)
            .await;

        let result = super::generate_with_base_url(
            "draft a PR summary",
            None,
            &test_config("custom-key"),
            "custom",
            &format!("{}/", server.uri()),
        )
        .await;

        assert_eq!(result.unwrap(), "Custom provider output");
    }

    #[tokio::test]
    async fn should_reject_empty_command() {
        let result =
            super::generate("", None, &test_config("key"), "openai", "").await;

        match result {
            Err(AppError::Command(msg)) => assert_eq!(msg, "no command to run"),
            other => panic!("expected Command error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn should_reject_empty_llm_output() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("   ")))
            .expect(1)
            .mount(&server)
            .await;

        let result = super::generate_with_base_url(
            "write release notes",
            None,
            &test_config("key"),
            "openai",
            &format!("{}/", server.uri()),
        )
        .await;

        match result {
            Err(AppError::Command(msg)) => assert_eq!(msg, "empty command output"),
            other => panic!("expected Command error, got {other:?}"),
        }
    }
}
