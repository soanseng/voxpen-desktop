import { useCallback, useEffect, useRef, useState } from "react";
import type { Settings } from "../types/settings";
import { defaultSettings } from "../types/settings";
import { getSettings, saveSettings } from "../lib/tauri";

interface UseSettingsResult {
  settings: Settings;
  updateSetting: <K extends keyof Settings>(
    key: K,
    value: Settings[K],
  ) => void;
  loading: boolean;
}

export function useSettings(): UseSettingsResult {
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [loading, setLoading] = useState(true);
  const debounceTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingSettings = useRef<Settings>(defaultSettings);

  useEffect(() => {
    let cancelled = false;

    async function load() {
      try {
        const loaded = await getSettings();
        if (!cancelled) {
          setSettings(loaded);
          pendingSettings.current = loaded;
        }
      } catch {
        // Tauri backend not available (e.g. during dev), keep defaults
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    load();

    return () => {
      cancelled = true;
    };
  }, []);

  const updateSetting = useCallback(
    <K extends keyof Settings>(key: K, value: Settings[K]) => {
      const next = { ...pendingSettings.current, [key]: value };
      pendingSettings.current = next;
      setSettings(next);

      if (debounceTimer.current !== null) {
        clearTimeout(debounceTimer.current);
      }

      debounceTimer.current = setTimeout(() => {
        debounceTimer.current = null;
        saveSettings(pendingSettings.current).catch(() => {
          // Save failed silently; settings remain in local state
        });
      }, 300);
    },
    [],
  );

  return { settings, updateSetting, loading };
}
