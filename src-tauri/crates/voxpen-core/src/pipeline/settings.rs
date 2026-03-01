use serde::{Deserialize, Serialize};

use crate::pipeline::state::{Language, RecordingMode, TonePreset};

/// A rule that maps an app name pattern to a tone preset.
/// The pattern is matched case-insensitively as a substring of the active app name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppToneRule {
    /// Case-insensitive substring pattern matched against the active app name.
    pub app_pattern: String,
    /// Tone preset to apply when this rule matches.
    pub tone: TonePreset,
}

/// Find the first matching tone preset for the given app name.
/// Matching is case-insensitive substring: rule fires if `app_name` contains `app_pattern`.
pub fn find_matching_tone(rules: &[AppToneRule], app_name: &str) -> Option<TonePreset> {
    let lower = app_name.to_lowercase();
    rules
        .iter()
        .find(|r| lower.contains(&r.app_pattern.to_lowercase()))
        .map(|r| r.tone.clone())
}

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
    /// Maximum recording duration in seconds. Recording auto-stops when exceeded.
    /// Default: 360 (6 minutes). Set to 0 to disable the limit.
    #[serde(default = "default_max_recording_secs")]
    pub max_recording_secs: u32,
    /// Whether to translate speech to `translation_target` language instead of
    /// cleaning it up. Has no effect when `refinement_enabled` is false
    /// (requires LLM pipeline to be active).
    #[serde(default)]
    pub translation_enabled: bool,
    /// Target language for translation mode.
    /// Default: English (most common translation target for CJK users).
    #[serde(default = "default_translation_target")]
    pub translation_target: Language,
    /// Whether to replace spoken formatting keywords with punctuation characters.
    /// E.g. "comma" → "," and "new line" → "\n". Applied after STT, before LLM refinement.
    /// Default: false.
    #[serde(default)]
    pub voice_commands_enabled: bool,
    /// Hotkey for "Voice Edit" mode: user selects text, presses this hotkey,
    /// speaks an edit command, and the selected text is replaced with the
    /// LLM-edited result. Only combo shortcuts supported (e.g. "CommandOrControl+Shift+E").
    /// Default: empty string (feature disabled).
    #[serde(default)]
    pub hotkey_edit: String,
    /// Rules for auto-selecting a tone preset based on the active application.
    /// First matching rule wins. Empty = feature disabled.
    #[serde(default)]
    pub app_tone_rules: Vec<AppToneRule>,
}

fn default_hotkey_toggle() -> String {
    "CommandOrControl+Shift+V".to_string()
}

fn default_max_recording_secs() -> u32 {
    360
}

fn default_translation_target() -> Language {
    Language::English
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
            max_recording_secs: default_max_recording_secs(),
            translation_enabled: false,
            translation_target: default_translation_target(),
            voice_commands_enabled: false,
            hotkey_edit: String::new(),
            app_tone_rules: Vec::new(),
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

    // -- max_recording_secs tests --

    #[test]
    fn should_default_max_recording_secs_to_360() {
        let settings = Settings::default();
        assert_eq!(settings.max_recording_secs, 360);
    }

    #[test]
    fn should_roundtrip_max_recording_secs() {
        let mut settings = Settings::default();
        settings.max_recording_secs = 120;
        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.max_recording_secs, 120);
    }

    #[test]
    fn should_deserialize_old_settings_without_max_recording_secs() {
        let json = r#"{"hotkey_ptt":"RAlt","hotkey_toggle":"CommandOrControl+Shift+V","recording_mode":"HoldToRecord","auto_paste":true,"launch_at_login":false,"stt_provider":"groq","stt_language":"Auto","stt_model":"whisper-large-v3-turbo","refinement_enabled":false,"refinement_provider":"groq","refinement_model":"openai/gpt-oss-120b","theme":"system","ui_language":"en"}"#;
        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.max_recording_secs, 360); // gets default
    }

    // -- translation_enabled / translation_target tests --

    #[test]
    fn should_default_translation_enabled_to_false() {
        let s = Settings::default();
        assert!(!s.translation_enabled);
    }

    #[test]
    fn should_default_translation_target_to_english() {
        let s = Settings::default();
        assert_eq!(s.translation_target, Language::English);
    }

    #[test]
    fn should_deserialize_old_settings_without_translation_fields() {
        let json = r#"{"hotkey_ptt":"RAlt","hotkey_toggle":"CommandOrControl+Shift+V","recording_mode":"HoldToRecord","auto_paste":true,"launch_at_login":false,"stt_provider":"groq","stt_language":"Auto","stt_model":"whisper-large-v3-turbo","refinement_enabled":false,"refinement_provider":"groq","refinement_model":"openai/gpt-oss-120b","theme":"system","ui_language":"en"}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert!(!s.translation_enabled);
        assert_eq!(s.translation_target, Language::English);
    }

    #[test]
    fn should_roundtrip_translation_fields() {
        let mut s = Settings::default();
        s.translation_enabled = true;
        s.translation_target = Language::Chinese;
        let json = serde_json::to_string(&s).unwrap();
        let s2: Settings = serde_json::from_str(&json).unwrap();
        assert!(s2.translation_enabled);
        assert_eq!(s2.translation_target, Language::Chinese);
    }

    // -- voice_commands_enabled tests --

    #[test]
    fn should_default_voice_commands_enabled_to_false() {
        let s = Settings::default();
        assert!(!s.voice_commands_enabled);
    }

    #[test]
    fn should_deserialize_old_settings_without_voice_commands() {
        // Old JSON without voice_commands_enabled should deserialize with false
        let json = r#"{"hotkey_ptt":"RAlt","hotkey_toggle":"CommandOrControl+Shift+V","recording_mode":"HoldToRecord","auto_paste":true,"launch_at_login":false,"stt_provider":"groq","stt_language":"Auto","stt_model":"whisper-large-v3-turbo","refinement_enabled":false,"refinement_provider":"groq","refinement_model":"openai/gpt-oss-120b","theme":"system","ui_language":"en"}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert!(!s.voice_commands_enabled);
    }

    // -- hotkey_edit tests --

    #[test]
    fn should_default_hotkey_edit_to_empty() {
        let s = Settings::default();
        assert_eq!(s.hotkey_edit, "");
    }

    #[test]
    fn should_roundtrip_hotkey_edit() {
        let mut s = Settings::default();
        s.hotkey_edit = "CommandOrControl+Shift+E".to_string();
        let json = serde_json::to_string(&s).unwrap();
        let s2: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s2.hotkey_edit, "CommandOrControl+Shift+E");
    }

    #[test]
    fn should_deserialize_old_settings_without_hotkey_edit() {
        // Old JSON without hotkey_edit should get empty string default
        let json = r#"{"hotkey_ptt":"RAlt","hotkey_toggle":"CommandOrControl+Shift+V","recording_mode":"HoldToRecord","auto_paste":true,"launch_at_login":false,"stt_provider":"groq","stt_language":"Auto","stt_model":"whisper-large-v3-turbo","refinement_enabled":false,"refinement_provider":"groq","refinement_model":"openai/gpt-oss-120b","theme":"system","ui_language":"en"}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.hotkey_edit, "");
    }

    // -- AppToneRule tests --

    #[test]
    fn should_serialize_app_tone_rule() {
        let rule = AppToneRule {
            app_pattern: "slack".to_string(),
            tone: TonePreset::Professional,
        };
        let json = serde_json::to_string(&rule).unwrap();
        assert!(json.contains("slack"));
        assert!(json.contains("Professional"));
    }

    #[test]
    fn should_find_matching_tone_case_insensitive() {
        let rules = vec![
            AppToneRule { app_pattern: "slack".to_string(), tone: TonePreset::Casual },
            AppToneRule { app_pattern: "outlook".to_string(), tone: TonePreset::Email },
        ];
        assert_eq!(find_matching_tone(&rules, "SLACK"), Some(TonePreset::Casual));
        assert_eq!(find_matching_tone(&rules, "Microsoft Outlook"), Some(TonePreset::Email));
    }

    #[test]
    fn should_return_none_when_no_rule_matches() {
        let rules = vec![
            AppToneRule { app_pattern: "slack".to_string(), tone: TonePreset::Casual },
        ];
        assert_eq!(find_matching_tone(&rules, "firefox"), None);
    }

    #[test]
    fn should_return_none_for_empty_rules() {
        assert_eq!(find_matching_tone(&[], "any_app"), None);
    }

    #[test]
    fn should_default_app_tone_rules_to_empty() {
        let s = Settings::default();
        assert!(s.app_tone_rules.is_empty());
    }

    #[test]
    fn should_roundtrip_app_tone_rules() {
        let mut s = Settings::default();
        s.app_tone_rules = vec![
            AppToneRule { app_pattern: "code".to_string(), tone: TonePreset::Note },
        ];
        let json = serde_json::to_string(&s).unwrap();
        let s2: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s2.app_tone_rules.len(), 1);
        assert_eq!(s2.app_tone_rules[0].app_pattern, "code");
        assert_eq!(s2.app_tone_rules[0].tone, TonePreset::Note);
    }

    #[test]
    fn should_deserialize_old_settings_without_app_tone_rules() {
        let json = r#"{"hotkey_ptt":"RAlt","hotkey_toggle":"CommandOrControl+Shift+V","recording_mode":"HoldToRecord","auto_paste":true,"launch_at_login":false,"stt_provider":"groq","stt_language":"Auto","stt_model":"whisper-large-v3-turbo","refinement_enabled":false,"refinement_provider":"groq","refinement_model":"openai/gpt-oss-120b","theme":"system","ui_language":"en"}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert!(s.app_tone_rules.is_empty());
    }
}
