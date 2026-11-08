// @vitest-environment jsdom
// ADR-026: the app opens on Home, the needs-you card comes first, Play and Rest split by
// category, Continue lists saved activities, and Integrations lives in Settings.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const NOW = new Date().toISOString();
const activity = (id: string, name: string, category: string, savedUtc: string | null = null) => ({
  id,
  name,
  description: `About ${name}`,
  category,
  tags: category === "break" ? ["breathing"] : ["puzzle"],
  version: "1.0.0",
  storage: true,
  savedUtc,
  help: null,
  preview: false,
  playedSeconds: 0,
  engine: null,
});
const ACTIVITIES = [
  activity("g1", "Tide Game", "arcade"),
  activity("g2", "Saved Quiz", "interactive", NOW),
  activity("b1", "Slow Breath", "break"),
  activity("b2", "Saved Stretch", "break", "2020-01-01T00:00:00Z"),
];

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string) => {
    switch (command) {
      case "dashboard":
        return Promise.resolve({
          sessions: [
            {
              provider: "claude-code",
              providerSessionId: "abc12345",
              state: "needs_attention",
              reason: "observed",
              lastKnownState: "needs_attention",
              lastSeenUtc: new Date().toISOString(),
              lastReceivedUtc: new Date().toISOString(),
              needsReview: false,
              userTitle: null,
              agentTitle: null,
              intentLine: null,
              disconnectedUtc: null,
            },
          ],
          store: { status: "ok" },
          ipc: { status: "ok" },
          staleAfterSeconds: 600,
        });
      case "notes_overview":
        return Promise.resolve({
          truncated: false,
          sessions: [
            {
              provider: "claude-code",
              sessionId: "abc12345",
              lastReceivedUtc: new Date().toISOString(),
              userTitle: null,
              agentTitle: null,
              intent: null,
              notes: "a note",
              checklist: [1, 2, 3].map((id) => ({
                id,
                kind: "checklist",
                category: null,
                text: `Check ${id}`,
                done: id === 3,
                position: id,
                createdUtc: new Date().toISOString(),
              })),
            },
          ],
        });
      case "activities_list":
        return Promise.resolve(ACTIVITIES);
      case "wait_estimates":
        return Promise.resolve([]);
      case "claude_code_status":
      case "codex_status":
      case "cursor_status":
      case "opencode_status":
        return Promise.resolve({ status: "not_installed", settingsPath: null, events: [] });
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
});

const nav = (label: string) =>
  [...host.querySelectorAll<HTMLButtonElement>(".nav-item")].find((b) =>
    b.querySelector(".nav-label")?.textContent?.includes(label),
  )!;
const go = async (label: string) => {
  await act(async () => nav(label).click());
  await settle();
};
const cards = () =>
  [...host.querySelectorAll(".activity-card")].map((c) => c.textContent).join(" ");
const visibleText = () => host.querySelector("main")!.textContent ?? "";

describe("navigation", () => {
  it("lists Home, Sessions, the While you wait group, then Settings", () => {
    const labels = [...host.querySelectorAll(".nav-label")].map((e) => e.textContent);
    expect(labels).toEqual([
      "Home",
      "Sessions",
      "Notes",
      "Play",
      "Rest",
      "Continue",
      "Companion",
      "Settings",
    ]);
    expect(host.querySelector("#nav-wait")?.textContent).toBe("While you wait");
    expect(labels).not.toContain("Integrations");
    expect(nav("Home").getAttribute("aria-current")).toBe("page");
  });

  it("shows counts for Play, Rest and Continue, and the orange badge only on Sessions", () => {
    expect(nav("Play").querySelector(".nav-count")?.textContent).toBe("2");
    expect(nav("Rest").querySelector(".nav-count")?.textContent).toBe("2");
    expect(nav("Continue").querySelector(".nav-count")?.textContent).toBe("2");
    expect(nav("Sessions").querySelector(".nav-badge")?.textContent).toContain("1");
    expect(nav("Play").querySelector(".nav-badge")).toBeNull();
  });

  it("counts open checklist items on Notes, never with the orange badge", () => {
    expect(nav("Notes").querySelector(".nav-count")?.textContent).toBe("2");
    expect(nav("Notes").querySelector(".nav-badge")).toBeNull();
  });

  it("opens Notes with the user's text and Open session goes to that session", async () => {
    await go("Notes");
    expect(host.querySelector("h1")?.textContent).toBe("Notes");
    expect(visibleText()).toContain("a note");
    const open = [...host.querySelectorAll("button")].find((b) =>
      b.getAttribute("aria-label")?.startsWith("Open session"),
    )!;
    await act(async () => open.click());
    await settle();
    expect(host.querySelector("h1")?.textContent).toBe("Sessions");
    expect(nav("Sessions").getAttribute("aria-current")).toBe("page");
  });
});

describe("Home", () => {
  it("puts the needs-you card first, with Go to session", async () => {
    const heading = host.querySelector("h1")!;
    expect(heading.textContent).toBe("Claude Code needs you");
    const button = [...host.querySelectorAll("button")].find(
      (b) => b.textContent === "Go to session",
    )!;
    expect(button).toBeDefined();
    await act(async () => button.click());
    await settle();
    expect(nav("Sessions").getAttribute("aria-current")).toBe("page");
    // ...and that session's detail panel is open, not just the page.
    expect(host.querySelector(".detail-panel")).not.toBeNull();
  });

  it("announces a suggestion only after Shuffle is pressed", async () => {
    const live = () => host.querySelector(".home-suggest-actions [role=status]")?.textContent;
    expect(live()).toBe("");
    const shuffle = [...host.querySelectorAll("button")].find((b) =>
      b.textContent?.includes("Shuffle"),
    )!;
    await act(async () => shuffle.click());
    expect(live()).toContain("Suggesting");
  });

  it("suggests resuming the newest save and lists saved activities", () => {
    expect(visibleText()).toContain("Saved Quiz");
    expect(visibleText()).toContain("Pick up where you left off");
    expect(visibleText()).toContain("Resume");
    // Nothing is working, so the wait line says so rather than guessing a length.
    expect(visibleText()).toContain("If you'd like a break");
  });
});

describe("Play, Rest and Continue", () => {
  it("splits by category and words the help link for the page", async () => {
    await go("Play");
    expect(visibleText()).toContain("Tide Game");
    expect(visibleText()).not.toContain("Slow Breath");
    expect(cards()).toContain("How to play");
    await go("Rest");
    expect(visibleText()).toContain("Slow Breath");
    expect(visibleText()).not.toContain("Tide Game");
    expect(cards()).toContain("How it works");
    expect(cards()).not.toContain("How to play");
  });

  it("filters with a host chip and by search", async () => {
    await go("Rest");
    const chip = [...host.querySelectorAll<HTMLButtonElement>(".chip")].find(
      (b) => b.textContent === "Breathing",
    )!;
    expect(chip).toBeDefined();
    // Only the host's own chip labels appear, never a manifest tag.
    expect([...host.querySelectorAll(".chip")].map((c) => c.textContent)).toEqual([
      "All",
      "Breathing",
    ]);
    const search = host.querySelector<HTMLInputElement>('input[type="search"]')!;
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    await act(async () => {
      set.call(search, "nothing like this");
      search.dispatchEvent(new Event("input", { bubbles: true }));
    });
    expect(visibleText()).toContain("Nothing matches");
  });

  it("lists every saved activity on Continue, newest first, with Resume and Start over", async () => {
    await go("Continue");
    const names = [...host.querySelectorAll(".resume-name")].map((e) => e.textContent);
    expect(names).toEqual(["Saved Quiz", "Saved Stretch"]);
    expect(visibleText()).toContain("Start over");
    expect(visibleText()).toContain("saved just now");
    // No counts, totals or streaks.
    expect(visibleText()).not.toMatch(/streak|total|played/i);
  });
});

describe("Integrations", () => {
  it("opens from Settings, not as a view of its own", async () => {
    await go("Settings");
    expect(host.querySelector("#integrations-heading")?.textContent).toBe("Integrations");
  });
});
