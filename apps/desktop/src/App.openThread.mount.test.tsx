// @vitest-environment jsdom
// A notice's Open button (companion/events openThread) takes you to Companion and puts focus on
// the view heading, from another view and when already there.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string) => {
    switch (command) {
      case "dashboard":
        return Promise.resolve({
          sessions: [],
          store: { status: "ok" },
          ipc: { status: "ok" },
          staleAfterSeconds: 600,
        });
      case "companion_profiles":
      case "companion_threads":
        return Promise.resolve([]);
      case "companion_prefs":
        return Promise.resolve({ toolsEnabled: false, lastScope: null });
      default:
        return Promise.reject(new Error("unavailable"));
    }
  },
  Channel: class {},
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

import App from "./App";
import { openThread } from "./companion/events";
import { polyfillDialog, settle } from "./companion/testDom";

polyfillDialog();

let host: HTMLDivElement;
let root: Root;

beforeEach(async () => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => {
    root.render(<App />);
  });
  await settle();
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  vi.useRealTimers();
});

const companionNav = () =>
  [...host.querySelectorAll<HTMLButtonElement>(".nav-item")].find((b) =>
    b.textContent?.includes("Companion"),
  )!;

describe("Open in a notice", () => {
  it("goes to Companion and focuses its heading", async () => {
    expect(companionNav().getAttribute("aria-current")).toBeNull();
    await act(async () => openThread(3));
    await settle();
    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });
    expect(companionNav().getAttribute("aria-current")).toBe("page");
    expect(document.activeElement?.id).toBe("companion-heading");
  });

  it("focuses the heading when Companion is already showing", async () => {
    await act(async () => companionNav().click());
    await settle();
    host.querySelector<HTMLElement>("main")!.focus();
    await act(async () => openThread(3));
    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });
    expect(document.activeElement?.id).toBe("companion-heading");
  });
});
