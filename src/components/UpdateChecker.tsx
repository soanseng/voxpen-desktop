import { useEffect, useState } from "react";
import { check, Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { useTranslation } from "react-i18next";

type UpdateStatus = "idle" | "checking" | "available" | "downloading" | "error";

export function UpdateChecker() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<UpdateStatus>("idle");
  const [update, setUpdate] = useState<Update | null>(null);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState("");

  useEffect(() => {
    checkForUpdates();
  }, []);

  async function checkForUpdates() {
    try {
      setStatus("checking");
      const result = await check();
      if (result) {
        setUpdate(result);
        setStatus("available");
      } else {
        setStatus("idle");
      }
    } catch (e) {
      setError(String(e));
      setStatus("error");
    }
  }

  async function installUpdate() {
    if (!update) return;
    try {
      setStatus("downloading");
      let totalLength = 0;
      let downloaded = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started" && event.data.contentLength) {
          totalLength = event.data.contentLength;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          if (totalLength > 0) {
            setProgress(Math.round((downloaded / totalLength) * 100));
          }
        }
      });
      await relaunch();
    } catch (e) {
      setError(String(e));
      setStatus("error");
    }
  }

  if (status === "idle" || status === "checking") return null;

  if (status === "error") {
    return (
      <div className="p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg text-sm">
        <p className="text-red-700 dark:text-red-300">
          {t("update.error", { error })}
        </p>
      </div>
    );
  }

  if (status === "available" && update) {
    return (
      <div className="p-3 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-lg text-sm">
        <p className="text-blue-700 dark:text-blue-300 mb-2">
          {t("update.available", { version: update.version })}
        </p>
        <button
          onClick={installUpdate}
          className="px-3 py-1 bg-blue-600 text-white rounded hover:bg-blue-700 text-xs"
        >
          {t("update.install")}
        </button>
      </div>
    );
  }

  if (status === "downloading") {
    return (
      <div className="p-3 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-lg text-sm">
        <p className="text-blue-700 dark:text-blue-300 mb-2">
          {t("update.downloading")}
        </p>
        <div className="w-full bg-blue-200 dark:bg-blue-800 rounded-full h-2">
          <div
            className="bg-blue-600 h-2 rounded-full transition-all"
            style={{ width: `${progress}%` }}
          />
        </div>
        <p className="text-blue-500 dark:text-blue-400 text-xs mt-1">
          {progress}%
        </p>
      </div>
    );
  }

  return null;
}
