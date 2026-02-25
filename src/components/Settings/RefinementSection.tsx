import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Settings } from "../../types/settings";
import { saveApiKey } from "../../lib/tauri";

interface RefinementSectionProps {
  settings: Settings;
  onUpdate: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

const REFINEMENT_PROVIDERS = [
  { value: "groq", label: "Groq" },
  { value: "openai", label: "OpenAI" },
  { value: "anthropic", label: "Anthropic" },
  { value: "custom", label: "Custom Server" },
];

function getModelsForProvider(provider: string) {
  switch (provider) {
    case "groq":
      return [
        { value: "openai/gpt-oss-120b", label: "openai/gpt-oss-120b" },
        { value: "openai/gpt-oss-20b", label: "openai/gpt-oss-20b" },
      ];
    case "openai":
      return [
        { value: "gpt-4o", label: "GPT-4o" },
        { value: "gpt-4o-mini", label: "GPT-4o mini" },
      ];
    case "anthropic":
      return [
        { value: "claude-sonnet-4-5-20250514", label: "Claude Sonnet 4.5" },
        { value: "claude-haiku-4-5-20250514", label: "Claude Haiku 4.5" },
      ];
    default:
      return [];
  }
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

export default function RefinementSection({
  settings,
  onUpdate,
}: RefinementSectionProps) {
  const { t } = useTranslation();
  const [apiKey, setApiKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveStatus, setSaveStatus] = useState<"idle" | "saved" | "error">(
    "idle",
  );

  const models = getModelsForProvider(settings.refinement_provider);
  const disabled = !settings.refinement_enabled;
  const sameProvider = settings.refinement_provider === settings.stt_provider;

  async function handleSaveKey() {
    if (!apiKey.trim()) return;
    setSaving(true);
    setSaveStatus("idle");
    try {
      await saveApiKey(settings.refinement_provider, apiKey.trim());
      setSaveStatus("saved");
      setApiKey("");
      setTimeout(() => setSaveStatus("idle"), 2000);
    } catch {
      setSaveStatus("error");
      setTimeout(() => setSaveStatus("idle"), 3000);
    } finally {
      setSaving(false);
    }
  }

  function handleProviderChange(provider: string) {
    onUpdate("refinement_provider", provider);
    const newModels = getModelsForProvider(provider);
    if (newModels.length > 0) {
      onUpdate("refinement_model", newModels[0].value);
    }
  }

  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">
          {t("refinement")}
        </h2>
        <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
          {t("refinementDescription")}
        </p>
      </div>

      {/* Enable toggle */}
      <div className="flex items-center justify-between">
        <div>
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            {t("enable")}
          </label>
          <p className="text-xs text-gray-400 dark:text-gray-500">
            {t("enableHint")}
          </p>
        </div>
        <ToggleSwitch
          id="refinement-enabled"
          checked={settings.refinement_enabled}
          onChange={(v) => onUpdate("refinement_enabled", v)}
        />
      </div>

      {/* Provider */}
      <div className={`space-y-2 ${disabled ? "opacity-40" : ""}`}>
        <label
          htmlFor="refinement-provider"
          className="block text-sm font-medium text-gray-700 dark:text-gray-300"
        >
          {t("provider")}
        </label>
        <select
          id="refinement-provider"
          value={settings.refinement_provider}
          onChange={(e) => handleProviderChange(e.target.value)}
          disabled={disabled}
          className={
            "w-full max-w-xs rounded-lg border border-gray-300 bg-white " +
            "px-3 py-2 text-sm text-gray-900 " +
            "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
            "disabled:cursor-not-allowed " +
            "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
          }
        >
          {REFINEMENT_PROVIDERS.map((p) => (
            <option key={p.value} value={p.value}>
              {p.label}
            </option>
          ))}
        </select>
      </div>

      {/* API Key — hidden when same provider as STT (shared key) */}
      {sameProvider ? (
        <div className={`space-y-2 ${disabled ? "opacity-40" : ""}`}>
          <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
            {t("apiKey")}
          </label>
          <p className="text-sm text-gray-500 dark:text-gray-400">
            {t("sharedApiKey")}
          </p>
        </div>
      ) : (
        <div className={`space-y-2 ${disabled ? "opacity-40" : ""}`}>
          <label
            htmlFor="refinement-api-key"
            className="block text-sm font-medium text-gray-700 dark:text-gray-300"
          >
            {t("apiKey")}
          </label>
          <div className="flex max-w-md items-center gap-2">
            <div className="relative flex-1">
              <input
                id="refinement-api-key"
                type={showKey ? "text" : "password"}
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder={t("apiKeyPlaceholder")}
                disabled={disabled}
                className={
                  "w-full rounded-lg border border-gray-300 bg-white " +
                  "px-3 py-2 pr-10 text-sm text-gray-900 " +
                  "placeholder:text-gray-400 " +
                  "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
                  "disabled:cursor-not-allowed " +
                  "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100 " +
                  "dark:placeholder:text-gray-500"
                }
              />
              <button
                type="button"
                onClick={() => setShowKey(!showKey)}
                disabled={disabled}
                className={
                  "absolute right-2 top-1/2 -translate-y-1/2 " +
                  "text-gray-400 hover:text-gray-600 " +
                  "disabled:cursor-not-allowed " +
                  "dark:hover:text-gray-300"
                }
                aria-label={showKey ? "Hide API key" : "Show API key"}
              >
                {showKey ? (
                  <svg
                    className="h-4 w-4"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    strokeWidth={2}
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.878 9.878L6.59 6.59m7.532 7.532l3.29 3.29M3 3l18 18"
                    />
                  </svg>
                ) : (
                  <svg
                    className="h-4 w-4"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    strokeWidth={2}
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
                    />
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"
                    />
                  </svg>
                )}
              </button>
            </div>
            <button
              type="button"
              onClick={() => void handleSaveKey()}
              disabled={disabled || saving || !apiKey.trim()}
              className={
                "rounded-lg bg-blue-500 px-4 py-2 text-sm font-medium text-white " +
                "transition-colors hover:bg-blue-600 " +
                "disabled:cursor-not-allowed disabled:opacity-50 " +
                "dark:bg-blue-600 dark:hover:bg-blue-700"
              }
            >
              {saving ? t("saving") : t("save")}
            </button>
          </div>
          {saveStatus === "saved" && (
            <p className="text-xs text-green-600 dark:text-green-400">
              {t("saved")}
            </p>
          )}
          {saveStatus === "error" && (
            <p className="text-xs text-red-600 dark:text-red-400">
              {t("saveFailed")}
            </p>
          )}
        </div>
      )}

      {/* Model */}
      {models.length > 0 && (
        <div className={`space-y-2 ${disabled ? "opacity-40" : ""}`}>
          <label
            htmlFor="refinement-model"
            className="block text-sm font-medium text-gray-700 dark:text-gray-300"
          >
            {t("model")}
          </label>
          <select
            id="refinement-model"
            value={settings.refinement_model}
            onChange={(e) => onUpdate("refinement_model", e.target.value)}
            disabled={disabled}
            className={
              "w-full max-w-xs rounded-lg border border-gray-300 bg-white " +
              "px-3 py-2 text-sm text-gray-900 " +
              "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
              "disabled:cursor-not-allowed " +
              "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
            }
          >
            {models.map((m) => (
              <option key={m.value} value={m.value}>
                {m.label}
              </option>
            ))}
          </select>
        </div>
      )}
    </div>
  );
}
