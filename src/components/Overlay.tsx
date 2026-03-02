import { useState, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "../lib/tauri";

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

const PURCHASE_URL = "https://anatomind.lemonsqueezy.com/checkout/buy/299dd747-4424-4b1b-8aa8-2c47a94f7dd1";

export default function Overlay() {
  const { t } = useTranslation();
  const [state, setState] = useState<PipelineEvent>({ type: "Idle" });
  const [usageRemaining, setUsageRemaining] = useState<number | null>(null);
  const [usageExhausted, setUsageExhausted] = useState(false);
  const [promoExpired, setPromoExpired] = useState(false);
  const [timedOut, setTimedOut] = useState(false);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const exhaustedTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    const unlisten = listen<PipelineEvent>("pipeline-state", (event) => {
      if (hideTimer.current !== null) {
        clearTimeout(hideTimer.current);
        hideTimer.current = null;
      }

      setState(event.payload);

      // Reset timed-out flag and usage warning when a new recording starts
      if (event.payload.type === "Recording") {
        setTimedOut(false);
      }

      // Clear usage warning when not recording
      if (event.payload.type !== "Recording") {
        setUsageRemaining(null);
      }

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

    const unlistenWarning = listen<number>("usage-warning", (event) => {
      setUsageRemaining(event.payload);
    });

    const unlistenTimeout = listen<number>("recording-timed-out", () => {
      setTimedOut(true);
    });

    const unlistenPromoExpired = listen("promo-expired", () => {
      setPromoExpired(true);
      getCurrentWindow().setIgnoreCursorEvents(false).catch(() => {});
    });

    const unlistenExhausted = listen("usage-exhausted", () => {
      setUsageExhausted(true);
      // Make overlay clickable for the upgrade button
      getCurrentWindow().setIgnoreCursorEvents(false).catch(() => {});

      if (exhaustedTimer.current !== null) {
        clearTimeout(exhaustedTimer.current);
      }
      exhaustedTimer.current = setTimeout(() => {
        setUsageExhausted(false);
        getCurrentWindow().setIgnoreCursorEvents(true).catch(() => {});
        exhaustedTimer.current = null;
      }, 5000);
    });

    return () => {
      unlisten.then((fn) => fn());
      unlistenWarning.then((fn) => fn());
      unlistenTimeout.then((fn) => fn());
      unlistenExhausted.then((fn) => fn());
      unlistenPromoExpired.then((fn) => fn());
      if (hideTimer.current !== null) {
        clearTimeout(hideTimer.current);
      }
      if (exhaustedTimer.current !== null) {
        clearTimeout(exhaustedTimer.current);
      }
    };
  }, []);

  if (state.type === "Idle" && !usageExhausted && !promoExpired) {
    return null;
  }

  // Exhausted state: amber overlay with upgrade button
  if (usageExhausted) {
    return (
      <div className="flex h-screen w-screen items-end justify-center pb-0">
        <div className="flex items-center gap-3 rounded-full bg-amber-900/90 px-5 py-2 shadow-lg backdrop-blur-md">
          <svg
            className="h-4 w-4 text-amber-400"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z"
            />
          </svg>
          <span className="text-xs font-medium text-amber-300">
            {t("license.exhausted")}
          </span>
          <button
            type="button"
            onClick={() => {
              void openUrl(PURCHASE_URL);
            }}
            className="rounded-full bg-amber-500 px-3 py-1 text-xs font-medium text-white hover:bg-amber-400"
          >
            {t("license.upgradePrompt")}
          </button>
        </div>
      </div>
    );
  }

  if (promoExpired) {
    return (
      <div className="flex h-screen w-screen items-end justify-center pb-0">
        <div className="flex flex-col items-center gap-2 rounded-2xl bg-amber-900/90 px-6 py-3 shadow-lg backdrop-blur-md">
          <div className="flex items-center gap-2">
            <svg
              className="h-4 w-4 text-amber-400"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
            <span className="text-xs font-semibold text-amber-200">
              {t("license.promoExpiredTitle")}
            </span>
          </div>
          <span className="text-[11px] text-amber-300/80">
            {t("license.promoExpiredMessage")}
          </span>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => {
                void openUrl(PURCHASE_URL);
                setPromoExpired(false);
                getCurrentWindow().setIgnoreCursorEvents(true).catch(() => {});
              }}
              className="rounded-full bg-amber-500 px-3 py-1 text-xs font-medium text-white hover:bg-amber-400"
            >
              {t("license.promoExpiredUpgrade")}
            </button>
            <button
              type="button"
              onClick={() => {
                setPromoExpired(false);
                getCurrentWindow().setIgnoreCursorEvents(true).catch(() => {});
              }}
              className="rounded-full border border-amber-500/50 px-3 py-1 text-xs font-medium text-amber-300 hover:bg-amber-800/50"
            >
              {t("license.promoExpiredDismiss")}
            </button>
          </div>
        </div>
      </div>
    );
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
          "flex flex-col items-center gap-1 " +
          "transition-all duration-300"
        }
      >
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
      {/* Usage warning subtitle during recording */}
      {isRecording && usageRemaining !== null && (
        <span className="text-[10px] font-medium text-amber-400/80">
          {t("license.warningRemaining", { count: usageRemaining })}
        </span>
      )}
      {/* Time limit reached indicator */}
      {timedOut && (
        <p className="text-xs text-yellow-400 mt-1 text-center">錄音時間上限已達</p>
      )}
      </div>
    </div>
  );
}
