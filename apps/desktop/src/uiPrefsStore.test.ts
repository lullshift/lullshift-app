import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import {
  DEFAULT_UI_PREFS,
  loadUiPrefs,
  resetUiPrefsForTests,
  shouldShowWelcome,
  uiPrefsStore,
  updateUiPrefs,
} from "./uiPrefsStore";

describe("interface preferences", () => {
  beforeEach(() => resetUiPrefsForTests());

  it("defaults to a pinned panel and full width", () => {
    expect(DEFAULT_UI_PREFS).toEqual({
      detailPanelPinned: true,
      contentWidth: "full",
      onboardingDone: false,
    });
  });

  it("keeps the defaults when the saved preferences can't be read", async () => {
    await loadUiPrefs(() => Promise.reject(new Error("store_unavailable")));
    expect(uiPrefsStore.getState()).toEqual({
      prefs: DEFAULT_UI_PREFS,
      loaded: true,
      loadFailed: true,
    });
  });

  it("saves the whole set, so one setting never overwrites another", async () => {
    await loadUiPrefs(() =>
      Promise.resolve({
        detailPanelPinned: false,
        contentWidth: "comfortable",
        onboardingDone: true,
      }),
    );
    const save = vi.fn(() => Promise.resolve(null));
    await updateUiPrefs({ contentWidth: "full" }, save);
    expect(save).toHaveBeenCalledWith({
      detailPanelPinned: false,
      contentWidth: "full",
      onboardingDone: true,
    });
    await updateUiPrefs({ detailPanelPinned: true }, save);
    expect(save).toHaveBeenLastCalledWith({
      detailPanelPinned: true,
      contentWidth: "full",
      onboardingDone: true,
    });
  });

  it("keeps the change for now when saving fails", async () => {
    await updateUiPrefs({ contentWidth: "comfortable" }, () => Promise.reject(new Error("x")));
    expect(uiPrefsStore.getState().prefs.contentWidth).toBe("comfortable");
  });

  it("shows the welcome only once the saved preferences were read and it isn't done", async () => {
    expect(shouldShowWelcome(uiPrefsStore.getState())).toBe(false); // not loaded yet
    await loadUiPrefs(() => Promise.resolve({ ...DEFAULT_UI_PREFS }));
    expect(shouldShowWelcome(uiPrefsStore.getState())).toBe(true);
    await updateUiPrefs({ onboardingDone: true }, () => Promise.resolve(null));
    expect(shouldShowWelcome(uiPrefsStore.getState())).toBe(false);
  });

  it("never greets again once saved as done, and stays quiet when the prefs can't be read", async () => {
    await loadUiPrefs(() => Promise.resolve({ ...DEFAULT_UI_PREFS, onboardingDone: true }));
    expect(shouldShowWelcome(uiPrefsStore.getState())).toBe(false);
    resetUiPrefsForTests();
    await loadUiPrefs(() => Promise.reject(new Error("store_unavailable")));
    expect(shouldShowWelcome(uiPrefsStore.getState())).toBe(false);
  });
});
