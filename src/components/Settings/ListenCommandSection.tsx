import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Settings } from "../../types/settings";
import { getApiKeyStatus, saveApiKey } from "../../lib/tauri";

interface ListenCommandSectionProps {
  settings: Settings;
  onUpdate: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

const COMMAND_PROVIDERS = [
  { value: "openai", label: "OpenAI" },
  { value: "groq", label: "Groq" },
  { value: "openrouter", label: "OpenRouter" },
  { value: "custom", label: "Custom OpenAI-compatible" },
];

interface ModelOption {
  value: string;
  label: string;
  tag?: "recommended" | "budget" | "multilingual" | "quality";
}

function getModelsForProvider(provider: string): ModelOption[] {
  switch (provider) {
    case "openai":
      return [
        { value: "gpt-5.2", label: "GPT-5.2", tag: "recommended" },
        { value: "gpt-5.1", label: "GPT-5.1", tag: "quality" },
        { value: "gpt-5", label: "GPT-5" },
        { value: "gpt-5-mini", label: "GPT-5 Mini", tag: "budget" },
        { value: "gpt-5-nano", label: "GPT-5 Nano", tag: "budget" },
      ];
    case "groq":
      return [
        { value: "openai/gpt-oss-120b", label: "GPT-OSS 120B", tag: "recommended" },
        { value: "openai/gpt-oss-20b", label: "GPT-OSS 20B", tag: "budget" },
        { value: "qwen/qwen3-32b", label: "Qwen3 32B", tag: "multilingual" },
      ];
    case "openrouter":
      return [
        { value: "openai/gpt-5.2", label: "OpenAI GPT-5.2", tag: "recommended" },
        { value: "anthropic/claude-haiku-4.5", label: "Claude Haiku 4.5", tag: "quality" },
        { value: "google/gemini-3-flash", label: "Gemini 3 Flash", tag: "budget" },
      ];
    default:
      return [];
  }
}

export default function ListenCommandSection({
  settings,
  onUpdate,
}: ListenCommandSectionProps) {
  const { t } = useTranslation();
  const [apiKey, setApiKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveStatus, setSaveStatus] = useState<"idle" | "saved" | "error">("idle");
  const [keyStatus, setKeyStatus] = useState<string | null>(null);

  const disabled = !settings.listen_command_enabled;
  const models = getModelsForProvider(settings.listen_command_provider);
  const selectedPreset = models.some((model) => model.value === settings.listen_command_model)
    ? settings.listen_command_model
    : "";

  useEffect(() => {
    getApiKeyStatus(settings.listen_command_provider)
      .then(setKeyStatus)
      .catch(() => setKeyStatus(null));
  }, [settings.listen_command_provider]);

  function handleProviderChange(provider: string) {
    onUpdate("listen_command_provider", provider);
    const nextModels = getModelsForProvider(provider);
    onUpdate("listen_command_model", nextModels[0]?.value ?? "");
  }

  async function handleSaveKey() {
    if (!apiKey.trim()) return;
    setSaving(true);
    setSaveStatus("idle");
    try {
      await saveApiKey(settings.listen_command_provider, apiKey.trim());
      setSaveStatus("saved");
      setApiKey("");
      getApiKeyStatus(settings.listen_command_provider)
        .then(setKeyStatus)
        .catch(() => {});
      setTimeout(() => setSaveStatus("idle"), 2000);
    } catch {
      setSaveStatus("error");
      setTimeout(() => setSaveStatus("idle"), 3000);
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">
          {t("listenCommandTab")}
        </h2>
        <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
          {t("listenCommandDescription")}
        </p>
      </div>

      <div className={disabled ? "space-y-6 opacity-40" : "space-y-6"}>
        <div className="space-y-2">
          <label
            htmlFor="listen-command-provider"
            className="block text-sm font-medium text-gray-700 dark:text-gray-300"
          >
            {t("listenCommandProvider")}
          </label>
          <p className="text-xs text-gray-400 dark:text-gray-500">
            {t("listenCommandProviderHint")}
          </p>
          <select
            id="listen-command-provider"
            value={settings.listen_command_provider}
            onChange={(e) => handleProviderChange(e.target.value)}
            disabled={disabled}
            className={
              "w-full max-w-xs rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm " +
              "text-gray-900 focus:border-blue-500 focus:outline-none focus:ring-1 " +
              "focus:ring-blue-500 disabled:cursor-not-allowed " +
              "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
            }
          >
            {COMMAND_PROVIDERS.map((provider) => (
              <option key={provider.value} value={provider.value}>
                {provider.label}
              </option>
            ))}
          </select>
        </div>

        {settings.listen_command_provider === "custom" && (
          <div className="space-y-2">
            <label
              htmlFor="listen-command-base-url"
              className="block text-sm font-medium text-gray-700 dark:text-gray-300"
            >
              {t("listenCommandCustomBaseUrl")}
            </label>
            <input
              id="listen-command-base-url"
              type="url"
              value={settings.listen_command_custom_base_url}
              onChange={(e) => onUpdate("listen_command_custom_base_url", e.target.value)}
              placeholder="https://api.example.com/"
              disabled={disabled}
              className={
                "w-full max-w-md rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm " +
                "text-gray-900 placeholder:text-gray-400 focus:border-blue-500 " +
                "focus:outline-none focus:ring-1 focus:ring-blue-500 disabled:cursor-not-allowed " +
                "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100 dark:placeholder:text-gray-500"
              }
            />
            <p className="text-xs text-gray-400 dark:text-gray-500">
              {t("listenCommandCustomBaseUrlHint")}
            </p>
          </div>
        )}

        {models.length > 0 && (
          <div className="space-y-2">
            <label
              htmlFor="listen-command-model-preset"
              className="block text-sm font-medium text-gray-700 dark:text-gray-300"
            >
              {t("listenCommandModel")}
            </label>
            <select
              id="listen-command-model-preset"
              value={selectedPreset}
              onChange={(e) => e.target.value && onUpdate("listen_command_model", e.target.value)}
              disabled={disabled}
              className={
                "w-full max-w-xs rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm " +
                "text-gray-900 focus:border-blue-500 focus:outline-none focus:ring-1 " +
                "focus:ring-blue-500 disabled:cursor-not-allowed " +
                "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
              }
            >
              <option value="">{t("listenCommandCustomModelSelected")}</option>
              {models.map((model) => (
                <option key={model.value} value={model.value}>
                  {model.label}
                  {model.tag ? ` - ${t(`modelTag.${model.tag}`)}` : ""}
                </option>
              ))}
            </select>
            <p className="text-xs text-gray-400 dark:text-gray-500">
              {t("listenCommandModelHint")}
            </p>
          </div>
        )}

        <div className="space-y-2">
          <label
            htmlFor="listen-command-custom-model"
            className="block text-sm font-medium text-gray-700 dark:text-gray-300"
          >
            {t("listenCommandCustomModel")}
          </label>
          <input
            id="listen-command-custom-model"
            type="text"
            value={settings.listen_command_model}
            onChange={(e) => onUpdate("listen_command_model", e.target.value)}
            placeholder={t("listenCommandCustomModelPlaceholder")}
            disabled={disabled}
            className={
              "w-full max-w-md rounded-lg border border-gray-300 bg-white px-3 py-2 font-mono " +
              "text-sm text-gray-900 placeholder:text-gray-400 focus:border-blue-500 " +
              "focus:outline-none focus:ring-1 focus:ring-blue-500 disabled:cursor-not-allowed " +
              "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100 dark:placeholder:text-gray-500"
            }
          />
          <p className="text-xs text-gray-400 dark:text-gray-500">
            {t("listenCommandCustomModelHint")}
          </p>
        </div>

        <div className="space-y-2">
          <label
            htmlFor="listen-command-api-key"
            className="block text-sm font-medium text-gray-700 dark:text-gray-300"
          >
            {t("listenCommandApiKey")}
          </label>
          <div className="flex max-w-md items-center gap-2">
            <div className="relative flex-1">
              <input
                id="listen-command-api-key"
                type={showKey ? "text" : "password"}
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder={t("apiKeyPlaceholder")}
                disabled={disabled}
                className={
                  "w-full rounded-lg border border-gray-300 bg-white px-3 py-2 pr-10 text-sm " +
                  "text-gray-900 placeholder:text-gray-400 focus:border-blue-500 " +
                  "focus:outline-none focus:ring-1 focus:ring-blue-500 disabled:cursor-not-allowed " +
                  "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100 dark:placeholder:text-gray-500"
                }
              />
              <button
                type="button"
                onClick={() => setShowKey(!showKey)}
                disabled={disabled}
                className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-gray-600 disabled:cursor-not-allowed dark:hover:text-gray-300"
                aria-label={showKey ? "Hide API key" : "Show API key"}
              >
                {showKey ? "Hide" : "Show"}
              </button>
            </div>
            <button
              type="button"
              onClick={() => void handleSaveKey()}
              disabled={disabled || saving || !apiKey.trim()}
              className="rounded-lg bg-blue-500 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-600 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {saving ? t("saving") : t("save")}
            </button>
          </div>
          <p className={`text-xs ${keyStatus ? "text-green-600 dark:text-green-400" : "text-gray-400 dark:text-gray-500"}`}>
            {keyStatus
              ? t("apiKeyConfigured", { masked: keyStatus })
              : t("apiKeyNotConfigured")}
          </p>
          {saveStatus === "saved" && (
            <p className="text-xs text-green-600 dark:text-green-400">{t("saved")}</p>
          )}
          {saveStatus === "error" && (
            <p className="text-xs text-red-500">{t("saveFailed")}</p>
          )}
        </div>

        <p className="text-xs leading-relaxed text-gray-400 dark:text-gray-500">
          {t("listenCommandSafetyHint")}
        </p>
      </div>
    </div>
  );
}
