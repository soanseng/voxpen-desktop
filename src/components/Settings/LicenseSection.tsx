import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import type { LicenseInfo, UsageStatus } from "../../types/settings";
import {
  getLicenseInfo,
  getUsageStatus,
  activateLicense,
  deactivateLicense,
  openUrl,
} from "../../lib/tauri";

const PURCHASE_URL = "https://anatomind.lemonsqueezy.com/checkout/buy/299dd747-4424-4b1b-8aa8-2c47a94f7dd1";
const FREE_DAILY_LIMIT = 15;

function maskKey(key: string): string {
  if (key.length <= 8) return key;
  return key.slice(0, 4) + "\u00B7\u00B7\u00B7\u00B7" + key.slice(-4);
}

function usageUsedCount(status: UsageStatus): number {
  switch (status.type) {
    case "Available":
      return FREE_DAILY_LIMIT - status.data.remaining;
    case "Warning":
      return FREE_DAILY_LIMIT - status.data.remaining;
    case "Exhausted":
      return FREE_DAILY_LIMIT;
    case "Unlimited":
      return 0;
  }
}

export default function LicenseSection() {
  const { t } = useTranslation();
  const [license, setLicense] = useState<LicenseInfo | null>(null);
  const [usage, setUsage] = useState<UsageStatus | null>(null);
  const [key, setKey] = useState("");
  const [activating, setActivating] = useState(false);
  const [message, setMessage] = useState<{
    type: "success" | "error";
    text: string;
  } | null>(null);

  // Load license and usage on mount
  useEffect(() => {
    getLicenseInfo()
      .then(setLicense)
      .catch(() => setLicense(null));
    getUsageStatus()
      .then(setUsage)
      .catch(() => {});
  }, []);

  // Listen for usage-updated events
  useEffect(() => {
    const unlisten = listen<UsageStatus>("usage-updated", (event) => {
      setUsage(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  async function handleActivate() {
    const trimmed = key.trim();
    if (!trimmed) return;
    setActivating(true);
    setMessage(null);
    try {
      const info = await activateLicense(trimmed);
      setLicense(info);
      setKey("");
      setMessage({ type: "success", text: t("license.activateSuccess") });
      // Refresh usage — now unlimited
      getUsageStatus()
        .then(setUsage)
        .catch(() => {});
    } catch (err) {
      setMessage({
        type: "error",
        text: t("license.activateFailed", { error: String(err) }),
      });
    } finally {
      setActivating(false);
    }
  }

  async function handleDeactivate() {
    if (!confirm(t("license.deactivate") + "?")) return;
    try {
      await deactivateLicense();
      setLicense(null);
      setMessage({ type: "success", text: t("license.deactivateSuccess") });
      // Refresh usage — back to free tier
      getUsageStatus()
        .then(setUsage)
        .catch(() => {});
    } catch (err) {
      setMessage({
        type: "error",
        text: String(err),
      });
    }
  }

  const isPro = license !== null && license.tier === "Pro";

  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">
          {isPro ? t("license.pro") : t("license.free")}
        </h2>
        {isPro && (
          <p className="mt-1 flex items-center gap-1.5 text-sm text-green-600 dark:text-green-400">
            <svg
              className="h-4 w-4"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2.5}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
            {t("license.pro")}
          </p>
        )}
      </div>

      {/* Usage bar (Free tier only) */}
      {!isPro && usage && (
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <span className="text-sm text-gray-600 dark:text-gray-400">
              {t("license.usageToday", {
                used: String(usageUsedCount(usage)),
                limit: String(FREE_DAILY_LIMIT),
              })}
            </span>
            {usage.type === "Exhausted" && (
              <span className="text-xs font-medium text-amber-600 dark:text-amber-400">
                {t("license.exhausted")}
              </span>
            )}
          </div>
          <div className="h-2 w-full overflow-hidden rounded-full bg-gray-200 dark:bg-gray-700">
            <div
              className={
                "h-full rounded-full transition-all duration-300 " +
                (usage.type === "Exhausted"
                  ? "bg-amber-500"
                  : usage.type === "Warning"
                    ? "bg-amber-400"
                    : "bg-blue-500")
              }
              style={{
                width: `${Math.min(100, (usageUsedCount(usage) / FREE_DAILY_LIMIT) * 100)}%`,
              }}
            />
          </div>
        </div>
      )}

      {/* Pro license details */}
      {isPro && license && (
        <div className="space-y-3 rounded-lg border border-gray-200 bg-gray-50 p-4 dark:border-gray-700 dark:bg-gray-800">
          <div className="text-sm text-gray-600 dark:text-gray-400">
            <span className="font-medium text-gray-700 dark:text-gray-300">
              {maskKey(license.license_key)}
            </span>
          </div>
          <div className="text-xs text-gray-500 dark:text-gray-400">
            {t("license.validFor", {
              version: String(license.licensed_version),
            })}
          </div>
          <div className="text-xs text-gray-500 dark:text-gray-400">
            {t("license.lastVerified", {
              date: new Date(license.last_verified_at * 1000).toLocaleDateString(),
            })}
          </div>
          <button
            type="button"
            onClick={() => void handleDeactivate()}
            className={
              "rounded-lg border border-red-300 px-3 py-1.5 text-sm font-medium " +
              "text-red-600 hover:bg-red-50 " +
              "dark:border-red-700 dark:text-red-400 dark:hover:bg-red-900/20"
            }
          >
            {t("license.deactivate")}
          </button>
        </div>
      )}

      {/* Activate license (Free tier) */}
      {!isPro && (
        <>
          <div className="space-y-2">
            <label
              htmlFor="license-key"
              className="block text-sm font-medium text-gray-700 dark:text-gray-300"
            >
              {t("license.tab")}
            </label>
            <div className="flex max-w-md items-center gap-2">
              <input
                id="license-key"
                type="text"
                value={key}
                onChange={(e) => setKey(e.target.value)}
                placeholder={t("license.enterKey")}
                className={
                  "flex-1 rounded-lg border border-gray-300 bg-white " +
                  "px-3 py-2 text-sm text-gray-900 " +
                  "placeholder:text-gray-400 " +
                  "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
                  "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100 " +
                  "dark:placeholder:text-gray-500"
                }
              />
              <button
                type="button"
                onClick={() => void handleActivate()}
                disabled={activating || !key.trim()}
                className={
                  "rounded-lg bg-blue-500 px-4 py-2 text-sm font-medium text-white " +
                  "transition-colors hover:bg-blue-600 " +
                  "disabled:cursor-not-allowed disabled:opacity-50 " +
                  "dark:bg-blue-600 dark:hover:bg-blue-700"
                }
              >
                {activating ? t("license.activating") : t("license.activate")}
              </button>
            </div>
          </div>

          {/* Divider */}
          <div className="flex items-center gap-3">
            <div className="h-px flex-1 bg-gray-200 dark:bg-gray-700" />
            <span className="text-xs text-gray-400 dark:text-gray-500">or</span>
            <div className="h-px flex-1 bg-gray-200 dark:bg-gray-700" />
          </div>

          {/* Purchase button */}
          <button
            type="button"
            onClick={() => void openUrl(PURCHASE_URL)}
            className={
              "w-full rounded-lg border border-blue-500 px-4 py-2.5 text-sm font-medium " +
              "text-blue-600 transition-colors hover:bg-blue-50 " +
              "dark:border-blue-400 dark:text-blue-400 dark:hover:bg-blue-900/20"
            }
          >
            {t("license.purchase")}
          </button>
        </>
      )}

      {/* Status message */}
      {message && (
        <p
          className={
            "text-xs " +
            (message.type === "success"
              ? "text-green-600 dark:text-green-400"
              : "text-red-600 dark:text-red-400")
          }
        >
          {message.text}
        </p>
      )}
    </div>
  );
}
