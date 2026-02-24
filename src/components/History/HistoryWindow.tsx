import { useCallback, useEffect, useRef, useState } from "react";
import type { TranscriptionEntry } from "../../types/history";
import { deleteHistoryEntry, getHistory, searchHistory } from "../../lib/tauri";
import HistoryList from "./HistoryList";

const PAGE_SIZE = 50;

export default function HistoryWindow() {
  const [entries, setEntries] = useState<TranscriptionEntry[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const debounceTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const fetchEntries = useCallback(async (searchQuery: string) => {
    setLoading(true);
    try {
      const result =
        searchQuery.trim().length > 0
          ? await searchHistory(searchQuery.trim(), PAGE_SIZE, 0)
          : await getHistory(PAGE_SIZE, 0);
      setEntries(result);
    } catch {
      // Tauri backend not available (e.g. during dev), show empty
      setEntries([]);
    } finally {
      setLoading(false);
    }
  }, []);

  // Initial load
  useEffect(() => {
    fetchEntries("");
  }, [fetchEntries]);

  function handleSearchChange(value: string) {
    setQuery(value);

    if (debounceTimer.current !== null) {
      clearTimeout(debounceTimer.current);
    }

    debounceTimer.current = setTimeout(() => {
      debounceTimer.current = null;
      fetchEntries(value);
    }, 300);
  }

  async function handleDelete(id: string) {
    try {
      await deleteHistoryEntry(id);
      setEntries((prev) => prev.filter((e) => e.id !== id));
    } catch {
      // Delete failed silently
    }
  }

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">
          History
        </h2>
        <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
          Browse and search your past transcriptions.
        </p>
      </div>

      {/* Search bar */}
      <div className="relative">
        <svg
          className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-gray-400 dark:text-gray-500"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth={2}
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 105.196 5.196a7.5 7.5 0 0010.607 10.607z"
          />
        </svg>
        <input
          type="text"
          value={query}
          onChange={(e) => handleSearchChange(e.target.value)}
          placeholder="Search transcriptions..."
          className={
            "w-full rounded-lg border border-gray-300 bg-white py-2 pl-10 pr-4 text-sm " +
            "text-gray-900 placeholder:text-gray-400 " +
            "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
            "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100 dark:placeholder:text-gray-500"
          }
        />
      </div>

      {/* Content */}
      {loading ? (
        <div className="flex items-center justify-center py-12">
          <svg
            className="h-5 w-5 animate-spin text-gray-400"
            viewBox="0 0 24 24"
            fill="none"
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
        </div>
      ) : (
        <HistoryList entries={entries} onDelete={handleDelete} />
      )}
    </div>
  );
}
