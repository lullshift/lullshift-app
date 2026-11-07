// @vitest-environment jsdom
// The theme an activity gets in its init message is the one chosen in Settings, not just the OS
// preference: "dark" while the OS is light. (Only init carries it; there is no later update.)

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string) => {
    if (command === "activity_launch") return Promise.resolve("lullshift-activity://a/index.html");
    return Promise.resolve(null);
  },
  Channel: class {},
}));

import { ActivityHost, type ActivityInfo } from "./ActivityHost";
import { resetUiPrefsForTests, updateUiPrefs } from "../uiPrefsStore";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const ACTIVITY: ActivityInfo = {
  id: "app.test.theme",
  name: "Theme test",
  description: "",
  category: "game",
  version: "1.0.0",
  storage: false,
  savedUtc: null,
  help: null,
  preview: false,
  playedSeconds: 0,
  engine: null,
};

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  resetUiPrefsForTests();
  // The OS is light and has no reduced-motion preference.
  window.matchMedia = ((query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  })) as unknown as typeof window.matchMedia;
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

async function initPayloadFor(theme: "dark" | "light" | "system") {
  await updateUiPrefs({ theme }, () => Promise.resolve(null));
  await act(async () => {
    root.render(
      <ActivityHost
        activity={ACTIVITY}
        active
        pause={null}
        onClose={() => undefined}
        onGoToSessions={() => undefined}
      />,
    );
    await new Promise((r) => setTimeout(r, 20));
  });
  const frame = host.querySelector("iframe")!;
  const nonce = /#n=([0-9a-f]+)$/.exec(frame.getAttribute("src") ?? "")![1]!;
  const posted: unknown[] = [];
  frame.contentWindow!.postMessage = ((m: unknown) => {
    posted.push(m);
  }) as typeof window.postMessage;
  await act(async () => {
    window.dispatchEvent(
      new MessageEvent("message", {
        data: { lullshift: 1, nonce, type: "ready" },
        source: frame.contentWindow,
        origin: "null",
      }),
    );
    await Promise.resolve();
  });
  return posted.find((m) => (m as { type?: string }).type === "init") as
    { theme: string } | undefined;
}

describe("ActivityHost init", () => {
  it("carries the stored theme, dark while the OS is light", async () => {
    expect((await initPayloadFor("dark"))?.theme).toBe("dark");
  });

  it("follows the OS for System", async () => {
    expect((await initPayloadFor("system"))?.theme).toBe("light");
  });
});
