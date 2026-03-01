export interface AppToneRule {
  app_pattern: string;
  tone: "Casual" | "Professional" | "Email" | "Note" | "Social" | "Custom";
}

export interface Settings {
  hotkey_ptt: string;
  hotkey_toggle: string;
  recording_mode: "HoldToRecord" | "Toggle";
  auto_paste: boolean;
  launch_at_login: boolean;
  stt_provider: string;
  stt_language: "Auto" | "Chinese" | "English" | "Japanese" | "Korean" | "French" | "German" | "Spanish" | "Vietnamese" | "Indonesian" | "Thai";
  stt_model: string;
  refinement_enabled: boolean;
  refinement_provider: string;
  refinement_model: string;
  refinement_prompt: string;
  tone_preset: "Casual" | "Professional" | "Email" | "Note" | "Social" | "Custom";
  custom_base_url: string;
  theme: "system" | "light" | "dark";
  ui_language: string;
  microphone_device: string | null;
  max_recording_secs: number;
  translation_enabled: boolean;
  translation_target: "Chinese" | "English" | "Japanese" | "Korean" | "French" | "German" | "Spanish" | "Vietnamese" | "Indonesian" | "Thai";
  voice_commands_enabled: boolean;
  hotkey_edit: string;
  app_tone_rules: AppToneRule[];
}

export type LicenseTier = "Free" | "Pro";

export interface LicenseInfo {
  tier: LicenseTier;
  license_key: string;
  instance_id: string;
  licensed_version: number;
  activated_at: number;
  last_verified_at: number;
  verification_grace_until: number | null;
}

export type UsageStatus =
  | { type: "Available"; data: { remaining: number } }
  | { type: "Warning"; data: { remaining: number } }
  | { type: "Exhausted" }
  | { type: "Unlimited" };

export interface CategorizedUsageStatus {
  voice_input: UsageStatus;
  refinement: UsageStatus;
  file_transcription: UsageStatus;
}

export const FREE_LIMITS = {
  VoiceInput: 30,
  Refinement: 10,
  FileTranscription: 2,
} as const;

export interface WhisperModel {
  id: string;
  tier: string;
  url: string;
  filename: string;
  size_bytes: number;
}

export type ModelStatus =
  | { status: "NotDownloaded" }
  | { status: "Downloading"; progress: number }
  | { status: "Ready"; size_bytes: number };

export interface FileTranscriptionResult {
  text: string;
  refined: string | null;
}

export const defaultSettings: Settings = {
  hotkey_ptt: "RAlt",
  hotkey_toggle: "CommandOrControl+Shift+V",
  recording_mode: "HoldToRecord",
  auto_paste: true,
  launch_at_login: false,
  stt_provider: "groq",
  stt_language: "Auto",
  stt_model: "whisper-large-v3-turbo",
  refinement_enabled: false,
  refinement_provider: "groq",
  refinement_model: "openai/gpt-oss-120b",
  refinement_prompt: "",
  tone_preset: "Casual",
  custom_base_url: "",
  theme: "system",
  ui_language: "en",
  microphone_device: null,
  max_recording_secs: 360,
  translation_enabled: false,
  translation_target: "English",
  voice_commands_enabled: false,
  hotkey_edit: "",
  app_tone_rules: [],
};
