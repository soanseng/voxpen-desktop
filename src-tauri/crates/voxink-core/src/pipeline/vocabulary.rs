use crate::pipeline::state::Language;

/// Maximum token budget for Whisper prompt vocabulary.
/// Base prompt uses ~10-20 tokens, leaving ~200 for vocabulary.
const WHISPER_VOCAB_TOKEN_BUDGET: usize = 200;

/// Estimate token count for a string.
/// CJK character ≈ 2 tokens, Latin character ≈ 0.25 tokens.
fn estimate_tokens(s: &str) -> usize {
    let mut tokens = 0.0_f64;
    for ch in s.chars() {
        if ch.is_ascii() {
            tokens += 0.25;
        } else {
            tokens += 2.0;
        }
    }
    tokens.ceil() as usize
}

/// Build vocabulary hint string for Whisper STT prompt parameter.
///
/// Appends vocabulary words to the language-specific base prompt, separated by `, `.
/// Truncates from the end (oldest entries) if over token budget.
/// Returns `None` if vocabulary is empty.
pub fn build_stt_hint(words: &[String], language: &Language) -> Option<String> {
    if words.is_empty() {
        return None;
    }

    let base = language.prompt();
    let separator = " ";
    let mut result = base.to_string();
    let base_tokens = estimate_tokens(&result);
    let mut remaining = WHISPER_VOCAB_TOKEN_BUDGET.saturating_sub(base_tokens);

    let mut added = Vec::new();
    for word in words {
        let word_tokens = estimate_tokens(word) + 1; // +1 for ", " separator
        if word_tokens > remaining {
            break;
        }
        added.push(word.as_str());
        remaining -= word_tokens;
    }

    if added.is_empty() {
        return None;
    }

    result.push_str(separator);
    result.push_str(&added.join(", "));
    Some(result)
}

/// Build vocabulary suffix for LLM refinement system prompt.
///
/// Returns a localized suffix like:
/// - Chinese: `\n\n術語表（請優先使用這些詞彙）：語墨, Anthropic`
/// - English: `\n\nVocabulary (prefer these terms): VoxInk, Anthropic`
/// Returns `None` if vocabulary is empty.
pub fn build_llm_suffix(words: &[String], language: &Language) -> Option<String> {
    if words.is_empty() {
        return None;
    }

    let joined = words.join(", ");
    let suffix = match language {
        Language::English => format!("\n\nVocabulary (prefer these terms): {joined}"),
        Language::Japanese => format!("\n\n用語集（以下の用語を優先してください）：{joined}"),
        _ => format!("\n\n術語表（請優先使用這些詞彙）：{joined}"),
    };
    Some(suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_return_none_for_empty_vocabulary_stt() {
        assert!(build_stt_hint(&[], &Language::Chinese).is_none());
    }

    #[test]
    fn should_return_none_for_empty_vocabulary_llm() {
        assert!(build_llm_suffix(&[], &Language::Chinese).is_none());
    }

    #[test]
    fn should_append_words_to_base_prompt() {
        let words = vec!["語墨".to_string(), "Anthropic".to_string()];
        let hint = build_stt_hint(&words, &Language::Chinese).unwrap();
        assert!(hint.starts_with("繁體中文轉錄。"));
        assert!(hint.contains("語墨"));
        assert!(hint.contains("Anthropic"));
    }

    #[test]
    fn should_truncate_when_over_token_budget() {
        // Create many long CJK words to exceed budget
        let words: Vec<String> = (0..200)
            .map(|i| format!("很長的詞彙名稱{i}"))
            .collect();
        let hint = build_stt_hint(&words, &Language::Chinese).unwrap();
        // Should not contain all 200 words
        let comma_count = hint.matches(',').count();
        assert!(comma_count < 199, "should truncate, got {comma_count} commas");
    }

    #[test]
    fn should_build_chinese_llm_suffix() {
        let words = vec!["語墨".to_string(), "Anthropic".to_string()];
        let suffix = build_llm_suffix(&words, &Language::Chinese).unwrap();
        assert!(suffix.contains("術語表"));
        assert!(suffix.contains("語墨, Anthropic"));
    }

    #[test]
    fn should_build_english_llm_suffix() {
        let words = vec!["VoxInk".to_string()];
        let suffix = build_llm_suffix(&words, &Language::English).unwrap();
        assert!(suffix.contains("Vocabulary (prefer these terms)"));
        assert!(suffix.contains("VoxInk"));
    }

    #[test]
    fn should_build_japanese_llm_suffix() {
        let words = vec!["語墨".to_string()];
        let suffix = build_llm_suffix(&words, &Language::Japanese).unwrap();
        assert!(suffix.contains("用語集"));
    }

    #[test]
    fn should_use_chinese_suffix_for_auto_language() {
        let words = vec!["語墨".to_string()];
        let suffix = build_llm_suffix(&words, &Language::Auto).unwrap();
        assert!(suffix.contains("術語表"));
    }

    #[test]
    fn should_estimate_cjk_tokens_higher_than_ascii() {
        let cjk_tokens = estimate_tokens("語墨");
        let ascii_tokens = estimate_tokens("VoxInk");
        assert!(cjk_tokens > ascii_tokens);
    }
}
