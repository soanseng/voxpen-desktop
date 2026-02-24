import { invoke } from "@tauri-apps/api/core";
import type { Settings } from "../types/settings";
import type { TranscriptionEntry } from "../types/history";

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
