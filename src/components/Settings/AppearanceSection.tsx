import type { Settings } from "../../types/settings";

interface AppearanceSectionProps {
  settings: Settings;
  onUpdate: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
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

export default function AppearanceSection({
  settings,
  onUpdate,
}: AppearanceSectionProps) {
  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">
          Appearance
        </h2>
        <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
          Customize the look and language of VoxInk.
        </p>
      </div>

      {/* Theme */}
      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
          Theme
        </label>
        <SegmentedControl
          options={[
            { label: "System", value: "system" as const },
            { label: "Light", value: "light" as const },
            { label: "Dark", value: "dark" as const },
          ]}
          value={settings.theme}
          onChange={(v) => onUpdate("theme", v)}
        />
      </div>

      {/* UI Language */}
      <div className="space-y-2">
        <label
          htmlFor="ui-language"
          className="block text-sm font-medium text-gray-700 dark:text-gray-300"
        >
          Interface Language
        </label>
        <select
          id="ui-language"
          value={settings.ui_language}
          onChange={(e) => onUpdate("ui_language", e.target.value)}
          className={
            "w-full max-w-xs rounded-lg border border-gray-300 bg-white " +
            "px-3 py-2 text-sm text-gray-900 " +
            "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
            "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
          }
        >
          <option value="en">English</option>
          <option value="zh-TW">繁體中文</option>
        </select>
      </div>
    </div>
  );
}
