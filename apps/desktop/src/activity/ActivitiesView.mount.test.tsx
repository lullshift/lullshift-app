// @vitest-environment jsdom
// Each launch gets its own host (fresh stats and bookkeeping); a different activity waits for the
// current one to pause and save; the same paused activity resumes in one step; a sidebar click
// pauses a running activity and shows the list with a quiet "paused · Resume" bar.

import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ActivityInfo, HostControl } from "./ActivityHost";

const log: string[] = [];
let mounts = 0;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: () => Promise.reject(new Error("unavailable")),
}));

// A stand-in host: it records what the page above asks of it. Its "played" counter is local
// state, like the real host's, so a reused host would carry it over.
vi.mock("./ActivityHost", async () => {
  const React = await import("react");
  return {
    ActivityHost: ({
      activity,
      active,
      controlRef,
    }: {
      activity: ActivityInfo;
      active: boolean;
      controlRef: { current: HostControl | null };
    }) => {
      const [played] = React.useState(() => {
        mounts += 1;
        return `mount-${mounts}`;
      });
      useEffect(() => {
        controlRef.current = {
          pauseNow: async () => {
            log.push(`pause ${activity.id}`);
            await new Promise((r) => setTimeout(r, 5));
            log.push(`paused ${activity.id}`);
          },
          resume: () => log.push(`resume ${activity.id}`),
        };
        return () => {
          log.push(`unmount ${activity.id}`);
          controlRef.current = null;
        };
      }, [activity.id, controlRef]);
      return (
        <div data-testid="host" data-active={String(active)}>
          {activity.name}:{played}
        </div>
      );
    },
  };
});

import ActivitiesView from "./ActivitiesView";
import type { ActivityLoad } from "./useActivityList";

const activity = (id: string, name: string, category = "arcade"): ActivityInfo => ({
  id,
  name,
  description: `About ${name}`,
  category,
  version: "1.0.0",
  storage: true,
  savedUtc: null,
  help: null,
  preview: false,
  playedSeconds: 0,
  engine: null,
});
const A = activity("a", "Alpha");
const B = activity("b", "Bravo");
const LOAD: ActivityLoad = { status: "ready", list: [A, B] };

let host: HTMLDivElement;
let root: Root;

type Launch = { id: string; nonce: number; activity?: ActivityInfo } | null;
const render = (launch: Launch, navNonce: number) =>
  act(async () => {
    root.render(
      <ActivitiesView
        headingRef={{ current: null }}
        active
        page="play"
        load={LOAD}
        refresh={() => undefined}
        pause={null}
        now={Date.now()}
        launch={launch}
        navNonce={navNonce}
        onGoToSessions={() => undefined}
        onShowPage={() => undefined}
      />,
    );
  });
const text = () => host.textContent ?? "";
const hostEl = () => host.querySelector('[data-testid="host"]');

beforeEach(() => {
  log.length = 0;
  mounts = 0;
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

describe("launches", () => {
  it("gives a different activity a fresh host, after the first one has paused", async () => {
    await render({ id: "a", nonce: 1, activity: A }, 0);
    expect(hostEl()?.textContent).toBe("Alpha:mount-1");
    await render({ id: "b", nonce: 2, activity: B }, 0);
    await act(async () => {
      await new Promise((r) => setTimeout(r, 30));
    });
    expect(log).toEqual(["pause a", "paused a", "unmount a"]);
    // A new host, not Alpha's carried over: its own state starts from scratch.
    expect(hostEl()?.textContent).toBe("Bravo:mount-2");
  });

  it("makes one host when the same different activity is chosen twice while the first pauses", async () => {
    await render({ id: "a", nonce: 1, activity: A }, 0);
    await render({ id: "b", nonce: 2, activity: B }, 0);
    await render({ id: "b", nonce: 3, activity: B }, 0);
    await act(async () => {
      await new Promise((r) => setTimeout(r, 40));
    });
    expect(mounts).toBe(2);
    expect(log.filter((l) => l === "unmount a")).toHaveLength(1);
    expect(hostEl()?.textContent).toBe("Bravo:mount-2");
  });

  it("resumes the activity that is already paused in one step, without a new host", async () => {
    await render({ id: "a", nonce: 1, activity: A }, 0);
    await render({ id: "a", nonce: 2, activity: A }, 0);
    expect(log).toEqual(["resume a"]);
    expect(mounts).toBe(1);
  });
});

describe("the sidebar while something runs", () => {
  it("shows the list with a paused bar, and Resume brings the host back", async () => {
    await render({ id: "a", nonce: 1, activity: A }, 0);
    expect(host.querySelector(".paused-bar")).toBeNull();
    await render({ id: "a", nonce: 1, activity: A }, 1);
    // The host is hidden and told it is not active, so it pauses and saves the normal way.
    expect(hostEl()?.getAttribute("data-active")).toBe("false");
    expect(host.querySelector("section[hidden]")).not.toBeNull();
    const bar = host.querySelector(".paused-bar");
    expect(bar?.textContent).toContain("Alpha is paused");
    expect(text()).toContain("Play");
    const resume = bar!.querySelector("button")!;
    await act(async () => resume.click());
    expect(host.querySelector(".paused-bar")).toBeNull();
    expect(hostEl()?.getAttribute("data-active")).toBe("true");
    expect(log).toContain("resume a");
    expect(mounts).toBe(1);
  });
});
