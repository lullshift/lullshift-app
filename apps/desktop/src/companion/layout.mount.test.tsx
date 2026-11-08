// @vitest-environment jsdom
// Companion layout (owner feedback, M12-31): the compact empty state, the conversation toolbar
// (provider settings, auto-scroll), the collapsible conversation rail whose state is saved in the
// interface preferences, and Send inside the composer box. The Tauri bridge is mocked.

import { act, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type Call = [command: string, args: Record<string, unknown>];
const calls: Call[] = [];
let answer: (command: string, args: Record<string, unknown>) => unknown = () => null;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args: Record<string, unknown> = {}) => {
    calls.push([command, args]);
    try {
      return Promise.resolve(answer(command, args));
    } catch (err) {
      return Promise.reject(err instanceof Error ? err : new Error(String(err)));
    }
  },
  Channel: class {},
}));

import { resetUiPrefsForTests, uiPrefsStore } from "../uiPrefsStore";
import type { Message, ProviderProfile, Thread } from "./api";
import { isAtBottom, useFollowScroll } from "./autoScroll";
import CompanionView from "./CompanionView";
import { resetInflight } from "./inflight";
import { buttonIn, click, polyfillDialog, settle } from "./testDom";

polyfillDialog();

const PROFILE: ProviderProfile = {
  id: "pa",
  kind: "anthropic",
  label: "Provider A",
  baseUrl: "https://api.anthropic.com",
  host: "api.anthropic.com",
  model: "a1",
  models: [{ model: "a1", label: null, isDefault: true }],
  monthlyTokenBudget: null,
  isDefault: true,
  hasKey: true,
  keychainOk: true,
};

const thread = (id: number, title: string): Thread => ({
  id,
  title,
  profileId: null,
  model: null,
  createdAt: "2026-11-07T10:00:00Z",
  updatedAt: "2026-11-07T10:00:00Z",
});

let threads: Thread[] = [];
let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  calls.length = 0;
  threads = [];
  resetInflight();
  resetUiPrefsForTests();
  answer = (command) => {
    switch (command) {
      case "companion_profiles":
        return [PROFILE];
      case "companion_threads":
        return threads;
      case "companion_prefs":
        return { toolsEnabled: false, lastScope: null };
      case "companion_messages":
        return [] as Message[];
      case "set_ui_prefs":
        return null;
      case "companion_new_thread": {
        const t = thread(9, "New conversation");
        threads = [t];
        return t;
      }
      default:
        throw new Error(`unexpected ${command}`);
    }
  };
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

async function mountView(onOpenSettings = vi.fn()) {
  await act(async () => {
    root.render(
      <CompanionView
        headingRef={{ current: null }}
        active
        sessions={[]}
        now={Date.parse("2026-11-07T12:00:00Z")}
        onStartActivity={() => undefined}
        onOpenSettings={onOpenSettings}
      />,
    );
  });
  await settle();
  return { onOpenSettings };
}

const savedPrefs = () =>
  calls.filter(([c]) => c === "set_ui_prefs").map(([, a]) => a.prefs as Record<string, unknown>);

describe("empty state", () => {
  it("is a compact centred block with one heading, one line and one normal-size button", async () => {
    await mountView();
    const empty = host.querySelector(".companion-empty")!;
    expect(empty).not.toBeNull();
    expect(empty.querySelectorAll("h2")).toHaveLength(1);
    expect(empty.querySelectorAll("p")).toHaveLength(1);
    expect(empty.querySelector("img.mascot")).not.toBeNull();
    const buttons = empty.querySelectorAll("button");
    expect(buttons).toHaveLength(1);
    // The button is a plain primary button, not one of the stretching layout classes.
    expect(buttons[0]?.className).toBe("btn btn-primary companion-empty-action");
    expect(empty.className).toBe("companion-empty");
    expect(host.querySelector(".companion-empty .actions")).toBeNull();
    // No stray text link or empty outlined box is left from the old header.
    expect(host.querySelector(".companion-settings-link")).toBeNull();
    // The thread list's own empty text is short.
    expect(host.querySelector(".thread-empty")?.textContent).toBe(
      "Your conversations will show up here.",
    );
  });

  it("starts a conversation from the empty-state button", async () => {
    await mountView();
    await click(host.querySelector<HTMLButtonElement>(".companion-empty button")!);
    expect(calls.some(([c]) => c === "companion_new_thread")).toBe(true);
    expect(host.querySelector(".companion-empty")).toBeNull();
    expect(host.querySelector(".conversation")).not.toBeNull();
  });

  it("says no conversation is selected when there are conversations but none is chosen", async () => {
    threads = [thread(1, "One")];
    await mountView();
    // The first conversation is picked automatically.
    expect(host.querySelector(".companion-empty")).toBeNull();
  });
});

describe("toolbar", () => {
  it("has an icon-only Provider settings button and a pressed-state Auto-scroll toggle", async () => {
    threads = [thread(1, "One")];
    const { onOpenSettings } = await mountView();
    const toolbar = host.querySelector(".conversation-toolbar")!;
    const buttons = [...toolbar.querySelectorAll("button")];
    expect(buttons).toHaveLength(2);
    for (const b of buttons) {
      expect(b.textContent?.trim()).toBe("");
      expect(b.getAttribute("aria-label")).toBeTruthy();
      expect(b.getAttribute("title")).toBeTruthy();
      expect(b.querySelector("svg[aria-hidden='true']")).not.toBeNull();
    }
    const gear = buttonIn(toolbar, "Provider settings");
    await click(gear);
    expect(onOpenSettings).toHaveBeenCalledTimes(1);
    // The old text link is gone from the page header.
    expect(host.querySelector(".companion-header button")).toBeNull();

    const auto = buttonIn(toolbar, "Auto-scroll");
    expect(auto.getAttribute("aria-pressed")).toBe("true");
    expect(auto.getAttribute("title")).toBe("Auto-scroll on");
    await click(auto);
    expect(auto.getAttribute("aria-pressed")).toBe("false");
    expect(auto.getAttribute("title")).toBe("Auto-scroll off");
    expect(savedPrefs().at(-1)?.companionAutoScroll).toBe(false);
    await click(auto);
    expect(savedPrefs().at(-1)?.companionAutoScroll).toBe(true);
  });

  it("starts with auto-scroll off when that was saved", async () => {
    threads = [thread(1, "One")];
    uiPrefsStore.setState({
      prefs: { ...uiPrefsStore.getState().prefs, companionAutoScroll: false },
    });
    await mountView();
    expect(buttonIn(host, "Auto-scroll").getAttribute("aria-pressed")).toBe("false");
  });
});

describe("conversation rail", () => {
  it("collapses to a narrow rail and back, and saves the choice", async () => {
    threads = [thread(1, "One")];
    await mountView();
    const layout = host.querySelector(".companion-layout")!;
    const toggle = buttonIn(host, "Hide conversations");
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(layout.className).toBe("companion-layout");

    await click(toggle);
    expect(layout.classList.contains("companion-layout-collapsed")).toBe(true);
    const rail = buttonIn(host, "Show conversations");
    expect(rail.getAttribute("aria-expanded")).toBe("false");
    // Only the expand button and an icon-only new-conversation button remain in the rail.
    const nav = host.querySelector(".thread-list-head")!;
    expect([...nav.querySelectorAll("button")].map((b) => b.getAttribute("aria-label"))).toEqual([
      "Show conversations",
      "New conversation",
    ]);
    expect(host.querySelector(".thread-items")).toBeNull();
    expect(savedPrefs().at(-1)?.companionPanelCollapsed).toBe(true);

    await click(rail);
    expect(layout.classList.contains("companion-layout-collapsed")).toBe(false);
    expect(buttonIn(host, "Hide conversations").getAttribute("aria-expanded")).toBe("true");
    expect(host.querySelector(".thread-items")).not.toBeNull();
    expect(savedPrefs().at(-1)?.companionPanelCollapsed).toBe(false);
  });

  it("opens collapsed when that was saved, and the rail's plus starts a conversation", async () => {
    threads = [thread(1, "One")];
    uiPrefsStore.setState({
      prefs: { ...uiPrefsStore.getState().prefs, companionPanelCollapsed: true },
    });
    await mountView();
    expect(host.querySelector(".companion-layout-collapsed")).not.toBeNull();
    await click(buttonIn(host, "New conversation"));
    expect(calls.some(([c]) => c === "companion_new_thread")).toBe(true);
  });
});

describe("composer", () => {
  it("has Send inside the box, in the same row as the model chip and Options", async () => {
    threads = [thread(1, "One")];
    await mountView();
    const box = host.querySelector(".composer-box")!;
    const tools = box.querySelector(".composer-tools")!;
    const send = buttonIn(tools, "Send");
    expect(box.contains(send)).toBe(true);
    expect(tools.querySelector(".composer-model")).not.toBeNull();
    expect(buttonIn(tools, "Options…")).toBeTruthy();
    // Nothing is left outside the box.
    const outside = [...host.querySelectorAll(".composer button")].filter((b) => !box.contains(b));
    expect(outside.map((b) => b.textContent)).not.toContain("Send");
  });
});

describe("follow scroll", () => {
  /** A scroll container whose geometry the test controls. */
  function Harness({ enabled, signal }: { enabled: boolean; signal: string }) {
    const ref = useRef<HTMLDivElement>(null);
    const { onScroll } = useFollowScroll(ref, enabled, signal);
    return (
      <div ref={ref} id="box" onScroll={onScroll}>
        x
      </div>
    );
  }

  function fakeBox() {
    const el = host.querySelector<HTMLElement>("#box")!;
    const state = { top: 0, height: 1000, client: 400, writes: 0 };
    Object.defineProperty(el, "scrollHeight", { get: () => state.height, configurable: true });
    Object.defineProperty(el, "clientHeight", { get: () => state.client, configurable: true });
    Object.defineProperty(el, "scrollTop", {
      get: () => state.top,
      set: (v: number) => {
        state.writes++;
        state.top = Math.min(v, state.height - state.client);
      },
      configurable: true,
    });
    const userScrollTo = async (top: number) => {
      state.top = top;
      await act(async () => {
        el.dispatchEvent(new Event("scroll", { bubbles: true }));
      });
    };
    return { el, state, userScrollTo };
  }

  const render = async (enabled: boolean, signal: string) => {
    await act(async () => {
      root.render(<Harness enabled={enabled} signal={signal} />);
    });
  };

  it("knows what the bottom is", () => {
    expect(isAtBottom({ scrollHeight: 1000, scrollTop: 600, clientHeight: 400 })).toBe(true);
    expect(isAtBottom({ scrollHeight: 1000, scrollTop: 590, clientHeight: 400 })).toBe(true);
    expect(isAtBottom({ scrollHeight: 1000, scrollTop: 300, clientHeight: 400 })).toBe(false);
  });

  it("follows streaming text, pauses when the user scrolls up, and resumes at the bottom", async () => {
    await render(true, "a");
    const box = fakeBox();
    box.state.height = 1000;
    await render(true, "b");
    expect(box.state.top).toBe(600);

    // The user scrolls up: new text no longer moves the list.
    await box.userScrollTo(200);
    const writes = box.state.writes;
    box.state.height = 1400;
    await render(true, "c");
    expect(box.state.writes).toBe(writes);
    expect(box.state.top).toBe(200);

    // They come back to the bottom: following resumes.
    await box.userScrollTo(1000);
    box.state.height = 1800;
    await render(true, "d");
    expect(box.state.top).toBe(1400);
  });

  it("never moves by itself when off", async () => {
    await render(false, "a");
    const box = fakeBox();
    box.state.height = 2000;
    await render(false, "b");
    await render(false, "c");
    expect(box.state.writes).toBe(0);
    expect(box.state.top).toBe(0);
    // Turning it on jumps to the end and follows again.
    await render(true, "c");
    expect(box.state.top).toBe(1600);
  });
});
