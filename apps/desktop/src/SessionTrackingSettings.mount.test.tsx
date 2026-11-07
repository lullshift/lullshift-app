// @vitest-environment jsdom
// Mounted with the Tauri bridge mocked: the catch-up setting is off until switched on, only
// that one value is saved, and an unreadable setting or a failed save is said out loud
// (decision 35).

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

type Call = [command: string, args: unknown];
const calls: Call[] = [];
let answer: (command: string, args: Record<string, unknown>) => unknown = () => null;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args: Record<string, unknown> = {}) => {
    calls.push([command, args]);
    try {
      return Promise.resolve(answer(command, args));
    } catch (err) {
      return Promise.reject(err instanceof Error ? err : (err as string));
    }
  },
}));

import { SessionTrackingSettings } from "./SessionTrackingSettings";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | null = null;
let host: HTMLElement | null = null;

async function mount() {
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
  await act(async () => {
    root?.render(<SessionTrackingSettings />);
  });
  await act(async () => {});
}

afterEach(() => {
  act(() => root?.unmount());
  host?.remove();
  root = null;
  host = null;
  calls.length = 0;
});

const status = {
  outcome: "not_run",
  finishedUtc: null,
  updated: 0,
  unreadable: [],
  incomplete: false,
};

const toggle = () => host?.querySelector<HTMLButtonElement>('button[role="switch"]');

describe("SessionTrackingSettings", () => {
  it("shows catch-up off by default and reads nothing from the logs by itself", async () => {
    answer = (command) =>
      command === "session_tracking_prefs" ? { catchUpFromLogs: false } : status;
    await mount();
    expect(toggle()?.getAttribute("aria-checked")).toBe("false");
    expect(host?.textContent).toContain("Catch up from agent logs at startup");
    expect(host?.textContent).toContain("Ended (process gone)");
    expect(calls.map((c) => c[0]).sort()).toEqual(["catch_up_status", "session_tracking_prefs"]);
  });

  it("saves only the one value when switched on, then says it runs at the next start", async () => {
    answer = (command, args) => {
      if (command === "session_tracking_prefs") return { catchUpFromLogs: false };
      if (command === "set_session_tracking_prefs") return args.prefs;
      return status;
    };
    await mount();
    await act(async () => {
      toggle()?.click();
    });
    const save = calls.find((c) => c[0] === "set_session_tracking_prefs");
    expect(save?.[1]).toEqual({ prefs: { catchUpFromLogs: true } });
    expect(toggle()?.getAttribute("aria-checked")).toBe("true");
    expect(host?.textContent).toContain("next time Lullshift starts");
  });

  it("shows what the last catch-up did, including failures", async () => {
    answer = (command) =>
      command === "session_tracking_prefs"
        ? { catchUpFromLogs: true }
        : {
            outcome: "done",
            finishedUtc: "2026-11-07T10:00:00Z",
            updated: 2,
            unreadable: ["codex"],
            incomplete: true,
          };
    await mount();
    const text = host?.textContent ?? "";
    expect(text).toContain("updated 2 sessions");
    expect(text).toContain("Couldn't read the logs of Codex.");
    expect(text).toContain("Older sessions may not have been checked.");
  });

  it("says so when the setting can't be read or saved", async () => {
    answer = () => {
      throw "store_unavailable";
    };
    await mount();
    expect(host?.textContent).toContain("Session settings couldn't be read");
    expect(toggle()).toBeNull();
    act(() => root?.unmount());
    host?.remove();

    answer = (command) => {
      if (command === "set_session_tracking_prefs") throw "store_unavailable";
      return command === "session_tracking_prefs" ? { catchUpFromLogs: false } : status;
    };
    await mount();
    await act(async () => {
      toggle()?.click();
    });
    expect(host?.textContent).toContain("The setting wasn't saved");
    expect(toggle()?.getAttribute("aria-checked")).toBe("false");
  });
});
