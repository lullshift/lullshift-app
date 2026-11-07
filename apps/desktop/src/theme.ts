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

const CACHE_KEY = "lullshift.theme";

/** The theme remembered by the last run, so the first paint isn't the wrong one. */
export function cachedTheme(): Theme | undefined {
  try {
    const v = window.localStorage.getItem(CACHE_KEY);
    return v === "light" || v === "dark" || v === "system" ? v : undefined;
  } catch {
    return undefined;
  }
}

function cacheTheme(theme: Theme) {
  try {
    window.localStorage.setItem(CACHE_KEY, theme);
  } catch {
    // A blocked or full store only costs the flash-free start.
  }
}

/**
 * Applies the remembered theme at once (before the first render, so there is no flash), then
 * the saved preference as soon as it is known and whenever it changes. The preference is the
 * truth; the cache only covers the moment before it loads, and a failed load keeps the cache.
 * Returns the unsubscribe.
 */
export function startThemeSync(): () => void {
  applyTheme(cachedTheme());
  let last: Theme | undefined = cachedTheme();
  return uiPrefsStore.subscribe((s) => {
    // Known: the preferences were read, or the user already changed the theme.
    const known = s.loaded ? !s.loadFailed : s.prefs.theme !== undefined;
    if (!known) return;
    const next = themeOf(s.prefs.theme);
    if (next === last) return;
    last = next;
    applyTheme(next);
    cacheTheme(next);
  });
}
