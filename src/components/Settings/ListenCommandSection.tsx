import { useTranslation } from "react-i18next";
import type { Settings } from "../../types/settings";

interface ListenCommandSectionProps {
  settings: Settings;
  onUpdate: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

const COMMAND_PROVIDER_LABELS: Record<string, string> = {
  groq: "Groq",
  openai: "OpenAI",
  openrouter: "OpenRouter",
  custom: "Custom / LiteLLM / Ollama",
};

interface ModelOption {
  value: string;
  label: string;
  tag?: "recommended" | "budget" | "multilingual" | "quality";
}

function getModelsForProvider(provider: string): ModelOption[] {
  switch (provider) {
    case "openai":
      return [
        { value: "gpt-5-nano", label: "GPT-5 Nano", tag: "recommended" },
        { value: "gpt-5-mini", label: "GPT-5 Mini", tag: "quality" },
        { value: "gpt-4.1-mini", label: "GPT-4.1 Mini" },
      ];
    case "groq":
      return [
        { value: "openai/gpt-oss-120b", label: "GPT-OSS 120B", tag: "recommended" },
        { value: "openai/gpt-oss-20b", label: "GPT-OSS 20B", tag: "budget" },
        { value: "qwen/qwen3-32b", label: "Qwen3 32B", tag: "multilingual" },
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

export default function ListenCommandSection({
  settings,
  onUpdate,
}: ListenCommandSectionProps) {
  const { t } = useTranslation();

  const disabled = !settings.listen_command_enabled;
  const inheritedProvider =
    COMMAND_PROVIDER_LABELS[settings.refinement_provider] ?? settings.refinement_provider;
  const models = getModelsForProvider(settings.refinement_provider);
  const selectedPreset = models.some((model) => model.value === settings.listen_command_model)
    ? settings.listen_command_model
    : "";

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

      <div className="flex items-center justify-between">
        <div>
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            {t("listenCommandEnabled")}
          </label>
          <p className="text-xs text-gray-400 dark:text-gray-500">
            {t("listenCommandEnabledHint")}
          </p>
        </div>
        <ToggleSwitch
          id="listen-command-section-enabled"
          checked={settings.listen_command_enabled}
          onChange={(v) => onUpdate("listen_command_enabled", v)}
        />
      </div>

      <div className={disabled ? "space-y-6 opacity-40" : "space-y-6"}>
        <div className="space-y-2">
          <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
            {t("listenCommandProvider")}
          </label>
          <div className="w-full max-w-md rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 text-sm text-gray-700 dark:border-gray-700 dark:bg-gray-800/60 dark:text-gray-200">
            {t("listenCommandProviderInherited", { provider: inheritedProvider })}
          </div>
          <p className="text-xs text-gray-400 dark:text-gray-500">
            {t("listenCommandProviderHint")}
          </p>
        </div>

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

        <p className="text-xs leading-relaxed text-gray-400 dark:text-gray-500">
          {t("listenCommandSafetyHint")}
        </p>
      </div>
    </div>
  );
}
