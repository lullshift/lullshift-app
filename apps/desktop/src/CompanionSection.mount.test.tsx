// @vitest-environment jsdom
// The Companion section of Settings (decision 36): collapsed by default and then nothing is read
// from the keychain; it expands with a disclosure button whose state is saved, and the Companion
// view's "Provider settings" link opens and focuses it.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const calls: string[] = [];
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string) => {
    calls.push(command);
    if (command === "companion_profiles") return Promise.resolve([]);
    if (command === "companion_prefs")
      return Promise.resolve({ toolsEnabled: false, lastScope: null });
    if (command === "companion_usage") return Promise.resolve([]);
    return Promise.resolve(null);
  },
  Channel: class {},
}));

import { click, polyfillDialog, settle } from "./companion/testDom";
import { CompanionSection } from "./Settings";
import { resetUiPrefsForTests, uiPrefsStore } from "./uiPrefsStore";

polyfillDialog();

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  calls.length = 0;
  resetUiPrefsForTests();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

const toggle = () => host.querySelector<HTMLButtonElement>(".disclosure-button")!;

describe("CompanionSection", () => {
  it("starts collapsed, reads nothing, and expands on click with the state saved", async () => {
    await act(async () => root.render(<CompanionSection focusRequest={0} />));
    await settle();
    expect(toggle().getAttribute("aria-expanded")).toBe("false");
    const body = document.getElementById(toggle().getAttribute("aria-controls")!)!;
    expect(body.hidden).toBe(true);
    expect(calls.filter((c) => c.startsWith("companion_"))).toEqual([]);

    await click(toggle());
    expect(toggle().getAttribute("aria-expanded")).toBe("true");
    expect(body.hidden).toBe(false);
    expect(calls).toContain("set_ui_prefs");
    expect(uiPrefsStore.getState().prefs.companionSettingsOpen).toBe(true);
    expect(calls).toContain("companion_profiles");
    expect(host.textContent).toContain("Add a provider");
  });

  it("opens and takes focus when the Companion view asks", async () => {
    await act(async () => root.render(<CompanionSection focusRequest={1} />));
    await settle();
    expect(toggle().getAttribute("aria-expanded")).toBe("true");
    expect(document.activeElement).toBe(toggle());
  });
});
