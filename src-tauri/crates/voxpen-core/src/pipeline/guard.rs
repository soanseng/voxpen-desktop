// Whisper hallucination detection.
//
// When Whisper receives silence or very low-energy audio, it often
// "hallucinates" — echoing the prompt text, producing common filler
// phrases, or outputting fragments of its training data. This module
// detects such outputs and prevents them from being pasted.

/// Known prompt fragments that Whisper echoes when given silence.
/// These are substrings of the language prompts sent to the Whisper API.
const PROMPT_FRAGMENTS: &[&str] = &[
    "請勿使用簡體中文",
    "以繁體中文輸出",
    "以繁體中文轉錄",
    "可能夾雜英文",
    "Transcribe the following",
    "文字起こし",
    "음성을 전사",
    "Transcription de la parole",
    "Transkription der deutschen",
    "Transcripción del habla",
    "Phiên âm giọng nói",
    "Transkripsi ucapan",
    "ถอดเสียงภาษาไทย",
];

/// Common Whisper hallucinations on silence (language-agnostic).
const COMMON_HALLUCINATIONS: &[&str] = &[
    "Thank you for watching",
    "Thanks for watching",
    "Subscribe to my channel",
    "Please subscribe",
    "Subtitles by",
    "字幕提供",
];

/// Check if STT output is likely a Whisper hallucination rather than
/// genuine speech transcription.
///
/// Returns `true` if the text matches known hallucination patterns:
/// - Empty or whitespace-only output
/// - Prompt fragment echoes
/// - Common Whisper silence hallucinations
pub fn is_hallucination(text: &str) -> bool {
    let trimmed = text.trim();

    if trimmed.is_empty() {
        return true;
    }

    // Check against known prompt fragments
    for pattern in PROMPT_FRAGMENTS {
        if trimmed.contains(pattern) {
            return true;
        }
    }

    // Check against common silence hallucinations
    let lower = trimmed.to_lowercase();
    for pattern in COMMON_HALLUCINATIONS {
        if lower.contains(&pattern.to_lowercase()) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_detect_empty_as_hallucination() {
        assert!(is_hallucination(""));
        assert!(is_hallucination("   "));
        assert!(is_hallucination("\n\t"));
    }

    #[test]
    fn should_detect_prompt_echo_as_hallucination() {
        assert!(is_hallucination("請勿使用簡體中文。"));
        assert!(is_hallucination("以繁體中文輸出，可能夾雜英文。請勿使用簡體中文。"));
        assert!(is_hallucination("以繁體中文轉錄，請勿使用簡體中文。"));
    }

    #[test]
    fn should_detect_common_hallucinations() {
        assert!(is_hallucination("Thank you for watching."));
        // eq_ignore_ascii_case for common patterns
        assert!(is_hallucination("thank you for watching"));
        assert!(is_hallucination("Subscribe to my channel"));
    }

    #[test]
    fn should_not_flag_genuine_speech() {
        assert!(!is_hallucination("今天天氣很好"));
        assert!(!is_hallucination("Hello world, this is a test."));
        assert!(!is_hallucination("我想要訂一杯咖啡"));
        assert!(!is_hallucination("明天開會要討論新的 API 設計"));
    }

    #[test]
    fn should_detect_partial_prompt_fragments() {
        // Whisper sometimes echoes just a fragment
        assert!(is_hallucination("請勿使用簡體中文"));
        assert!(is_hallucination("以繁體中文轉錄"));
        assert!(is_hallucination("Transcribe the following English speech."));
    }

    #[test]
    fn should_detect_japanese_prompt_echo() {
        assert!(is_hallucination("以下の日本語音声を文字起こししてください。"));
    }
}
