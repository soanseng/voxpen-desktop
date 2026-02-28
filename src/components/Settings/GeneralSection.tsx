import { useState, useRef, useEffect } from "react";
import { useTranslation } from "react-i18next";
import type { Settings } from "../../types/settings";
import { setHotkey, listInputDevices } from "../../lib/tauri";

interface GeneralSectionProps {
  settings: Settings;
  onUpdate: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

function ToggleSwitch({
  checked,
  onChange,
  id,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  id: string;
}) {
  return (
    <label htmlFor={id} className="relative inline-flex cursor-pointer items-center">
      <input
        id={id}
        type="checkbox"
        className="peer sr-only"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      <div
        className={
          "h-6 w-11 rounded-full bg-gray-300 transition-colors " +
          "after:absolute after:left-[2px] after:top-[2px] after:h-5 after:w-5 " +
          "after:rounded-full after:bg-white after:transition-transform " +
          "peer-checked:bg-blue-500 peer-checked:after:translate-x-5 " +
          "dark:bg-gray-600 dark:peer-checked:bg-blue-500"
        }
      />
    </label>
  );
}

/** Map browser KeyboardEvent.code to our shortcut string format. */
function formatKeyEvent(e: React.KeyboardEvent): string {
  e.preventDefault();
  e.stopPropagation();

  // Standalone modifier keys -> single-key shortcuts for rdev
  const standaloneModifiers: Record<string, string> = {
    AltRight: "RAlt",
    AltLeft: "LAlt",
    ControlRight: "RControl",
    ControlLeft: "LControl",
    ShiftRight: "RShift",
    ShiftLeft: "LShift",
  };

  if (standaloneModifiers[e.code]) {
    return standaloneModifiers[e.code];
  }

  // Build combo shortcut for Tauri global-shortcut
  const parts: string[] = [];
  if (e.ctrlKey || e.metaKey) parts.push("CommandOrControl");
  if (e.shiftKey) parts.push("Shift");
  if (e.altKey) parts.push("Alt");

  const keyMap: Record<string, string> = {
    KeyA: "A", KeyB: "B", KeyC: "C", KeyD: "D", KeyE: "E",
    KeyF: "F", KeyG: "G", KeyH: "H", KeyI: "I", KeyJ: "J",
    KeyK: "K", KeyL: "L", KeyM: "M", KeyN: "N", KeyO: "O",
    KeyP: "P", KeyQ: "Q", KeyR: "R", KeyS: "S", KeyT: "T",
    KeyU: "U", KeyV: "V", KeyW: "W", KeyX: "X", KeyY: "Y",
    KeyZ: "Z",
    Digit0: "0", Digit1: "1", Digit2: "2", Digit3: "3",
    Digit4: "4", Digit5: "5", Digit6: "6", Digit7: "7",
    Digit8: "8", Digit9: "9",
    Space: "Space", Enter: "Enter", Escape: "Escape",
    F1: "F1", F2: "F2", F3: "F3", F4: "F4", F5: "F5",
    F6: "F6", F7: "F7", F8: "F8", F9: "F9", F10: "F10",
    F11: "F11", F12: "F12",
  };

  const key = keyMap[e.code];
  if (key && parts.length > 0) {
    parts.push(key);
    return parts.join("+");
  }

  // Single non-modifier key (e.g., F5) without modifiers
  if (key && parts.length === 0) {
    return key;
  }

  return "";
}

/** Human-readable display label for a shortcut string. */
function displayShortcut(shortcut: string): string {
  const labels: Record<string, string> = {
    RAlt: "Right Alt",
    LAlt: "Left Alt",
    RControl: "Right Ctrl",
    LControl: "Left Ctrl",
    RShift: "Right Shift",
    LShift: "Left Shift",
  };
  return labels[shortcut] ?? shortcut;
}

function HotkeyPicker({
  label,
  hint,
  currentValue,
  kind,
  onSaved,
  t,
}: {
  label: string;
  hint: string;
  currentValue: string;
  kind: "ptt" | "toggle";
  onSaved: (shortcut: string) => void;
  t: (key: string, opts?: Record<string, string>) => string;
}) {
  const [mode, setMode] = useState<"display" | "recording">("display");
  const [pending, setPending] = useState("");
  const [error, setError] = useState("");
  const recorderRef = useRef<HTMLDivElement>(null);

  async function save(shortcut: string) {
    try {
      await setHotkey(shortcut, kind);
      onSaved(shortcut);
      setMode("display");
      setError("");
    } catch (err) {
      setError(String(err));
    }
  }

  return (
    <div className="space-y-2">
      <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
        {label}
      </label>

      {mode === "display" ? (
        <div className="flex items-center gap-3">
          <div
            className={
              "inline-block rounded-lg border border-gray-300 bg-gray-50 " +
              "px-4 py-2 font-mono text-sm text-gray-700 " +
              "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-300"
            }
          >
            {displayShortcut(currentValue)}
          </div>
          <button
            type="button"
            onClick={() => {
              setMode("recording");
              setPending("");
              setError("");
              setTimeout(() => recorderRef.current?.focus(), 50);
            }}
            className="rounded-lg bg-blue-500 px-3 py-2 text-sm font-medium text-white hover:bg-blue-600"
          >
            {t("hotkeyChange")}
          </button>
        </div>
      ) : (
        <div className="space-y-3">
          <div
            ref={recorderRef}
            tabIndex={0}
            onKeyDown={(e) => {
              const shortcut = formatKeyEvent(e);
              if (shortcut) setPending(shortcut);
            }}
            className={
              "flex h-12 items-center rounded-lg border-2 border-blue-500 " +
              "bg-blue-50 px-4 font-mono text-sm text-gray-700 outline-none " +
              "dark:bg-blue-900/20 dark:text-gray-300"
            }
          >
            {pending ? displayShortcut(pending) : t("hotkeyRecording")}
          </div>

          {/* Presets */}
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => { setPending("RAlt"); void save("RAlt"); }}
              className={
                "rounded-md border border-gray-300 px-3 py-1.5 text-xs " +
                "text-gray-600 hover:bg-gray-100 " +
                "dark:border-gray-600 dark:text-gray-400 dark:hover:bg-gray-700"
              }
            >
              {t("hotkeyPresetAlt")}
            </button>
            <button
              type="button"
              onClick={() => { setPending("CommandOrControl+Shift+V"); void save("CommandOrControl+Shift+V"); }}
              className={
                "rounded-md border border-gray-300 px-3 py-1.5 text-xs " +
                "text-gray-600 hover:bg-gray-100 " +
                "dark:border-gray-600 dark:text-gray-400 dark:hover:bg-gray-700"
              }
            >
              {t("hotkeyPresetCombo")}
            </button>
          </div>

          {/* Actions */}
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => pending && void save(pending)}
              disabled={!pending}
              className={
                "rounded-lg bg-blue-500 px-3 py-1.5 text-sm font-medium " +
                "text-white hover:bg-blue-600 disabled:opacity-50"
              }
            >
              {t("hotkeySave")}
            </button>
            <button
              type="button"
              onClick={() => {
                setMode("display");
                setError("");
              }}
              className={
                "rounded-lg border border-gray-300 px-3 py-1.5 text-sm " +
                "text-gray-600 hover:bg-gray-100 " +
                "dark:border-gray-600 dark:text-gray-400 dark:hover:bg-gray-700"
              }
            >
              {t("hotkeyCancel")}
            </button>
          </div>

          {error && (
            <p className="text-xs text-red-500">
              {t("hotkeyError", { error })}
            </p>
          )}
        </div>
      )}

      <p className="text-xs text-gray-400 dark:text-gray-500">{hint}</p>
    </div>
  );
}

export default function GeneralSection({
  settings,
  onUpdate,
}: GeneralSectionProps) {
  const { t } = useTranslation();
  const [micDevices, setMicDevices] = useState<string[]>([]);

  useEffect(() => {
    listInputDevices()
      .then(setMicDevices)
      .catch(() => setMicDevices([]));
  }, []);

  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">
          {t("general")}
        </h2>
        <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
          {t("generalDescription")}
        </p>
      </div>

      {/* Push-to-Talk Hotkey */}
      <HotkeyPicker
        label={t("hotkeyPtt")}
        hint={t("hotkeyPttHint")}
        currentValue={settings.hotkey_ptt}
        kind="ptt"
        onSaved={(s) => onUpdate("hotkey_ptt", s)}
        t={t}
      />

      {/* Hands-free / Toggle Hotkey */}
      <HotkeyPicker
        label={t("hotkeyToggle")}
        hint={t("hotkeyToggleHint")}
        currentValue={settings.hotkey_toggle}
        kind="toggle"
        onSaved={(s) => onUpdate("hotkey_toggle", s)}
        t={t}
      />

      {/* Microphone device */}
      <div className="space-y-2">
        <label
          htmlFor="mic-device"
          className="block text-sm font-medium text-gray-700 dark:text-gray-300"
        >
          {t("micDevice")}
        </label>
        <select
          id="mic-device"
          value={settings.microphone_device ?? ""}
          onChange={(e) =>
            onUpdate("microphone_device", e.target.value || null)
          }
          className={
            "w-full max-w-xs rounded-lg border border-gray-300 bg-white " +
            "px-3 py-2 text-sm text-gray-900 " +
            "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
            "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
          }
        >
          <option value="">{t("micDeviceDefault")}</option>
          {micDevices.map((name) => (
            <option key={name} value={name}>
              {name}
            </option>
          ))}
        </select>
        <p className="text-xs text-gray-400 dark:text-gray-500">
          {t("micDeviceHint")}
        </p>
      </div>

      {/* Auto-paste */}
      <div className="flex items-center justify-between">
        <div>
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            {t("autoPaste")}
          </label>
          <p className="text-xs text-gray-400 dark:text-gray-500">
            {t("autoPasteHint")}
          </p>
        </div>
        <ToggleSwitch
          id="auto-paste"
          checked={settings.auto_paste}
          onChange={(v) => onUpdate("auto_paste", v)}
        />
      </div>

      {/* Launch at login */}
      <div className="flex items-center justify-between">
        <div>
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            {t("launchAtLogin")}
          </label>
          <p className="text-xs text-gray-400 dark:text-gray-500">
            {t("launchAtLoginHint")}
          </p>
        </div>
        <ToggleSwitch
          id="launch-at-login"
          checked={settings.launch_at_login}
          onChange={(v) => onUpdate("launch_at_login", v)}
        />
      </div>

      {/* Max recording duration */}
      <div className="flex items-center justify-between">
        <div>
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            最長錄音時間
          </label>
          <p className="text-xs text-gray-400 dark:text-gray-500">
            Max Recording Duration (0 = unlimited)
          </p>
        </div>
        <div className="flex items-center gap-1">
          <input
            type="number"
            min={0}
            max={3600}
            step={30}
            value={settings.max_recording_secs}
            onChange={(e) =>
              onUpdate("max_recording_secs", parseInt(e.target.value) || 0)
            }
            className={
              "w-20 rounded-lg border border-gray-300 bg-white px-2 py-1 text-sm text-right " +
              "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
              "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
            }
          />
          <span className="text-xs text-gray-400 dark:text-gray-500">秒</span>
        </div>
      </div>

      {/* Voice Commands */}
      <div className="flex items-center justify-between">
        <div>
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            {t("voiceCommandsEnabled")}
          </label>
          <p className="text-xs text-gray-400 dark:text-gray-500">
            {t("voiceCommandsEnabledHint")}
          </p>
        </div>
        <ToggleSwitch
          id="voice-commands-enabled"
          checked={settings.voice_commands_enabled}
          onChange={(v) => onUpdate("voice_commands_enabled", v)}
        />
      </div>
    </div>
  );
}
