import { useState, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
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

/** Animated waveform bars — 5 bars with staggered animation. */
function Waveform({ color }: { color: string }) {
  const bars = [0, 1, 2, 3, 4];
  // Each bar gets a different animation delay for organic feel
  const delays = ["0s", "0.15s", "0.3s", "0.12s", "0.24s"];
  const heights = ["60%", "90%", "100%", "80%", "70%"];

  return (
    <div className="flex items-center gap-[3px]" style={{ height: 24 }}>
      {bars.map((i) => (
        <div
          key={i}
          className="w-[3px] rounded-full"
          style={{
            backgroundColor: color,
            height: heights[i],
            animation: "waveform 0.8s ease-in-out infinite alternate",
            animationDelay: delays[i],
          }}
        />
      ))}
    </div>
  );
}

/** Pulsing dot animation for processing states. */
function PulsingDots({ color }: { color: string }) {
  const dots = [0, 1, 2];
  const delays = ["0s", "0.2s", "0.4s"];

  return (
    <div className="flex items-center gap-1">
      {dots.map((i) => (
        <div
          key={i}
          className="h-[6px] w-[6px] rounded-full"
          style={{
            backgroundColor: color,
            animation: "pulse-dot 1.2s ease-in-out infinite",
            animationDelay: delays[i],
          }}
        />
      ))}
    </div>
  );
}

export default function Overlay() {
  const { t } = useTranslation();
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

  const isRecording = state.type === "Recording";
  const isProcessing =
    state.type === "Processing" || state.type === "Refining";
  const isDone = state.type === "Result" || state.type === "Refined";
  const isError = state.type === "Error";

  return (
    <div className="flex h-screen w-screen items-end justify-center pb-0">
      <style>{`
        @keyframes waveform {
          0% { transform: scaleY(0.3); }
          100% { transform: scaleY(1); }
        }
        @keyframes pulse-dot {
          0%, 100% { opacity: 0.3; transform: scale(0.8); }
          50% { opacity: 1; transform: scale(1.2); }
        }
      `}</style>
      <div
        className={
          "flex items-center gap-3 rounded-full px-5 py-2 shadow-lg backdrop-blur-md " +
          "transition-all duration-300 " +
          (isRecording
            ? "bg-red-900/80"
            : isProcessing
              ? "bg-blue-900/80"
              : isDone
                ? "bg-green-900/80"
                : "bg-gray-900/80")
        }
      >
        {isRecording && (
          <>
            <Waveform color="#f87171" />
            <span className="text-xs font-medium text-red-300">
              {t("recording")}
            </span>
            <Waveform color="#f87171" />
          </>
        )}

        {isProcessing && (
          <>
            <PulsingDots color="#60a5fa" />
            <span className="text-xs font-medium text-blue-300">
              {t("processing")}
            </span>
            <PulsingDots color="#60a5fa" />
          </>
        )}

        {isDone && (
          <>
            <svg
              className="h-4 w-4 text-green-400"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={3}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M4.5 12.75l6 6 9-13.5"
              />
            </svg>
            <span className="text-xs font-medium text-green-300">
              {t("done")}
            </span>
          </>
        )}

        {isError && (
          <>
            <svg
              className="h-4 w-4 text-red-400"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={3}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M6 18L18 6M6 6l12 12"
              />
            </svg>
            <span className="max-w-[160px] truncate text-xs font-medium text-red-300">
              {state.data?.message ?? t("error")}
            </span>
          </>
        )}
      </div>
    </div>
  );
}
