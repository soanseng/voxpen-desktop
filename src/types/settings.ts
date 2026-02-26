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
};
