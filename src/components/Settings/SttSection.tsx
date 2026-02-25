import { useEffect, useState, useCallback } from "react";
import { useTranslation } from "react-i18next";
import type { Settings } from "../../types/settings";
import { checkMicrophone, getApiKeyStatus, saveApiKey } from "../../lib/tauri";

interface SttSectionProps {
  settings: Settings;
  onUpdate: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

const STT_PROVIDERS = [
  { value: "groq", label: "Groq" },
  { value: "openai", label: "OpenAI" },
  { value: "custom", label: "Custom Server" },
];

function getLanguageOptions(t: (key: string) => string): { value: Settings["stt_language"]; label: string }[] {
  return [
    { value: "Auto", label: t("autoDetect") },
    { value: "Chinese", label: t("chinese") },
    { value: "English", label: t("english") },
    { value: "Japanese", label: t("japanese") },
    { value: "Korean", label: t("korean") },
    { value: "French", label: t("french") },
    { value: "German", label: t("german") },
    { value: "Spanish", label: t("spanish") },
    { value: "Vietnamese", label: t("vietnamese") },
    { value: "Indonesian", label: t("indonesian") },
    { value: "Thai", label: t("thai") },
  ];
}

function getModelsForProvider(provider: string) {
  switch (provider) {
    case "groq":
      return [
        { value: "whisper-large-v3-turbo", label: "whisper-large-v3-turbo" },
        { value: "whisper-large-v3", label: "whisper-large-v3" },
      ];
    case "openai":
      return [
        { value: "whisper-1", label: "whisper-1" },
        { value: "gpt-4o-transcribe", label: "gpt-4o-transcribe" },
      ];
    default:
      return [];
  }
}

export default function SttSection({ settings, onUpdate }: SttSectionProps) {
  const { t } = useTranslation();
  const [apiKey, setApiKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveStatus, setSaveStatus] = useState<"idle" | "saved" | "error">(
    "idle",
  );
  const [keyStatus, setKeyStatus] = useState<string | null>(null);
  const [mic, setMic] = useState<{ status: "checking" | "ok" | "error"; detail: string }>({
    status: "checking",
    detail: "",
  });

  const models = getModelsForProvider(settings.stt_provider);
  const languages = getLanguageOptions(t);

  const recheckMic = useCallback(() => {
    setMic({ status: "checking", detail: "" });
    checkMicrophone()
      .then((name) => setMic({ status: "ok", detail: name }))
      .catch((err) => setMic({ status: "error", detail: String(err) }));
  }, []);

  // Check microphone on mount
  useEffect(() => { recheckMic(); }, [recheckMic]);

  // Load key status on mount and when provider changes
  useEffect(() => {
    getApiKeyStatus(settings.stt_provider).then(setKeyStatus).catch(() => setKeyStatus(null));
  }, [settings.stt_provider]);

  async function handleSaveKey() {
    if (!apiKey.trim()) return;
    setSaving(true);
    setSaveStatus("idle");
    try {
      await saveApiKey(settings.stt_provider, apiKey.trim());
      setSaveStatus("saved");
      setApiKey("");
      getApiKeyStatus(settings.stt_provider).then(setKeyStatus).catch(() => {});
      setTimeout(() => setSaveStatus("idle"), 2000);
    } catch {
      setSaveStatus("error");
      setTimeout(() => setSaveStatus("idle"), 3000);
    } finally {
      setSaving(false);
    }
  }

  function handleProviderChange(provider: string) {
    onUpdate("stt_provider", provider);
    // Reset model to first available for the new provider
    const newModels = getModelsForProvider(provider);
    if (newModels.length > 0) {
      onUpdate("stt_model", newModels[0].value);
    }
  }

  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">
          {t("speech")}
        </h2>
        <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
          {t("speechDescription")}
        </p>
      </div>

      {/* Microphone status */}
      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
          {t("microphone")}
        </label>
        <div className="flex items-center gap-2">
          {mic.status === "checking" && (
            <span className="text-sm text-gray-400 dark:text-gray-500">{t("micChecking")}</span>
          )}
          {mic.status === "ok" && (
            <span className="text-sm text-green-600 dark:text-green-400">
              {t("micOk", { name: mic.detail })}
            </span>
          )}
          {mic.status === "error" && (
            <span className="text-sm text-red-600 dark:text-red-400">
              {t("micError", { error: mic.detail })}
            </span>
          )}
          {mic.status !== "checking" && (
            <button
              type="button"
              onClick={recheckMic}
              className="text-xs text-blue-500 hover:text-blue-600 dark:text-blue-400 dark:hover:text-blue-300"
            >
              {t("micRecheck")}
            </button>
          )}
        </div>
      </div>

      {/* Provider */}
      <div className="space-y-2">
        <label
          htmlFor="stt-provider"
          className="block text-sm font-medium text-gray-700 dark:text-gray-300"
        >
          {t("provider")}
        </label>
        <select
          id="stt-provider"
          value={settings.stt_provider}
          onChange={(e) => handleProviderChange(e.target.value)}
          className={
            "w-full max-w-xs rounded-lg border border-gray-300 bg-white " +
            "px-3 py-2 text-sm text-gray-900 " +
            "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
            "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
          }
        >
          {STT_PROVIDERS.map((p) => (
            <option key={p.value} value={p.value}>
              {p.label}
            </option>
          ))}
        </select>
      </div>

      {/* API Key */}
      <div className="space-y-2">
        <label
          htmlFor="stt-api-key"
          className="block text-sm font-medium text-gray-700 dark:text-gray-300"
        >
          {t("apiKey")}
        </label>
        <div className="flex max-w-md items-center gap-2">
          <div className="relative flex-1">
            <input
              id="stt-api-key"
              type={showKey ? "text" : "password"}
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder={t("apiKeyPlaceholder")}
              className={
                "w-full rounded-lg border border-gray-300 bg-white " +
                "px-3 py-2 pr-10 text-sm text-gray-900 " +
                "placeholder:text-gray-400 " +
                "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
                "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100 " +
                "dark:placeholder:text-gray-500"
              }
            />
            <button
              type="button"
              onClick={() => setShowKey(!showKey)}
              className={
                "absolute right-2 top-1/2 -translate-y-1/2 " +
                "text-gray-400 hover:text-gray-600 dark:hover:text-gray-300"
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
            disabled={saving || !apiKey.trim()}
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
        {saveStatus === "idle" && (
          <p className={`text-xs ${keyStatus ? "text-green-600 dark:text-green-400" : "text-gray-400 dark:text-gray-500"}`}>
            {keyStatus
              ? t("apiKeyConfigured", { masked: keyStatus })
              : t("apiKeyNotConfigured")}
          </p>
        )}
      </div>

      {/* Language */}
      <div className="space-y-2">
        <label
          htmlFor="stt-language"
          className="block text-sm font-medium text-gray-700 dark:text-gray-300"
        >
          {t("language")}
        </label>
        <select
          id="stt-language"
          value={settings.stt_language}
          onChange={(e) =>
            onUpdate(
              "stt_language",
              e.target.value as Settings["stt_language"],
            )
          }
          className={
            "w-full max-w-xs rounded-lg border border-gray-300 bg-white " +
            "px-3 py-2 text-sm text-gray-900 " +
            "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
            "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
          }
        >
          {languages.map((l) => (
            <option key={l.value} value={l.value}>
              {l.label}
            </option>
          ))}
        </select>
      </div>

      {/* Model */}
      {models.length > 0 && (
        <div className="space-y-2">
          <label
            htmlFor="stt-model"
            className="block text-sm font-medium text-gray-700 dark:text-gray-300"
          >
            {t("model")}
          </label>
          <select
            id="stt-model"
            value={settings.stt_model}
            onChange={(e) => onUpdate("stt_model", e.target.value)}
            className={
              "w-full max-w-xs rounded-lg border border-gray-300 bg-white " +
              "px-3 py-2 text-sm text-gray-900 " +
              "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
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
