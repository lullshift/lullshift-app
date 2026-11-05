import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import {
  DEFAULT_UI_PREFS,
  loadUiPrefs,
  resetUiPrefsForTests,
  uiPrefsStore,
  updateUiPrefs,
} from "./uiPrefsStore";

describe("interface preferences", () => {
  beforeEach(() => resetUiPrefsForTests());

  it("defaults to a pinned panel and full width", () => {
    expect(DEFAULT_UI_PREFS).toEqual({ detailPanelPinned: true, contentWidth: "full" });
  });

  it("keeps the defaults when the saved preferences can't be read", async () => {
    await loadUiPrefs(() => Promise.reject(new Error("store_unavailable")));
    expect(uiPrefsStore.getState()).toEqual({ prefs: DEFAULT_UI_PREFS, loaded: true });
  });

  it("saves the whole set, so one setting never overwrites another", async () => {
    await loadUiPrefs(() =>
      Promise.resolve({ detailPanelPinned: false, contentWidth: "comfortable" }),
    );
    const save = vi.fn(() => Promise.resolve(null));
    await updateUiPrefs({ contentWidth: "full" }, save);
    expect(save).toHaveBeenCalledWith({ detailPanelPinned: false, contentWidth: "full" });
    await updateUiPrefs({ detailPanelPinned: true }, save);
    expect(save).toHaveBeenLastCalledWith({ detailPanelPinned: true, contentWidth: "full" });
  });

  it("keeps the change for now when saving fails", async () => {
    await updateUiPrefs({ contentWidth: "comfortable" }, () => Promise.reject(new Error("x")));
    expect(uiPrefsStore.getState().prefs.contentWidth).toBe("comfortable");
  });
});
