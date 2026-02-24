import { useState, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

interface PipelineEvent {
  type:
    | "Idle"
    | "Recording"
    | "Processing"
    | "Result"
    | "Refining"
    | "Refined"
    | "Error";
  data?: {
    text?: string;
    message?: string;
    original?: string;
    refined?: string;
  };
}

function RecordingIndicator() {
  return (
    <div className="flex items-center gap-3">
      <span className="relative flex h-4 w-4">
        <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-red-400 opacity-75" />
        <span className="relative inline-flex h-4 w-4 rounded-full bg-red-500" />
      </span>
      <span className="text-sm font-medium text-white">Recording...</span>
    </div>
  );
}

function ProcessingIndicator() {
  return (
    <div className="flex items-center gap-3">
      <svg
        className="h-5 w-5 animate-spin text-blue-400"
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
      <span className="text-sm font-medium text-white">Processing...</span>
    </div>
  );
}

function DoneIndicator() {
  return (
    <div className="flex items-center gap-3">
      <svg
        className="h-5 w-5 text-green-400"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={2.5}
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          d="M4.5 12.75l6 6 9-13.5"
        />
      </svg>
      <span className="text-sm font-medium text-white">Done</span>
    </div>
  );
}

function ErrorIndicator({ message }: { message: string }) {
  return (
    <div className="flex items-center gap-3">
      <svg
        className="h-5 w-5 text-red-400"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={2.5}
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          d="M6 18L18 6M6 6l12 12"
        />
      </svg>
      <span className="max-w-[140px] truncate text-sm font-medium text-white">
        {message}
      </span>
    </div>
  );
}

export default function Overlay() {
  const [state, setState] = useState<PipelineEvent>({ type: "Idle" });
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    const unlisten = listen<PipelineEvent>("pipeline-state", (event) => {
      if (hideTimer.current !== null) {
        clearTimeout(hideTimer.current);
        hideTimer.current = null;
      }

      setState(event.payload);

      const eventType = event.payload.type;
      if (eventType === "Result" || eventType === "Refined") {
        hideTimer.current = setTimeout(() => {
          setState({ type: "Idle" });
          hideTimer.current = null;
        }, 1500);
      } else if (eventType === "Error") {
        hideTimer.current = setTimeout(() => {
          setState({ type: "Idle" });
          hideTimer.current = null;
        }, 3000);
      }
    });

    return () => {
      unlisten.then((fn) => fn());
      if (hideTimer.current !== null) {
        clearTimeout(hideTimer.current);
      }
    };
  }, []);

  if (state.type === "Idle") {
    return null;
  }

  return (
    <div className="flex h-screen w-screen items-center justify-center">
      <div className="rounded-2xl bg-gray-900/80 px-5 py-3 shadow-lg backdrop-blur-md">
        {state.type === "Recording" && <RecordingIndicator />}
        {state.type === "Processing" && <ProcessingIndicator />}
        {state.type === "Refining" && <ProcessingIndicator />}
        {(state.type === "Result" || state.type === "Refined") && (
          <DoneIndicator />
        )}
        {state.type === "Error" && (
          <ErrorIndicator message={state.data?.message ?? "Unknown error"} />
        )}
      </div>
    </div>
  );
}
