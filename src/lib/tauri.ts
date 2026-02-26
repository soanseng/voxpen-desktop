import { invoke } from "@tauri-apps/api/core";
import type { Settings, LicenseInfo, LicenseTier, UsageStatus } from "../types/settings";
import type { TranscriptionEntry } from "../types/history";
import type { DictionaryEntry } from "../types/dictionary";

export async function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

export async function saveSettings(settings: Settings): Promise<void> {
  return invoke("save_settings", { settings });
}

export async function saveApiKey(
  provider: string,
  key: string,
): Promise<void> {
  return invoke("save_api_key", { provider, key });
}

export async function getApiKeyStatus(
  provider: string,
): Promise<string | null> {
  return invoke<string | null>("get_api_key_status", { provider });
}

export async function checkMicrophone(): Promise<string> {
  return invoke<string>("check_microphone");
}

export async function testApiKey(
  provider: string,
  key: string,
): Promise<boolean> {
  return invoke<boolean>("test_api_key", { provider, key });
}

export async function getHistory(
  limit: number,
  offset: number,
): Promise<TranscriptionEntry[]> {
  return invoke<TranscriptionEntry[]>("get_history", { limit, offset });
}

export async function deleteHistoryEntry(id: string): Promise<void> {
  return invoke("delete_history_entry", { id });
}

export async function searchHistory(
  query: string,
  limit: number,
  offset: number,
): Promise<TranscriptionEntry[]> {
  return invoke<TranscriptionEntry[]>("search_history", {
    query,
    limit,
    offset,
  });
}

export async function getDefaultRefinementPrompt(
  language: string,
  tone: string,
): Promise<string> {
  return invoke<string>("get_default_refinement_prompt", { language, tone });
}

export async function getDictionaryEntries(): Promise<DictionaryEntry[]> {
  return invoke<DictionaryEntry[]>("get_dictionary_entries");
}

export async function getDictionaryCount(): Promise<number> {
  return invoke<number>("get_dictionary_count");
}

export async function addDictionaryEntry(word: string): Promise<void> {
  return invoke("add_dictionary_entry", { word });
}

export async function deleteDictionaryEntry(id: number): Promise<void> {
  return invoke("delete_dictionary_entry", { id });
}

export async function listInputDevices(): Promise<string[]> {
  return invoke<string[]>("list_input_devices");
}

export interface UpdateInfo {
  current: string;
  latest: string;
  update_available: boolean;
  download_url: string;
}

export async function checkForUpdate(): Promise<UpdateInfo> {
  return invoke<UpdateInfo>("check_for_update");
}

export async function setHotkey(shortcut: string, kind: string): Promise<void> {
  return invoke("set_hotkey", { shortcut, kind });
}

export async function activateLicense(key: string): Promise<LicenseInfo> {
  return invoke<LicenseInfo>("activate_license", { key });
}

export async function deactivateLicense(): Promise<void> {
  return invoke("deactivate_license");
}

export async function getLicenseInfo(): Promise<LicenseInfo | null> {
  return invoke<LicenseInfo | null>("get_license_info");
}

export async function getUsageStatus(): Promise<UsageStatus> {
  return invoke<UsageStatus>("get_usage_status");
}

export async function getLicenseTier(): Promise<LicenseTier> {
  return invoke<LicenseTier>("get_license_tier");
}
