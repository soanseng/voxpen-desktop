import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { open, save } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { transcribeFile } from "../../lib/tauri";
import type { FileTranscriptionResult } from "../../types/settings";

const AUDIO_EXTENSIONS = ["wav", "mp3", "flac", "m4a", "ogg", "webm"];

export default function FileTranscriptionSection() {
  const { t } = useTranslation();
  const [transcribing, setTranscribing] = useState(false);
  const [result, setResult] = useState<FileTranscriptionResult | null>(null);
  const [error, setError] = useState("");
  const [copied, setCopied] = useState<"original" | "refined" | null>(null);
  const [exported, setExported] = useState<"srt" | "txt" | null>(null);
  const [selectedFile, setSelectedFile] = useState("");
  const [dragOver, setDragOver] = useState(false);

  async function startTranscription(filePath: string) {
    const ext = filePath.split(".").pop()?.toLowerCase() ?? "";
    if (!AUDIO_EXTENSIONS.includes(ext)) {
      setError(t("fileTranscribe.supported"));
      return;
    }

    setSelectedFile(filePath);
    setError("");
    setResult(null);
    setTranscribing(true);

    try {
      const r = await transcribeFile(filePath);
      setResult(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setTranscribing(false);
    }
  }

  useEffect(() => {
    const webview = getCurrentWebview();
    const unlisten = webview.onDragDropEvent((event) => {
      if (transcribing || result) return;
      if (event.payload.type === "enter" || event.payload.type === "over") {
        setDragOver(true);
      } else if (event.payload.type === "drop") {
        setDragOver(false);
        const paths = event.payload.paths;
        if (paths.length > 0) {
          startTranscription(paths[0]);
        }
      } else {
        setDragOver(false);
      }
    });
    return () => { unlisten.then((f) => f()); };
  }, [transcribing, result]);

  async function handleSelectFile() {
    const file = await open({
      multiple: false,
      filters: [
        {
          name: "Audio",
          extensions: AUDIO_EXTENSIONS,
        },
      ],
    });
    if (!file) return;
    startTranscription(file);
  }

  function handleCopy(text: string, which: "original" | "refined") {
    navigator.clipboard.writeText(text);
    setCopied(which);
    setTimeout(() => setCopied(null), 2000);
  }

  async function handleExport(format: "srt" | "txt") {
    if (!result) return;
    const content = format === "srt" ? result.srt : (result.refined ?? result.text);
    const ext = format;
    const defaultName = selectedFile
      ? selectedFile.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, `.${ext}`) ?? `transcription.${ext}`
      : `transcription.${ext}`;

    const filePath = await save({
      defaultPath: defaultName,
      filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
    });
    if (!filePath) return;

    await invoke("write_text_file", { filePath, content });
    setExported(format);
    setTimeout(() => setExported(null), 2000);
  }

  function handleReset() {
    setResult(null);
    setError("");
    setSelectedFile("");
  }

  return (
    <div>
      <h2 className="mb-1 text-lg font-semibold text-gray-900 dark:text-gray-100">
        {t("fileTranscribe.title")}
      </h2>
      <p className="mb-6 text-sm text-gray-500 dark:text-gray-400">
        {t("fileTranscribe.description")}
      </p>

      {result ? (
        /* Result view */
        <div className="space-y-4">
          {/* Original */}
          <div>
            <div className="mb-1 flex items-center justify-between">
              <span className="text-xs font-medium uppercase tracking-wide text-gray-500 dark:text-gray-400">
                {t("fileTranscribe.result")}
              </span>
              <button
                type="button"
                onClick={() => handleCopy(result.text, "original")}
                className="text-xs text-blue-600 hover:text-blue-700 dark:text-blue-400"
              >
                {copied === "original"
                  ? t("fileTranscribe.copiedResult")
                  : t("fileTranscribe.copyResult")}
              </button>
            </div>
            <div className="max-h-48 overflow-y-auto rounded-lg border border-gray-200 bg-gray-50 p-3 text-sm text-gray-800 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-200">
              {result.text}
            </div>
          </div>

          {/* Refined */}
          {result.refined && (
            <div>
              <div className="mb-1 flex items-center justify-between">
                <span className="text-xs font-medium uppercase tracking-wide text-gray-500 dark:text-gray-400">
                  {t("fileTranscribe.refinedResult")}
                </span>
                <button
                  type="button"
                  onClick={() => handleCopy(result.refined!, "refined")}
                  className="text-xs text-blue-600 hover:text-blue-700 dark:text-blue-400"
                >
                  {copied === "refined"
                    ? t("fileTranscribe.copiedResult")
                    : t("fileTranscribe.copyResult")}
                </button>
              </div>
              <div className="max-h-48 overflow-y-auto rounded-lg border border-gray-200 bg-gray-50 p-3 text-sm text-gray-800 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-200">
                {result.refined}
              </div>
            </div>
          )}

          {/* Export buttons */}
          <div className="flex gap-2">
            {result.srt && (
              <button
                type="button"
                onClick={() => handleExport("srt")}
                className="flex-1 rounded-lg bg-blue-50 px-4 py-2 text-sm font-medium text-blue-700 hover:bg-blue-100 dark:bg-blue-900/20 dark:text-blue-400 dark:hover:bg-blue-900/30"
              >
                {exported === "srt" ? t("fileTranscribe.exported") : t("fileTranscribe.exportSrt")}
              </button>
            )}
            <button
              type="button"
              onClick={() => handleExport("txt")}
              className="flex-1 rounded-lg bg-blue-50 px-4 py-2 text-sm font-medium text-blue-700 hover:bg-blue-100 dark:bg-blue-900/20 dark:text-blue-400 dark:hover:bg-blue-900/30"
            >
              {exported === "txt" ? t("fileTranscribe.exported") : t("fileTranscribe.exportTxt")}
            </button>
          </div>

          <button
            type="button"
            onClick={handleReset}
            className="w-full rounded-lg bg-gray-100 px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700"
          >
            {t("fileTranscribe.transcribeAnother")}
          </button>
        </div>
      ) : (
        /* File selection view */
        <div className="space-y-4">
          <button
            type="button"
            onClick={handleSelectFile}
            disabled={transcribing}
            className={`flex w-full flex-col items-center justify-center rounded-lg border-2 border-dashed px-6 py-8 text-center transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
              dragOver
                ? "border-blue-500 bg-blue-50 dark:border-blue-400 dark:bg-blue-900/20"
                : "border-gray-300 hover:border-blue-400 hover:bg-blue-50 dark:border-gray-600 dark:hover:border-blue-500 dark:hover:bg-blue-900/10"
            }`}
          >
            {transcribing ? (
              <>
                <svg
                  className="mb-2 h-8 w-8 animate-spin text-blue-500"
                  fill="none"
                  viewBox="0 0 24 24"
                >
                  <circle
                    className="opacity-25"
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    strokeWidth="4"
                  />
                  <path
                    className="opacity-75"
                    fill="currentColor"
                    d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
                  />
                </svg>
                <span className="text-sm font-medium text-blue-600 dark:text-blue-400">
                  {t("fileTranscribe.transcribing")}
                </span>
                {selectedFile && (
                  <span className="mt-1 max-w-full truncate text-xs text-gray-400">
                    {selectedFile.split(/[\\/]/).pop()}
                  </span>
                )}
              </>
            ) : dragOver ? (
              <>
                <svg
                  className="mb-2 h-8 w-8 text-blue-500"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth={1.5}
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5"
                  />
                </svg>
                <span className="text-sm font-medium text-blue-600 dark:text-blue-400">
                  {t("fileTranscribe.dragHint")}
                </span>
              </>
            ) : (
              <>
                <svg
                  className="mb-2 h-8 w-8 text-gray-400"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth={1.5}
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M19.114 5.636a9 9 0 010 12.728M16.463 8.288a5.25 5.25 0 010 7.424M6.75 8.25l4.72-4.72a.75.75 0 011.28.53v15.88a.75.75 0 01-1.28.53l-4.72-4.72H4.51c-.88 0-1.704-.507-1.938-1.354A9.01 9.01 0 012.25 12c0-.83.112-1.633.322-2.396C2.806 8.756 3.63 8.25 4.51 8.25H6.75z"
                  />
                </svg>
                <span className="text-sm font-medium text-gray-700 dark:text-gray-300">
                  {t("fileTranscribe.selectFile")}
                </span>
                <span className="mt-1 text-xs text-gray-400 dark:text-gray-500">
                  {t("fileTranscribe.dragHint")}
                </span>
              </>
            )}
          </button>

          <p className="text-center text-xs text-gray-400 dark:text-gray-500">
            {t("fileTranscribe.supported")}
          </p>

          {error && (
            <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-800 dark:bg-red-900/20 dark:text-red-400">
              {t("fileTranscribe.error", { error })}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
