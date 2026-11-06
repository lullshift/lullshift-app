import { useEffect, useRef } from "react";

/**
 * Moves focus to `target` only if it has been lost to the page body (WCAG 2.4.3). Focus that is
 * somewhere on purpose is left alone.
 */
export function focusIfLost(
  target: () => { focus: () => void } | null | undefined,
  doc: { activeElement: unknown; body: unknown } = document,
): boolean {
  if (doc.activeElement && doc.activeElement !== doc.body) return false;
  const el = target();
  if (!el) return false;
  el.focus();
  return true;
}

/**
 * A dialog gives focus back to the button that opened it. When the action it confirmed removes
 * that button (Connect becomes Disconnect), the browser has nowhere to return to, so focus drops
 * to the page. Call this with the dialog's open state and a fallback target.
 *
 * Effects run child first, so by the time this runs the dialog has already closed.
 */
export function useRestoreFocusAfterDialog(
  open: boolean,
  fallback: () => { focus: () => void } | null | undefined,
) {
  const wasOpen = useRef(false);
  useEffect(() => {
    if (wasOpen.current && !open) focusIfLost(fallback);
    wasOpen.current = open;
  });
}
