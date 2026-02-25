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
  theme: "system" | "light" | "dark";
  ui_language: string;
  microphone_device: string | null;
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
  theme: "system",
  ui_language: "en",
  microphone_device: null,
};
