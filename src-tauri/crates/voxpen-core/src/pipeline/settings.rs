use serde::{Deserialize, Serialize};

use crate::pipeline::state::{Language, RecordingMode, TonePreset};

/// Application settings, serialized for Tauri IPC and persistent storage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    /// Push-to-talk hotkey (hold to record, release to stop)
    /// Migrated from old `hotkey` field via serde alias.
    #[serde(alias = "hotkey")]
    pub hotkey_ptt: String,
    /// Toggle hotkey (press once to start, press again to stop)
    #[serde(default = "default_hotkey_toggle")]
    pub hotkey_toggle: String,
    /// Kept for backwards compatibility when deserializing old settings.
    /// Not used in logic — mode is determined by which hotkey was pressed.
    #[serde(default)]
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
    /// Tone preset for refinement output style (Casual, Professional, Email, Note, Social, Custom)
    #[serde(default)]
    pub tone_preset: TonePreset,
    /// Custom API base URL for the "custom" provider (e.g., http://localhost:11434/ for Ollama).
    /// Empty string means not configured.
    #[serde(default)]
    pub custom_base_url: String,
    /// Preferred microphone device name. None = system default.
    #[serde(default)]
    pub microphone_device: Option<String>,
}

fn default_hotkey_toggle() -> String {
    "CommandOrControl+Shift+V".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey_ptt: "RAlt".to_string(),
            hotkey_toggle: default_hotkey_toggle(),
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
            tone_preset: TonePreset::default(),
            custom_base_url: String::new(),
            theme: "system".to_string(),
            ui_language: "en".to_string(),
            microphone_device: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_have_sensible_defaults() {
        let settings = Settings::default();
        assert_eq!(settings.hotkey_ptt, "RAlt");
        assert_eq!(settings.hotkey_toggle, "CommandOrControl+Shift+V");
        assert!(settings.auto_paste);
        assert!(!settings.launch_at_login);
        assert_eq!(settings.stt_provider, "groq");
        assert_eq!(settings.stt_language, Language::Auto);
        assert!(!settings.refinement_enabled);
        assert_eq!(settings.theme, "system");
        assert_eq!(settings.ui_language, "en");
        assert_eq!(settings.microphone_device, None);
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
        assert!(value.get("hotkey_ptt").is_some());
        assert!(value.get("hotkey_toggle").is_some());
        assert!(value.get("auto_paste").is_some());
        assert!(value.get("stt_provider").is_some());
        assert!(value.get("refinement_enabled").is_some());
        assert!(value.get("theme").is_some());
        assert!(value.get("ui_language").is_some());
    }

    #[test]
    fn should_migrate_old_hotkey_field() {
        let json = r#"{"hotkey":"F5","auto_paste":true,"launch_at_login":false,"stt_provider":"groq","stt_language":"Auto","stt_model":"whisper-large-v3-turbo","refinement_enabled":false,"refinement_provider":"groq","refinement_model":"openai/gpt-oss-120b","theme":"system","ui_language":"en"}"#;
        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.hotkey_ptt, "F5"); // migrated from old "hotkey"
        assert_eq!(settings.hotkey_toggle, "CommandOrControl+Shift+V"); // default
    }

    // -- TonePreset in Settings tests --

    #[test]
    fn should_default_tone_to_casual() {
        let settings = Settings::default();
        assert_eq!(settings.tone_preset, TonePreset::Casual);
    }

    #[test]
    fn should_roundtrip_tone_preset_in_settings() {
        let mut settings = Settings::default();
        settings.tone_preset = TonePreset::Professional;
        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.tone_preset, TonePreset::Professional);
    }

    #[test]
    fn should_deserialize_old_settings_without_tone() {
        // Old settings JSON without tone_preset field should get Casual default
        let json = r#"{"hotkey":"F5","auto_paste":true,"launch_at_login":false,"stt_provider":"groq","stt_language":"Auto","stt_model":"whisper-large-v3-turbo","refinement_enabled":false,"refinement_provider":"groq","refinement_model":"openai/gpt-oss-120b","theme":"system","ui_language":"en"}"#;
        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.tone_preset, TonePreset::Casual);
    }

    // -- custom_base_url tests --

    #[test]
    fn should_default_custom_base_url_to_empty() {
        let settings = Settings::default();
        assert_eq!(settings.custom_base_url, "");
    }

    #[test]
    fn should_deserialize_old_settings_without_custom_base_url() {
        // Old settings JSON without custom_base_url field should get empty string default
        let json = r#"{"hotkey":"F5","auto_paste":true,"launch_at_login":false,"stt_provider":"groq","stt_language":"Auto","stt_model":"whisper-large-v3-turbo","refinement_enabled":false,"refinement_provider":"groq","refinement_model":"openai/gpt-oss-120b","theme":"system","ui_language":"en"}"#;
        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.custom_base_url, "");
    }

    #[test]
    fn should_roundtrip_custom_base_url() {
        let mut settings = Settings::default();
        settings.custom_base_url = "http://localhost:11434/".to_string();
        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.custom_base_url, "http://localhost:11434/");
    }
}
