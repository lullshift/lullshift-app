// @vitest-environment jsdom
// The Theme setting: System follows the OS, Light and Dark win over it, and the choice is saved
// with the other interface preferences and applied as data-theme on the root.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const saved: unknown[] = [];
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: { prefs?: unknown }) => {
    if (command === "set_ui_prefs") saved.push(args?.prefs);
    return Promise.resolve(null);
  },
}));

import { AppearanceSettings } from "./AppearanceSettings";
import { applyTheme, effectiveTheme, startThemeSync, themeOf } from "./theme";
import { loadUiPrefs, resetUiPrefsForTests, uiPrefsStore } from "./uiPrefsStore";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("theme logic", () => {
  it("reads unknown values as System", () => {
    expect(themeOf(undefined)).toBe("system");
    expect(themeOf("sepia")).toBe("system");
    expect(themeOf("dark")).toBe("dark");
  });

  it("System follows the OS and explicit values win over it", () => {
    expect(effectiveTheme("system", () => true)).toBe("dark");
    expect(effectiveTheme(undefined, () => false)).toBe("light");
    expect(effectiveTheme("light", () => true)).toBe("light");
    expect(effectiveTheme("dark", () => false)).toBe("dark");
  });

  it("applies data-theme, and System removes it", () => {
    const root = document.createElement("div");
    applyTheme("dark", root);
    expect(root.getAttribute("data-theme")).toBe("dark");
    applyTheme("light", root);
    expect(root.getAttribute("data-theme")).toBe("light");
    applyTheme("system", root);
    expect(root.hasAttribute("data-theme")).toBe(false);
  });
});

describe("theme at startup", () => {
  beforeEach(() => {
    resetUiPrefsForTests();
    window.localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
  });

  it("applies the remembered theme before the preferences load, then follows them", async () => {
    window.localStorage.setItem("lullshift.theme", "dark");
    const stop = startThemeSync();
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    // Older saved preferences have no theme: that is System, and the cache is corrected.
    await loadUiPrefs(() =>
      Promise.resolve({ detailPanelPinned: true, contentWidth: "full", onboardingDone: true }),
    );
    expect(document.documentElement.hasAttribute("data-theme")).toBe(false);
    expect(window.localStorage.getItem("lullshift.theme")).toBe("system");
    stop();
  });

  it("keeps the remembered theme when the preferences can't be read", async () => {
    window.localStorage.setItem("lullshift.theme", "light");
    const stop = startThemeSync();
    await loadUiPrefs(() => Promise.reject(new Error("store_unavailable")));
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
    stop();
  });

  it("works when the browser store is blocked", async () => {
    const get = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const set = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const stop = startThemeSync();
    expect(document.documentElement.hasAttribute("data-theme")).toBe(false);
    await loadUiPrefs(() =>
      Promise.resolve({
        detailPanelPinned: true,
        contentWidth: "full",
        onboardingDone: true,
        theme: "dark",
      }),
    );
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    stop();
    get.mockRestore();
    set.mockRestore();
  });
});

describe("Appearance > Theme", () => {
  let host: HTMLDivElement;
  let root: Root;
  let stop: () => void;

  beforeEach(() => {
    saved.length = 0;
    window.localStorage.clear();
    resetUiPrefsForTests();
    document.documentElement.removeAttribute("data-theme");
    stop = startThemeSync();
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => root.render(<AppearanceSettings />));
  });
  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    stop();
  });

  const radio = (value: string) =>
    host.querySelector<HTMLInputElement>(`input[name="theme"][value="${value}"]`)!;

  it("defaults to System", () => {
    expect(radio("system").checked).toBe(true);
    expect(document.documentElement.hasAttribute("data-theme")).toBe(false);
  });

  it("choosing Dark sets data-theme and saves it with the other preferences", async () => {
    await act(async () => {
      radio("dark").click();
    });
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    expect(radio("dark").checked).toBe(true);
    expect(uiPrefsStore.getState().prefs.theme).toBe("dark");
    expect(saved.at(-1)).toMatchObject({ theme: "dark", contentWidth: "full" });
    await act(async () => {
      radio("system").click();
    });
    expect(document.documentElement.hasAttribute("data-theme")).toBe(false);
  });
});
