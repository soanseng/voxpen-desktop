import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { DictionaryEntry } from "../../types/dictionary";
import {
  addDictionaryEntry,
  deleteDictionaryEntry,
  getDictionaryEntries,
} from "../../lib/tauri";

export default function VocabularySection() {
  const { t } = useTranslation();
  const [entries, setEntries] = useState<DictionaryEntry[]>([]);
  const [inputValue, setInputValue] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const fetchEntries = async () => {
    try {
      const data = await getDictionaryEntries();
      setEntries(data);
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchEntries();
  }, []);

  const handleAdd = async () => {
    const trimmed = inputValue.trim();
    if (!trimmed) return;

    // Check for duplicate locally
    if (entries.some((e) => e.word === trimmed)) {
      setError(t("dictionary.duplicate"));
      setTimeout(() => setError(null), 2000);
      setInputValue("");
      inputRef.current?.focus();
      return;
    }

    try {
      await addDictionaryEntry(trimmed);
      setInputValue("");
      setError(null);
      await fetchEntries();
      inputRef.current?.focus();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDelete = async (id: number) => {
    try {
      await deleteDictionaryEntry(id);
      await fetchEntries();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      handleAdd();
    }
  };

  if (loading) {
    return (
      <div className="text-sm text-gray-500 dark:text-gray-400">
        {t("loadingSettings")}
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-base font-semibold text-gray-900 dark:text-gray-100">
          {t("dictionary.title")}
        </h2>
        <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
          {t("dictionary.description")}
        </p>
      </div>

      {/* Add word input */}
      <div className="flex gap-2">
        <input
          ref={inputRef}
          type="text"
          value={inputValue}
          onChange={(e) => setInputValue(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={t("dictionary.addHint")}
          className="flex-1 rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-900 placeholder-gray-400 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100 dark:placeholder-gray-500"
        />
        <button
          type="button"
          onClick={handleAdd}
          disabled={!inputValue.trim()}
          className="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50 dark:bg-blue-500 dark:hover:bg-blue-600"
        >
          {t("dictionary.addButton")}
        </button>
      </div>

      {/* Error/status message */}
      {error && (
        <p className="text-sm text-amber-600 dark:text-amber-400">{error}</p>
      )}

      {/* Word count */}
      <p className="text-sm text-gray-500 dark:text-gray-400">
        {t("dictionary.count", { count: entries.length })}
      </p>

      {/* Entry list */}
      {entries.length === 0 ? (
        <div className="rounded-lg border border-dashed border-gray-300 p-6 text-center dark:border-gray-600">
          <p className="text-sm text-gray-500 dark:text-gray-400">
            {t("dictionary.empty")}
          </p>
        </div>
      ) : (
        <ul className="divide-y divide-gray-200 rounded-lg border border-gray-200 dark:divide-gray-700 dark:border-gray-700">
          {entries.map((entry) => (
            <li
              key={entry.id}
              className="flex items-center justify-between px-4 py-3"
            >
              <span className="text-sm text-gray-900 dark:text-gray-100">
                {entry.word}
              </span>
              <button
                type="button"
                onClick={() => handleDelete(entry.id)}
                className="rounded p-1 text-gray-400 transition-colors hover:bg-gray-100 hover:text-gray-600 dark:hover:bg-gray-700 dark:hover:text-gray-300"
                aria-label={`${t("delete")} ${entry.word}`}
              >
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
                    d="M6 18L18 6M6 6l12 12"
                  />
                </svg>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
