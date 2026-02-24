import type { TranscriptionEntry } from "../../types/history";
import HistoryEntry from "./HistoryEntry";

interface HistoryListProps {
  entries: TranscriptionEntry[];
  onDelete: (id: string) => void;
}

export default function HistoryList({ entries, onDelete }: HistoryListProps) {
  if (entries.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-16">
        <svg
          className="mb-4 h-12 w-12 text-gray-300 dark:text-gray-600"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth={1}
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m0 12.75h7.5m-7.5 3H12M10.5 2.25H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z"
          />
        </svg>
        <p className="text-sm text-gray-500 dark:text-gray-400">
          No transcriptions yet
        </p>
        <p className="mt-1 text-xs text-gray-400 dark:text-gray-500">
          Use the global hotkey to start recording.
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-2">
      {entries.map((entry) => (
        <HistoryEntry key={entry.id} entry={entry} onDelete={onDelete} />
      ))}
    </div>
  );
}
