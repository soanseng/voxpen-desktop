import { invoke } from "@tauri-apps/api/core";
import type { Settings } from "../types/settings";
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
): Promise<string> {
  return invoke<string>("get_default_refinement_prompt", { language });
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
