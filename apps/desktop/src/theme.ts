// The Theme setting (ADR-026 section 5): System follows the OS, Light and Dark are explicit.
// The choice is saved with the interface preferences and applied as data-theme on the root;
// tokens.css lets an explicit value win over prefers-color-scheme.

import type { Theme } from "./context/api";
import { uiPrefsStore } from "./uiPrefsStore";

export const THEMES: readonly Theme[] = ["system", "light", "dark"];

/** Anything that isn't a known value reads as System. */
export const themeOf = (value: unknown): Theme =>
  value === "light" || value === "dark" ? value : "system";

const DARK_QUERY = "(prefers-color-scheme: dark)";

/** The theme actually shown: an explicit choice, or the OS preference for System. */
export function effectiveTheme(
  theme: Theme | undefined,
  prefersDark: () => boolean = () =>
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia(DARK_QUERY).matches,
): "light" | "dark" {
  const t = themeOf(theme);
  return t === "system" ? (prefersDark() ? "dark" : "light") : t;
}

/** Sets or clears data-theme on the root. System removes it, so the media query decides. */
export function applyTheme(theme: Theme | undefined, root: HTMLElement = document.documentElement) {
  const t = themeOf(theme);
  if (t === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", t);
}

/** Applies the saved theme now and whenever the preferences change. Returns the unsubscribe. */
export function startThemeSync(): () => void {
  applyTheme(uiPrefsStore.getState().prefs.theme);
  let last = uiPrefsStore.getState().prefs.theme;
  return uiPrefsStore.subscribe((s) => {
    if (s.prefs.theme === last) return;
    last = s.prefs.theme;
    applyTheme(last);
  });
}
