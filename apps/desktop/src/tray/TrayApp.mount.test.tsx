// @vitest-environment jsdom
// ADR-026 section 6: the tray window shows needs-you first, suggests something that fits, asks the
// main window (never runs anything itself), focuses its first useful control when shown and hides
// on Esc.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const activity = (id: string, name: string, category: string, savedUtc: string | null = null) => ({
  id,
  name,
  description: `About ${name}`,
  category,
  tags: [],
  version: "1.0.0",
  storage: true,
  savedUtc,
  help: null,
  preview: false,
  playedSeconds: 0,
  engine: null,
});

const state = vi.hoisted(() => ({
  sessions: [] as unknown[],
  activities: [] as unknown[],
  calls: [] as { command: string; args: unknown }[],
  handlers: new Map<string, () => void>(),
  openFails: false,
  dashboardFails: false,
  welcomeDone: true,
}));

const session = (st: string) => ({
  provider: "claude-code",
  providerSessionId: "abc12345",
  state: st,
  reason: "observed",
  lastKnownState: st,
  lastSeenUtc: new Date().toISOString(),
  lastReceivedUtc: new Date().toISOString(),
  needsReview: false,
  userTitle: null,
  agentTitle: null,
  intentLine: null,
  disconnectedUtc: null,
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: unknown) => {
    state.calls.push({ command, args });
    switch (command) {
      case "dashboard":
        return state.dashboardFails
          ? Promise.reject(new Error("x"))
          : Promise.resolve({
              sessions: state.sessions,
              store: { status: "ok" },
              ipc: { status: "ok" },
              staleAfterSeconds: 600,
            });
      case "activities_list":
        return Promise.resolve(state.activities);
      case "wait_estimates":
        return Promise.resolve([]);
      case "ui_prefs":
        return Promise.resolve({ detailPanelPinned: true, onboardingDone: state.welcomeDone });
      case "open_main":
        return state.openFails ? Promise.reject(new Error("refused")) : Promise.resolve(null);
      case "tray_hide":
        return Promise.resolve(null);
      default:
        return Promise.reject(new Error("unavailable"));
    }
  },
  Channel: class {},
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, handler: () => void) => {
    state.handlers.set(event, handler);
    return Promise.resolve(() => state.handlers.delete(event));
  },
}));

import { settle } from "../companion/testDom";
import { resetUiPrefsForTests } from "../uiPrefsStore";
import TrayApp from "./TrayApp";

let host: HTMLDivElement;
let root: Root;

async function mount() {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => {
    root.render(<TrayApp />);
  });
  await settle();
}

beforeEach(() => {
  state.sessions = [];
  state.activities = [
    activity("g1", "Tide Game", "arcade"),
    activity("b1", "Slow Breath", "break"),
    activity("b2", "Saved Stretch", "break", "2020-01-01T00:00:00Z"),
  ];
  state.calls = [];
  state.handlers.clear();
  state.openFails = false;
  state.dashboardFails = false;
  state.welcomeDone = true;
  resetUiPrefsForTests();
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

const button = (text: string) =>
  [...host.querySelectorAll("button")].find((b) => b.textContent?.includes(text))!;
const opened = () => state.calls.filter((c) => c.command === "open_main").map((c) => c.args);

describe("TrayApp", () => {
  it("asks the main window to resume the newest save, and runs nothing itself", async () => {
    await mount();
    expect(host.querySelector("h1")?.textContent).toBe("No agents yet");
    await act(async () => button("Resume").click());
    expect(opened()).toEqual([{ view: "rest", activityId: "b2", sessionKey: null }]);
    expect(state.calls.some((c) => c.command.startsWith("activity_"))).toBe(false);
  });

  it("puts the needs-you card first and sends Go to session with the session key", async () => {
    state.sessions = [session("needs_attention")];
    await mount();
    expect(host.querySelector("h1")?.textContent).toBe("Claude Code needs you");
    const sections = [...host.querySelectorAll("section")].map((s) => s.className);
    expect(sections[0]).toContain("tray-needs");
    await act(async () => button("Go to session").click());
    expect(opened()).toEqual([
      {
        view: "sessions",
        activityId: null,
        sessionKey: { provider: "claude-code", sessionId: "abc12345" },
      },
    ]);
  });

  it("footer: Library opens Play and Talk opens Companion; Open Lullshift opens Home", async () => {
    await mount();
    await act(async () => button("Library").click());
    await act(async () => button("Talk").click());
    await act(async () => button("Open Lullshift").click());
    expect(opened().map((a) => (a as { view: string }).view)).toEqual([
      "play",
      "companion",
      "home",
    ]);
    expect(button("Now").getAttribute("aria-current")).toBe("page");
  });

  it("shows up to three chips, never the suggestion itself", async () => {
    await mount();
    const chips = [...host.querySelectorAll(".tray-chip")].map((c) => c.textContent);
    expect(chips.length).toBeLessThanOrEqual(3);
    expect(chips).not.toContain("Saved Stretch");
    await act(async () => (host.querySelector(".tray-chip") as HTMLButtonElement).click());
    expect((opened()[0] as { activityId: string }).activityId).not.toBe("b2");
  });

  it("says so when opening is refused", async () => {
    state.openFails = true;
    await mount();
    await act(async () => button("Open Lullshift").click());
    await settle();
    expect(host.textContent).toContain("couldn't open that");
  });

  it("says the view may be out of date when a refresh fails (the store keeps the last good one)", async () => {
    await mount();
    state.dashboardFails = true;
    await act(async () => state.handlers.get("tray-shown")?.());
    await settle();
    expect(host.textContent).toContain("couldn't refresh just now");
    expect(host.querySelector('[role="alert"]')).not.toBeNull();
  });

  it("focuses the first useful control when shown, and Esc hides", async () => {
    state.sessions = [session("needs_attention")];
    await mount();
    await act(async () => state.handlers.get("tray-shown")?.());
    await settle();
    expect(document.activeElement).toBe(button("Go to session"));
    await act(async () => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });
    expect(state.calls.some((c) => c.command === "tray_hide")).toBe(true);
  });

  it("offers only Open Lullshift until the welcome is done in the main window", async () => {
    state.welcomeDone = false;
    state.sessions = [session("needs_attention")];
    await mount();
    const labels = [...host.querySelectorAll("button")].map((b) => b.textContent?.trim());
    expect(labels).toEqual(["Open Lullshift"]);
    expect(host.querySelector(".tray-needs, .tray-suggest, .tray-tabs")).toBeNull();
  });

  it("focuses the suggestion's button when nothing needs you", async () => {
    await mount();
    await act(async () => state.handlers.get("tray-shown")?.());
    await settle();
    expect(document.activeElement).toBe(button("Resume"));
  });
});
