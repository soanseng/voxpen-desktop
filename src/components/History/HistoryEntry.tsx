import { useState } from "react";
import type { TranscriptionEntry } from "../../types/history";

interface HistoryEntryProps {
  entry: TranscriptionEntry;
  onDelete: (id: string) => void;
}

const LANGUAGE_BADGE: Record<string, { label: string; className: string }> = {
  zh: {
    label: "zh",
    className:
      "bg-red-100 text-red-700 dark:bg-red-900/40 dark:text-red-300",
  },
  en: {
    label: "en",
    className:
      "bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-300",
  },
  ja: {
    label: "ja",
    className:
      "bg-purple-100 text-purple-700 dark:bg-purple-900/40 dark:text-purple-300",
  },
};

const DEFAULT_BADGE = {
  label: "auto",
  className:
    "bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-400",
};

function formatTimestamp(ts: number): string {
  const date = new Date(ts);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

  const time = date.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  });

  if (diffDays === 0) {
    return `Today ${time}`;
  }
  if (diffDays === 1) {
    return `Yesterday ${time}`;
  }
  return date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatDuration(ms: number): string {
  const secs = Math.round(ms / 1000);
  if (secs < 60) {
    return `${secs}s`;
  }
  const mins = Math.floor(secs / 60);
  const remainder = secs % 60;
  return `${mins}m ${remainder}s`;
}

function truncate(text: string, maxLen: number): string {
  if (text.length <= maxLen) return text;
  return text.slice(0, maxLen) + "...";
}

export default function HistoryEntry({ entry, onDelete }: HistoryEntryProps) {
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const displayText = entry.refined_text ?? entry.original_text;
  const badge = LANGUAGE_BADGE[entry.language] ?? DEFAULT_BADGE;

  function handleCopy() {
    navigator.clipboard.writeText(displayText).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }).catch(() => {
      // clipboard access denied — silent
    });
  }

  function handleDelete() {
    if (!confirmDelete) {
      setConfirmDelete(true);
      setTimeout(() => setConfirmDelete(false), 3000);
      return;
    }
    onDelete(entry.id);
  }

  return (
    <div
      className={
        "rounded-lg border border-gray-200 bg-white transition-shadow hover:shadow-sm " +
        "dark:border-gray-700 dark:bg-gray-800"
      }
    >
      {/* Collapsed header — always visible */}
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-3 px-4 py-3 text-left"
      >
        {/* Expand chevron */}
        <svg
          className={
            "h-4 w-4 flex-shrink-0 text-gray-400 transition-transform " +
            (expanded ? "rotate-90" : "")
          }
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth={2}
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M8.25 4.5l7.5 7.5-7.5 7.5"
          />
        </svg>

        {/* Timestamp */}
        <span className="flex-shrink-0 text-xs text-gray-400 dark:text-gray-500">
          {formatTimestamp(entry.timestamp)}
        </span>

        {/* Truncated text */}
        <span className="min-w-0 flex-1 truncate text-sm text-gray-700 dark:text-gray-300">
          {truncate(displayText, 80)}
        </span>

        {/* Language badge */}
        <span
          className={
            "flex-shrink-0 rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase " +
            badge.className
          }
        >
          {badge.label}
        </span>
      </button>

      {/* Expanded body */}
      {expanded && (
        <div className="border-t border-gray-100 px-4 pb-4 pt-3 dark:border-gray-700">
          {/* Original + Refined side by side (or stacked) */}
          <div
            className={
              entry.refined_text
                ? "grid gap-4 sm:grid-cols-2"
                : ""
            }
          >
            {/* Original text */}
            <div>
              <h4 className="mb-1 text-xs font-semibold uppercase tracking-wide text-gray-400 dark:text-gray-500">
                Original
              </h4>
              <p className="whitespace-pre-wrap text-sm text-gray-700 dark:text-gray-300">
                {entry.original_text}
              </p>
            </div>

            {/* Refined text */}
            {entry.refined_text && (
              <div>
                <h4 className="mb-1 text-xs font-semibold uppercase tracking-wide text-gray-400 dark:text-gray-500">
                  Refined
                </h4>
                <p className="whitespace-pre-wrap text-sm text-gray-700 dark:text-gray-300">
                  {entry.refined_text}
                </p>
              </div>
            )}
          </div>

          {/* Meta + actions row */}
          <div className="mt-3 flex items-center gap-3">
            {/* Provider badge */}
            <span className="rounded bg-gray-100 px-1.5 py-0.5 text-[10px] text-gray-500 dark:bg-gray-700 dark:text-gray-400">
              {entry.provider}
            </span>

            {/* Duration */}
            {entry.audio_duration_ms > 0 && (
              <span className="text-[10px] text-gray-400 dark:text-gray-500">
                {formatDuration(entry.audio_duration_ms)}
              </span>
            )}

            <div className="flex-1" />

            {/* Copy button */}
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                handleCopy();
              }}
              title="Copy to clipboard"
              className={
                "rounded p-1.5 text-gray-400 transition-colors hover:bg-gray-100 hover:text-gray-600 " +
                "dark:hover:bg-gray-700 dark:hover:text-gray-300"
              }
            >
              {copied ? (
                <svg
                  className="h-4 w-4 text-green-500"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth={2}
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M4.5 12.75l6 6 9-13.5"
                  />
                </svg>
              ) : (
                <svg
                  className="h-4 w-4"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth={1.5}
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M15.666 3.888A2.25 2.25 0 0013.5 2.25h-3c-1.03 0-1.9.693-2.166 1.638m7.332 0c.055.194.084.4.084.612v0a.75.75 0 01-.75.75H9.75a.75.75 0 01-.75-.75v0c0-.212.03-.418.084-.612m7.332 0c.646.049 1.288.11 1.927.184 1.1.128 1.907 1.077 1.907 2.185V19.5a2.25 2.25 0 01-2.25 2.25H6.75A2.25 2.25 0 014.5 19.5V6.257c0-1.108.806-2.057 1.907-2.185a48.208 48.208 0 011.927-.184"
                  />
                </svg>
              )}
            </button>

            {/* Delete button */}
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                handleDelete();
              }}
              title={confirmDelete ? "Click again to confirm" : "Delete"}
              className={
                "rounded p-1.5 transition-colors " +
                (confirmDelete
                  ? "text-red-500 hover:bg-red-50 dark:hover:bg-red-900/30"
                  : "text-gray-400 hover:bg-gray-100 hover:text-gray-600 dark:hover:bg-gray-700 dark:hover:text-gray-300")
              }
            >
              <svg
                className="h-4 w-4"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth={1.5}
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0"
                />
              </svg>
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
