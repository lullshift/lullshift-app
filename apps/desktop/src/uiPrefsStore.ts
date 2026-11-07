// Interface preferences (pinned detail panel, content width), shared across views and saved
// in the local preferences table. Each change saves the whole set, so one setting never
// overwrites another.

import { useSyncExternalStore } from "react";
import { createStore } from "zustand/vanilla";
import { setUiPrefs, uiPrefs, type UiPrefs } from "./context/api";

export const DEFAULT_UI_PREFS: UiPrefs = {
  detailPanelPinned: true,
  contentWidth: "full",
  onboardingDone: false,
  companionSettingsOpen: false,
};

interface UiPrefsState {
  prefs: UiPrefs;
  loaded: boolean;
  /** The saved preferences couldn't be read, so the defaults are only a guess. */
  loadFailed: boolean;
}

export const uiPrefsStore = createStore<UiPrefsState>()(() => ({
  prefs: DEFAULT_UI_PREFS,
  loaded: false,
  loadFailed: false,
}));

let loading: Promise<void> | null = null;

/** Reads the saved preferences once. If they can't be read, the defaults stay. */
export function loadUiPrefs(read: () => Promise<UiPrefs> = uiPrefs): Promise<void> {
  loading ??= read()
    .then((p) => uiPrefsStore.setState({ prefs: { ...DEFAULT_UI_PREFS, ...p }, loaded: true }))
    .catch(() => uiPrefsStore.setState({ loaded: true, loadFailed: true }));
  return loading;
}

/**
 * Reads the saved preferences again (the tray window calls this each time it is shown, since the
 * main window may have changed them while it was hidden). A failed read keeps what is there.
 */
export function reloadUiPrefs(read: () => Promise<UiPrefs> = uiPrefs): Promise<void> {
  return read().then(
    (p) =>
      uiPrefsStore.setState({
        prefs: { ...DEFAULT_UI_PREFS, ...p },
        loaded: true,
        loadFailed: false,
      }),
    () => undefined,
  );
}

/** Applies a change at once and saves the whole set. A failed save keeps the change for now. */
export function updateUiPrefs(
  change: Partial<UiPrefs>,
  save: (p: UiPrefs) => Promise<unknown> = setUiPrefs,
): Promise<void> {
  const prefs = { ...uiPrefsStore.getState().prefs, ...change };
  uiPrefsStore.setState({ prefs });
  return save(prefs).then(
    () => undefined,
    () => undefined,
  );
}

export function useUiPrefs(): UiPrefs {
  return useSyncExternalStore(uiPrefsStore.subscribe, () => uiPrefsStore.getState().prefs);
}

/**
 * The welcome shows only when the saved preferences were read and say it hasn't been closed yet.
 * If they can't be read we can't tell, so it stays away rather than greeting the user every time.
 */
export function shouldShowWelcome(s: Pick<UiPrefsState, "loaded" | "loadFailed" | "prefs">) {
  return s.loaded && !s.loadFailed && !s.prefs.onboardingDone;
}

export function useShowWelcome(): boolean {
  return useSyncExternalStore(uiPrefsStore.subscribe, () =>
    shouldShowWelcome(uiPrefsStore.getState()),
  );
}

/** For tests. */
export function resetUiPrefsForTests() {
  loading = null;
  uiPrefsStore.setState({ prefs: DEFAULT_UI_PREFS, loaded: false, loadFailed: false });
}
