import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { Icon } from "./Icon";
import { dismissToast, TOAST_MS, useToasts, type Toast } from "./toastQueue";
import "./toasts.css";

function subscribeVisibility(fn: () => void): () => void {
  document.addEventListener("visibilitychange", fn);
  return () => document.removeEventListener("visibilitychange", fn);
}
const isVisible = () => document.visibilityState !== "hidden";

function ToastItem({ toast }: { toast: Toast }) {
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const visible = useSyncExternalStore(subscribeVisibility, isVisible, () => true);
  const paused = hovered || focused || !visible;

  // Auto-dismiss, paused while the pointer is over the notice, focus is inside it, or the window
  // is hidden (a notice must still be there when the user comes back). Leaving or returning
  // starts the full time again.
  useEffect(() => {
    if (paused) return;
    const timer = window.setTimeout(() => dismissToast(toast.id), TOAST_MS);
    return () => window.clearTimeout(timer);
  }, [paused, toast.id]);

  // Closing a notice that holds focus must not drop focus on the page body: it goes to the next
  // notice's first button, else to the main area.
  const ref = useRef<HTMLDivElement>(null);
  const close = () => {
    const el = ref.current;
    if (el?.contains(document.activeElement)) {
      const all = [...document.querySelectorAll<HTMLElement>(".toast-region .toast")];
      const i = all.indexOf(el);
      const next = [...all.slice(i + 1), ...all.slice(0, Math.max(i, 0))][0];
      (
        next?.querySelector<HTMLElement>("button") ?? document.getElementById("main-content")
      )?.focus();
    }
    dismissToast(toast.id);
  };

  const { action } = toast;
  return (
    <div
      ref={ref}
      className={`toast toast-${toast.tone}`}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onFocus={() => setFocused(true)}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setFocused(false);
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          close();
        }
      }}
    >
      <Icon name={toast.tone === "info" ? "chat" : "alert"} />
      <p className="toast-title">{toast.title}</p>
      <div className="toast-actions">
        {action && (
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => {
              dismissToast(toast.id);
              action.onAction();
            }}
          >
            {action.label}
          </button>
        )}
        <button
          type="button"
          className="btn btn-quiet toast-dismiss"
          aria-label={`Dismiss: ${toast.title}`}
          onClick={close}
        >
          <Icon name="close" size={16} />
        </button>
      </div>
    </div>
  );
}

/**
 * The notices, in two live regions that are always in the page (a region added together with its
 * text is not reliably announced). Polite for news; assertive only for something waiting on you.
 * Nothing here moves focus. The notices are the last thing in the tab order.
 */
export function ToastRegion() {
  const toasts = useToasts();
  const polite = toasts.filter((t) => !t.assertive);
  const assertive = toasts.filter((t) => t.assertive);
  return (
    <div className="toast-region">
      <div role="status" aria-live="polite" aria-label="Notices">
        {polite.map((t) => (
          <ToastItem key={t.id} toast={t} />
        ))}
      </div>
      <div role="alert" aria-live="assertive" aria-label="Notices that need you">
        {assertive.map((t) => (
          <ToastItem key={t.id} toast={t} />
        ))}
      </div>
    </div>
  );
}
