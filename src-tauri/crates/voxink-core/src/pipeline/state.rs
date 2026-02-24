use serde::{Deserialize, Serialize};

/// Pipeline processing state, mirroring Android's `ImeUiState` sealed interface.
///
/// 7 states: Idle → Recording → Processing → Result → Refining → Refined → Error
/// Serialized with tagged enum for frontend consumption via Tauri events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum PipelineState {
    /// Ready to record
    Idle,

    /// Currently capturing audio
    Recording,

    /// Audio captured, sending to STT API
    Processing,

    /// STT completed, raw text available
    Result { text: String },

    /// Sending raw text to LLM for refinement
    Refining { original: String },

    /// LLM refinement completed
    Refined { original: String, refined: String },

    /// An error occurred at any stage
    Error { message: String },
}

/// Supported STT languages, mirroring Android's `SttLanguage` sealed class.
///
/// Each language carries its Whisper API `language` code and a prompt hint
/// that biases Whisper toward the expected language/script.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Language {
    Auto,
    Chinese,
    English,
    Japanese,
}

impl Language {
    /// Whisper API `language` parameter value. `None` means auto-detect.
    pub fn code(&self) -> Option<&'static str> {
        match self {
            Language::Auto => None,
            Language::Chinese => Some("zh"),
            Language::English => Some("en"),
            Language::Japanese => Some("ja"),
        }
    }

    /// Whisper `prompt` parameter — biases the model toward expected language/script.
    pub fn prompt(&self) -> &'static str {
        match self {
            Language::Auto => "繁體中文，可能夾雜英文。",
            Language::Chinese => "繁體中文轉錄。",
            Language::English => "Transcribe the following English speech.",
            Language::Japanese => "以下の日本語音声を文字起こししてください。",
        }
    }
}

/// Recording mode — how the hotkey triggers recording.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum RecordingMode {
    /// Hold hotkey to record, release to stop (default)
    #[default]
    HoldToRecord,
    /// Press once to start, press again to stop
    Toggle,
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- PipelineState tests --

    #[test]
    fn should_serialize_idle_state() {
        let state = PipelineState::Idle;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, r#"{"type":"Idle"}"#);
    }

    #[test]
    fn should_serialize_recording_state() {
        let state = PipelineState::Recording;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, r#"{"type":"Recording"}"#);
    }

    #[test]
    fn should_serialize_processing_state() {
        let state = PipelineState::Processing;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, r#"{"type":"Processing"}"#);
    }

    #[test]
    fn should_serialize_result_state_with_text() {
        let state = PipelineState::Result {
            text: "hello world".to_string(),
        };
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, r#"{"type":"Result","data":{"text":"hello world"}}"#);
    }

    #[test]
    fn should_serialize_refining_state() {
        let state = PipelineState::Refining {
            original: "raw text".to_string(),
        };
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(
            json,
            r#"{"type":"Refining","data":{"original":"raw text"}}"#
        );
    }

    #[test]
    fn should_serialize_refined_state() {
        let state = PipelineState::Refined {
            original: "raw".to_string(),
            refined: "polished".to_string(),
        };
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(
            json,
            r#"{"type":"Refined","data":{"original":"raw","refined":"polished"}}"#
        );
    }

    #[test]
    fn should_serialize_error_state() {
        let state = PipelineState::Error {
            message: "network timeout".to_string(),
        };
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(
            json,
            r#"{"type":"Error","data":{"message":"network timeout"}}"#
        );
    }

    #[test]
    fn should_roundtrip_pipeline_state_through_json() {
        let original = PipelineState::Refined {
            original: "你好".to_string(),
            refined: "你好！".to_string(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: PipelineState = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    // -- Language tests --

    #[test]
    fn should_return_none_code_for_auto() {
        assert_eq!(Language::Auto.code(), None);
    }

    #[test]
    fn should_return_zh_code_for_chinese() {
        assert_eq!(Language::Chinese.code(), Some("zh"));
    }

    #[test]
    fn should_return_en_code_for_english() {
        assert_eq!(Language::English.code(), Some("en"));
    }

    #[test]
    fn should_return_ja_code_for_japanese() {
        assert_eq!(Language::Japanese.code(), Some("ja"));
    }

    #[test]
    fn should_return_mixed_prompt_for_auto() {
        assert!(Language::Auto.prompt().contains("繁體中文"));
        assert!(Language::Auto.prompt().contains("英文"));
    }

    #[test]
    fn should_return_chinese_prompt_for_chinese() {
        assert_eq!(Language::Chinese.prompt(), "繁體中文轉錄。");
    }

    #[test]
    fn should_return_english_prompt_for_english() {
        assert!(Language::English.prompt().contains("English"));
    }

    #[test]
    fn should_return_japanese_prompt_for_japanese() {
        assert!(Language::Japanese.prompt().contains("日本語"));
    }

    // -- RecordingMode tests --

    #[test]
    fn should_default_to_hold_to_record() {
        assert_eq!(RecordingMode::default(), RecordingMode::HoldToRecord);
    }

    #[test]
    fn should_serialize_recording_mode() {
        let mode = RecordingMode::Toggle;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, r#""Toggle""#);
    }
}
