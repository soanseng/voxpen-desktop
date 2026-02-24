use crate::pipeline::state::Language;

/// Returns the LLM refinement system prompt for the given language.
///
/// These prompts are identical to the Android version (`RefinementPrompt.kt`).
/// They instruct the LLM to clean up speech transcription into polished text.
pub fn for_language(lang: &Language) -> &'static str {
    match lang {
        Language::Chinese => ZH_TW_PROMPT,
        Language::English => EN_PROMPT,
        Language::Japanese => JA_PROMPT,
        Language::Auto => MIXED_PROMPT,
    }
}

const ZH_TW_PROMPT: &str = "\
你是一個語音轉文字的編輯助手。請將以下口語內容整理為流暢的書面文字：
1. 移除贅字（嗯、那個、就是、然後、對、呃）
2. 如果說話者中途改口，只保留最終的意思
3. 修正語法但保持原意
4. 適當加入標點符號
5. 不要添加原文沒有的內容
6. 保持繁體中文
只輸出整理後的文字，不要加任何解釋。";

const EN_PROMPT: &str = "\
You are a voice-to-text editor. Clean up the following speech transcription into polished written text:
1. Remove filler words (um, uh, like, you know, I mean, basically, actually, so)
2. If the speaker corrected themselves mid-sentence, keep only the final version
3. Fix grammar while preserving the original meaning
4. Add proper punctuation
5. Do not add content that wasn't in the original speech
Output only the cleaned text, no explanations.";

const JA_PROMPT: &str = "\
あなたは音声テキスト変換の編集アシスタントです。以下の口語内容を整った書き言葉に整理してください：
1. フィラー（えーと、あの、まあ、なんか、ちょっと）を除去
2. 言い直しがある場合は最終的な意味のみ残す
3. 文法を修正し、原意を保持
4. 適切に句読点を追加
5. 原文にない内容を追加しない
整理後のテキストのみ出力し、説明は不要です。";

const MIXED_PROMPT: &str = "\
你是一個語音轉文字的編輯助手。以下口語內容可能包含多種語言混合使用（如中英混合），請保持原本的語言混合方式，整理為流暢的書面文字：
1. 移除各語言的贅字
2. 如果說話者中途改口，只保留最終的意思
3. 修正語法但保持原意和原本的語言選擇
4. 適當加入標點符號
5. 不要把外語強制翻譯成中文
只輸出整理後的文字，不要加任何解釋。";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_return_zh_tw_prompt_for_chinese() {
        let prompt = for_language(&Language::Chinese);
        assert!(prompt.contains("繁體中文"));
        assert!(prompt.contains("移除贅字"));
    }

    #[test]
    fn should_return_en_prompt_for_english() {
        let prompt = for_language(&Language::English);
        assert!(prompt.contains("filler words"));
        assert!(prompt.contains("no explanations"));
    }

    #[test]
    fn should_return_ja_prompt_for_japanese() {
        let prompt = for_language(&Language::Japanese);
        assert!(prompt.contains("フィラー"));
        assert!(prompt.contains("説明は不要"));
    }

    #[test]
    fn should_return_mixed_prompt_for_auto() {
        let prompt = for_language(&Language::Auto);
        assert!(prompt.contains("多種語言混合"));
        assert!(prompt.contains("不要把外語強制翻譯"));
    }

    #[test]
    fn should_not_return_empty_prompts() {
        for lang in &[
            Language::Auto,
            Language::Chinese,
            Language::English,
            Language::Japanese,
        ] {
            assert!(!for_language(lang).is_empty());
        }
    }
}
