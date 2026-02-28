import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Settings } from "../../types/settings";
import { getApiKeyStatus, getDefaultRefinementPrompt, saveApiKey } from "../../lib/tauri";

interface RefinementSectionProps {
  settings: Settings;
  onUpdate: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

const TONE_PRESETS = [
  { value: "Casual", labelKey: "toneCasual" },
  { value: "Professional", labelKey: "toneProfessional" },
  { value: "Email", labelKey: "toneEmail" },
  { value: "Note", labelKey: "toneNote" },
  { value: "Social", labelKey: "toneSocial" },
  { value: "Custom", labelKey: "toneCustom" },
] as const;

const REFINEMENT_PROVIDERS = [
  { value: "groq", label: "Groq" },
  { value: "openai", label: "OpenAI" },
  { value: "openrouter", label: "OpenRouter" },
  { value: "custom", label: "Custom / Ollama" },
];

const TRANSLATION_TARGETS: { value: Settings["stt_language"]; labelKey: string }[] = [
  { value: "Chinese", labelKey: "chinese" },
  { value: "English", labelKey: "english" },
  { value: "Japanese", labelKey: "japanese" },
  { value: "Korean", labelKey: "korean" },
  { value: "French", labelKey: "french" },
  { value: "German", labelKey: "german" },
  { value: "Spanish", labelKey: "spanish" },
  { value: "Vietnamese", labelKey: "vietnamese" },
  { value: "Indonesian", labelKey: "indonesian" },
  { value: "Thai", labelKey: "thai" },
];

interface ModelOption {
  value: string;
  label: string;
  tag?: "recommended" | "budget" | "multilingual" | "quality";
}

function getModelsForProvider(provider: string): ModelOption[] {
  switch (provider) {
    case "groq":
      return [
        { value: "openai/gpt-oss-120b", label: "GPT-OSS 120B", tag: "recommended" },
        { value: "openai/gpt-oss-20b", label: "GPT-OSS 20B", tag: "budget" },
        { value: "qwen/qwen3-32b", label: "Qwen3 32B", tag: "multilingual" },
      ];
    case "openai":
      return [
        { value: "gpt-5-nano", label: "GPT-5 Nano", tag: "recommended" },
        { value: "gpt-5-mini", label: "GPT-5 Mini", tag: "quality" },
        { value: "gpt-4.1-mini", label: "GPT-4.1 Mini" },
      ];
    case "openrouter":
      return [
        { value: "google/gemini-3-flash", label: "Gemini 3 Flash", tag: "recommended" },
        { value: "anthropic/claude-haiku-4.5", label: "Claude Haiku 4.5", tag: "multilingual" },
        { value: "deepseek/deepseek-chat", label: "DeepSeek Chat", tag: "budget" },
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
  const [keyStatus, setKeyStatus] = useState<string | null>(null);

  const [defaultPrompt, setDefaultPrompt] = useState("");
  const [promptResetMsg, setPromptResetMsg] = useState(false);

  const models = getModelsForProvider(settings.refinement_provider);
  const disabled = !settings.refinement_enabled;
  const sameProvider = settings.refinement_provider === settings.stt_provider;

  // Load key status on mount and when provider changes
  useEffect(() => {
    getApiKeyStatus(settings.refinement_provider).then(setKeyStatus).catch(() => setKeyStatus(null));
  }, [settings.refinement_provider]);

  async function handleSaveKey() {
    if (!apiKey.trim()) return;
    setSaving(true);
    setSaveStatus("idle");
    try {
      await saveApiKey(settings.refinement_provider, apiKey.trim());
      setSaveStatus("saved");
      setApiKey("");
      getApiKeyStatus(settings.refinement_provider).then(setKeyStatus).catch(() => {});
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
    } else {
      onUpdate("refinement_model", "");
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

      {/* Translation Mode */}
      <div className={`space-y-3 ${disabled ? "opacity-40" : ""}`}>
        <div className="flex items-center justify-between">
          <div>
            <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
              {t("translationEnabled")}
            </label>
            <p className="text-xs text-gray-400 dark:text-gray-500">
              {t("translationEnabledHint")}
            </p>
          </div>
          <ToggleSwitch
            id="translation-enabled"
            checked={settings.translation_enabled}
            onChange={(v) => onUpdate("translation_enabled", v)}
          />
        </div>

        {settings.translation_enabled && (
          <div className="space-y-1">
            <label
              htmlFor="translation-target"
              className="block text-sm font-medium text-gray-700 dark:text-gray-300"
            >
              {t("translationTarget")}
            </label>
            <select
              id="translation-target"
              value={settings.translation_target}
              onChange={(e) =>
                onUpdate("translation_target", e.target.value as Settings["translation_target"])
              }
              disabled={disabled}
              className={
                "w-full max-w-xs rounded-lg border border-gray-300 bg-white " +
                "px-3 py-2 text-sm text-gray-900 " +
                "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
                "disabled:cursor-not-allowed " +
                "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
              }
            >
              {TRANSLATION_TARGETS.map((lang) => (
                <option key={lang.value} value={lang.value}>
                  {t(lang.labelKey)}
                </option>
              ))}
            </select>
          </div>
        )}
      </div>

      {/* Tone Preset */}
      <div className={`space-y-2 ${disabled ? "opacity-40" : ""}`}>
        <label
          htmlFor="tone-preset"
          className="block text-sm font-medium text-gray-700 dark:text-gray-300"
        >
          {t("tone")}
        </label>
        <p className="text-xs text-gray-400 dark:text-gray-500">
          {t("toneHint")}
        </p>
        <select
          id="tone-preset"
          value={settings.tone_preset}
          onChange={(e) => {
            onUpdate("tone_preset", e.target.value as Settings["tone_preset"]);
            // Clear default prompt cache so it reloads for new tone
            setDefaultPrompt("");
          }}
          disabled={disabled}
          className={
            "w-full max-w-xs rounded-lg border border-gray-300 bg-white " +
            "px-3 py-2 text-sm text-gray-900 " +
            "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
            "disabled:cursor-not-allowed " +
            "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
          }
        >
          {TONE_PRESETS.map((tp) => (
            <option key={tp.value} value={tp.value}>
              {t(tp.labelKey)}
            </option>
          ))}
        </select>
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
          <p className={`text-xs ${keyStatus ? "text-green-600 dark:text-green-400" : "text-gray-400 dark:text-gray-500"}`}>
            {keyStatus
              ? t("apiKeyConfigured", { masked: keyStatus })
              : t("apiKeyNotConfigured")}
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
          {saveStatus === "idle" && (
            <p className={`text-xs ${keyStatus ? "text-green-600 dark:text-green-400" : "text-gray-400 dark:text-gray-500"}`}>
              {keyStatus
                ? t("apiKeyConfigured", { masked: keyStatus })
                : t("apiKeyNotConfigured")}
            </p>
          )}
        </div>
      )}

      {/* Model — preset dropdown + custom override for known providers */}
      {settings.refinement_provider !== "custom" && (
        <div className={`space-y-2 ${disabled ? "opacity-40" : ""}`}>
          <label
            htmlFor="refinement-model"
            className="block text-sm font-medium text-gray-700 dark:text-gray-300"
          >
            {t("model")}
          </label>
          {models.length > 0 && (
            <select
              id="refinement-model"
              value={
                models.some((m) => m.value === settings.refinement_model)
                  ? settings.refinement_model
                  : ""
              }
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
              {!models.some((m) => m.value === settings.refinement_model) && (
                <option value="" disabled>
                  —
                </option>
              )}
              {models.map((m) => (
                <option key={m.value} value={m.value}>
                  {m.label}{m.tag ? ` \u2014 ${t(`modelTag.${m.tag}`)}` : ""}
                </option>
              ))}
            </select>
          )}
          {/* Custom model name override */}
          <div className="mt-2">
            <input
              type="text"
              value={
                models.some((m) => m.value === settings.refinement_model)
                  ? ""
                  : settings.refinement_model
              }
              onChange={(e) => onUpdate("refinement_model", e.target.value)}
              placeholder={t("customModelPlaceholder")}
              disabled={disabled}
              className={
                "w-full max-w-xs rounded-lg border border-gray-300 bg-white " +
                "px-3 py-2 text-sm text-gray-900 " +
                "placeholder:text-gray-400 " +
                "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
                "disabled:cursor-not-allowed " +
                "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100 " +
                "dark:placeholder:text-gray-500"
              }
            />
            <p className="mt-1 text-xs text-gray-400 dark:text-gray-500">
              {t("customModelHint")}
            </p>
          </div>
        </div>
      )}

      {/* Model & Base URL — custom/Ollama provider */}
      {settings.refinement_provider === "custom" && (
        <div className={`space-y-4 ${disabled ? "opacity-40" : ""}`}>
          <div className="space-y-2">
            <label
              htmlFor="custom-base-url"
              className="block text-sm font-medium text-gray-700 dark:text-gray-300"
            >
              {t("baseUrl")}
            </label>
            <input
              id="custom-base-url"
              type="text"
              value={settings.custom_base_url}
              onChange={(e) => onUpdate("custom_base_url", e.target.value)}
              placeholder="http://localhost:11434/"
              disabled={disabled}
              className={
                "w-full max-w-md rounded-lg border border-gray-300 bg-white " +
                "px-3 py-2 text-sm text-gray-900 " +
                "placeholder:text-gray-400 " +
                "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
                "disabled:cursor-not-allowed " +
                "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100 " +
                "dark:placeholder:text-gray-500"
              }
            />
            <p className="mt-1 text-xs text-gray-400 dark:text-gray-500">
              {t("baseUrlHint")}
            </p>
          </div>
          <div className="space-y-2">
            <label
              htmlFor="custom-model"
              className="block text-sm font-medium text-gray-700 dark:text-gray-300"
            >
              {t("model")}
            </label>
            <input
              id="custom-model"
              type="text"
              value={settings.refinement_model}
              onChange={(e) => onUpdate("refinement_model", e.target.value)}
              placeholder="llama3.1:8b"
              disabled={disabled}
              className={
                "w-full max-w-xs rounded-lg border border-gray-300 bg-white " +
                "px-3 py-2 text-sm text-gray-900 " +
                "placeholder:text-gray-400 " +
                "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
                "disabled:cursor-not-allowed " +
                "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100 " +
                "dark:placeholder:text-gray-500"
              }
            />
          </div>
        </div>
      )}

      {/* System Prompt — only editable when tone is Custom */}
      <div className={`space-y-2 ${disabled ? "opacity-40" : ""}`}>
        <div className="flex items-center justify-between">
          <label
            htmlFor="refinement-prompt"
            className="block text-sm font-medium text-gray-700 dark:text-gray-300"
          >
            {t("systemPrompt")}
          </label>
          {settings.tone_preset === "Custom" && (
            <button
              type="button"
              onClick={async () => {
                const lang = settings.stt_language;
                const prompt = await getDefaultRefinementPrompt(lang, "Casual");
                setDefaultPrompt(prompt);
                onUpdate("refinement_prompt", "");
                setPromptResetMsg(true);
                setTimeout(() => setPromptResetMsg(false), 2000);
              }}
              disabled={disabled || !settings.refinement_prompt}
              className={
                "rounded px-2 py-1 text-xs font-medium " +
                "text-blue-600 hover:bg-blue-50 " +
                "disabled:cursor-not-allowed disabled:opacity-50 " +
                "dark:text-blue-400 dark:hover:bg-gray-800"
              }
              title={t("resetPromptHint")}
            >
              {t("resetPrompt")}
            </button>
          )}
        </div>
        <p className="text-xs text-gray-400 dark:text-gray-500">
          {settings.tone_preset === "Custom"
            ? t("systemPromptHint")
            : t("toneUsingPreset", { tone: t(`tone${settings.tone_preset}`) })}
        </p>
        <textarea
          id="refinement-prompt"
          value={settings.tone_preset === "Custom" ? settings.refinement_prompt : ""}
          onChange={(e) => onUpdate("refinement_prompt", e.target.value)}
          onFocus={async () => {
            if (!defaultPrompt) {
              const tone = settings.tone_preset || "Casual";
              const prompt = await getDefaultRefinementPrompt(settings.stt_language, tone);
              setDefaultPrompt(prompt);
            }
          }}
          placeholder={defaultPrompt || t("systemPromptPlaceholder")}
          disabled={disabled || settings.tone_preset !== "Custom"}
          rows={5}
          className={
            "w-full rounded-lg border border-gray-300 bg-white " +
            "px-3 py-2 text-sm text-gray-900 " +
            "placeholder:text-gray-400 " +
            "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
            "disabled:cursor-not-allowed " +
            "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100 " +
            "dark:placeholder:text-gray-500"
          }
        />
        {promptResetMsg && (
          <p className="text-xs text-green-600 dark:text-green-400">
            {t("promptReset")}
          </p>
        )}
        {settings.tone_preset === "Custom" && !settings.refinement_prompt && !promptResetMsg && (
          <p className="text-xs text-gray-400 dark:text-gray-500">
            {t("usingDefault")}
          </p>
        )}
      </div>
    </div>
  );
}
