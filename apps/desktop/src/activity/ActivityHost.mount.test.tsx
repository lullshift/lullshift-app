// @vitest-environment jsdom
// A launch that is paused or closed while it is still loading must not start running behind the
// user's back, and a second resume must not create a second launch.

import { act, createRef, type MutableRefObject } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const calls: string[] = [];
let launchGate: { resolve: (url: string) => void } | null = null;
let snapshotGate: { resolve: () => void } | null = null;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string) => {
    calls.push(command);
    switch (command) {
      case "activity_snapshot":
        return snapshotGate
          ? new Promise((resolve) => {
              snapshotGate!.resolve = () => resolve(null);
            })
          : Promise.resolve(null);
      case "activity_launch":
        return new Promise<string>((resolve) => {
          launchGate = { resolve };
        });
      case "activity_launch_end":
        return Promise.resolve(null);
      default:
        return Promise.reject(new Error("unavailable"));
    }
  },
}));

import { ActivityHost, type ActivityInfo, type HostControl } from "./ActivityHost";

const ACTIVITY: ActivityInfo = {
  id: "app.test.game",
  name: "Test Game",
  description: "A test",
  category: "arcade",
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
let control: MutableRefObject<HostControl | null>;

const render = (active: boolean) =>
  act(async () => {
    root.render(
      <ActivityHost
        activity={ACTIVITY}
        active={active}
        pause={null}
        controlRef={control}
        onClose={() => undefined}
        onGoToSessions={() => undefined}
      />,
    );
  });
const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 10));
  });
const count = (command: string) => calls.filter((c) => c === command).length;

beforeEach(() => {
  calls.length = 0;
  launchGate = null;
  snapshotGate = null;
  control = createRef<HostControl | null>() as MutableRefObject<HostControl | null>;
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

describe("a pause while the launch is still loading", () => {
  it("stays paused when the key arrives after the host was hidden, and ends the key", async () => {
    await render(true);
    await settle();
    expect(count("activity_launch")).toBe(1);
    // The user leaves (or the window hides) before the launch key is issued.
    await render(false);
    await act(async () => launchGate!.resolve("lullshift-activity://k/index.html"));
    await settle();
    expect(host.querySelector("iframe")).toBeNull();
    expect(host.textContent).toContain("is paused");
    expect(count("activity_launch_end")).toBeGreaterThanOrEqual(1);
  });

  it("never asks for a launch once it has been unmounted", async () => {
    snapshotGate = { resolve: () => undefined };
    await render(true);
    await settle();
    expect(count("activity_launch")).toBe(0);
    act(() => root.unmount());
    snapshotGate.resolve();
    await settle();
    expect(count("activity_launch")).toBe(0);
    // Re-create the root so afterEach can unmount.
    root = createRoot(host);
  });
});

describe("resume", () => {
  it("called twice starts one launch", async () => {
    await render(true);
    await settle();
    await render(false);
    await act(async () => launchGate!.resolve("lullshift-activity://k/index.html"));
    await settle();
    const before = count("activity_launch");
    await render(true);
    await act(async () => {
      control.current!.resume();
      control.current!.resume();
    });
    await settle();
    expect(count("activity_launch") - before).toBe(1);
  });
});
