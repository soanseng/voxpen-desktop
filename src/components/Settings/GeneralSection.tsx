import { useTranslation } from "react-i18next";
import type { Settings } from "../../types/settings";

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

function SegmentedControl<T extends string>({
  options,
  value,
  onChange,
}: {
  options: { label: string; value: T }[];
  value: T;
  onChange: (value: T) => void;
}) {
  return (
    <div className="inline-flex rounded-lg bg-gray-200 p-1 dark:bg-gray-700">
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          onClick={() => onChange(option.value)}
          className={
            "rounded-md px-4 py-1.5 text-sm font-medium transition-colors " +
            (value === option.value
              ? "bg-white text-gray-900 shadow-sm dark:bg-gray-500 dark:text-white"
              : "text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-200")
          }
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

export default function GeneralSection({
  settings,
  onUpdate,
}: GeneralSectionProps) {
  const { t } = useTranslation();

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

      {/* Hotkey */}
      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
          {t("hotkey")}
        </label>
        <div
          className={
            "inline-block rounded-lg border border-gray-300 bg-gray-50 " +
            "px-4 py-2 font-mono text-sm text-gray-700 " +
            "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-300"
          }
        >
          {settings.hotkey}
        </div>
        <p className="text-xs text-gray-400 dark:text-gray-500">
          {t("hotkeyHint")}
        </p>
      </div>

      {/* Recording Mode */}
      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
          {t("recordingMode")}
        </label>
        <SegmentedControl
          options={[
            { label: t("holdToRecord"), value: "HoldToRecord" as const },
            { label: t("toggle"), value: "Toggle" as const },
          ]}
          value={settings.recording_mode}
          onChange={(v) => onUpdate("recording_mode", v)}
        />
        <p className="text-xs text-gray-400 dark:text-gray-500">
          {settings.recording_mode === "HoldToRecord"
            ? t("holdToRecordHint")
            : t("toggleHint")}
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
    </div>
  );
}
