use serde::{Deserialize, Serialize};

use crate::pipeline::state::{Language, RecordingMode};

/// Application settings, serialized for Tauri IPC and persistent storage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    /// Global hotkey shortcut string (e.g., "CommandOrControl+Shift+V")
    pub hotkey: String,
    /// Recording mode: hold-to-record or toggle
    pub recording_mode: RecordingMode,
    /// Whether to auto-paste transcribed text at cursor
    pub auto_paste: bool,
    /// Whether to launch app at system login
    pub launch_at_login: bool,
    /// STT provider name
    pub stt_provider: String,
    /// STT language
    pub stt_language: Language,
    /// STT model identifier
    pub stt_model: String,
    /// Whether LLM refinement is enabled
    pub refinement_enabled: bool,
    /// LLM refinement provider name
    pub refinement_provider: String,
    /// LLM model identifier
    pub refinement_model: String,
    /// Custom refinement system prompt (empty = use built-in per-language default)
    #[serde(default)]
    pub refinement_prompt: String,
    /// UI theme: "system", "light", or "dark"
    pub theme: String,
    /// UI language: "en" or "zh-TW"
    pub ui_language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "CommandOrControl+Shift+V".to_string(),
            recording_mode: RecordingMode::HoldToRecord,
            auto_paste: true,
            launch_at_login: false,
            stt_provider: "groq".to_string(),
            stt_language: Language::Auto,
            stt_model: crate::api::groq::DEFAULT_STT_MODEL.to_string(),
            refinement_enabled: false,
            refinement_provider: "groq".to_string(),
            refinement_model: crate::api::groq::DEFAULT_LLM_MODEL.to_string(),
            refinement_prompt: String::new(),
            theme: "system".to_string(),
            ui_language: "en".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_have_sensible_defaults() {
        let settings = Settings::default();
        assert_eq!(settings.hotkey, "CommandOrControl+Shift+V");
        assert_eq!(settings.recording_mode, RecordingMode::HoldToRecord);
        assert!(settings.auto_paste);
        assert!(!settings.launch_at_login);
        assert_eq!(settings.stt_provider, "groq");
        assert_eq!(settings.stt_language, Language::Auto);
        assert!(!settings.refinement_enabled);
        assert_eq!(settings.theme, "system");
        assert_eq!(settings.ui_language, "en");
    }

    #[test]
    fn should_roundtrip_through_json() {
        let settings = Settings::default();
        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(settings, deserialized);
    }

    #[test]
    fn should_serialize_to_expected_json_keys() {
        let settings = Settings::default();
        let value: serde_json::Value = serde_json::to_value(&settings).unwrap();
        assert!(value.get("hotkey").is_some());
        assert!(value.get("recording_mode").is_some());
        assert!(value.get("auto_paste").is_some());
        assert!(value.get("stt_provider").is_some());
        assert!(value.get("refinement_enabled").is_some());
        assert!(value.get("theme").is_some());
        assert!(value.get("ui_language").is_some());
    }
}
