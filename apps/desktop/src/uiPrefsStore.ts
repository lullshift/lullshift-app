// Interface preferences (pinned detail panel, content width), shared across views and saved
// in the local preferences table. Each change saves the whole set, so one setting never
// overwrites another.

import { useSyncExternalStore } from "react";
import { createStore } from "zustand/vanilla";
import { setUiPrefs, uiPrefs, type UiPrefs } from "./context/api";

export const DEFAULT_UI_PREFS: UiPrefs = { detailPanelPinned: true, contentWidth: "full" };

interface UiPrefsState {
  prefs: UiPrefs;
  loaded: boolean;
}

export const uiPrefsStore = createStore<UiPrefsState>()(() => ({
  prefs: DEFAULT_UI_PREFS,
  loaded: false,
}));

let loading: Promise<void> | null = null;

/** Reads the saved preferences once. If they can't be read, the defaults stay. */
export function loadUiPrefs(read: () => Promise<UiPrefs> = uiPrefs): Promise<void> {
  loading ??= read()
    .then((p) => uiPrefsStore.setState({ prefs: { ...DEFAULT_UI_PREFS, ...p }, loaded: true }))
    .catch(() => uiPrefsStore.setState({ loaded: true }));
  return loading;
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

/** For tests. */
export function resetUiPrefsForTests() {
  loading = null;
  uiPrefsStore.setState({ prefs: DEFAULT_UI_PREFS, loaded: false });
}
