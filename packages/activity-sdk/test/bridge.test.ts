// Runs the real bridge client against a fake parent window.
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("../bridge/lullshift-bridge-v1.js", import.meta.url), "utf8");
const NONCE = "c".repeat(32);

type Msg = Record<string, unknown>;

function load() {
  const sent: Msg[] = [];
  const listeners: ((e: { source: unknown; data: unknown }) => void)[] = [];
  const parent = { postMessage: (m: Msg) => sent.push(m) };
  const window: Record<string, unknown> = {
    parent,
    addEventListener: (_: string, fn: (e: { source: unknown; data: unknown }) => void) =>
      listeners.push(fn),
  };
  runInNewContext(source, { window, location: { hash: `#n=${NONCE}` }, Promise, Object });
  const bridge = window.LullshiftBridge as {
    start(h: object): void;
    autosave(s: unknown): void;
    summary(items: unknown): void;
  };
  const host = (m: Msg, from: unknown = parent) =>
    listeners.forEach((l) => l({ source: from, data: { lullshift: 1, nonce: NONCE, ...m } }));
  return { bridge, sent, host };
}

describe("bridge client", () => {
  it("says ready, then passes init through with content defaulting to null", () => {
    const { bridge, sent, host } = load();
    let info: unknown;
    bridge.start({ onInit: (i: unknown) => (info = i) });
    expect(sent).toEqual([{ lullshift: 1, nonce: NONCE, type: "ready" }]);
    host({ type: "init", restore: { moves: 2 }, reducedMotion: 1, theme: "dark" });
    expect(info).toEqual({
      restore: { moves: 2 },
      reducedMotion: true,
      theme: "dark",
      content: null,
    });
  });

  it("always answers a save: the current state, else the last known one", () => {
    const { bridge, sent, host } = load();
    let state: unknown;
    bridge.start({ saveState: () => state });
    host({ type: "init", restore: { level: 1 }, reducedMotion: false, theme: "light" });
    host({ type: "save", requestId: "s1" });
    expect(sent.at(-1)).toMatchObject({ type: "state", requestId: "s1", state: { level: 1 } });
    state = { level: 2 };
    host({ type: "save", requestId: "s2" });
    expect(sent.at(-1)).toMatchObject({ requestId: "s2", state: { level: 2 } });
    state = undefined;
    host({ type: "save", requestId: "s3" });
    expect(sent.at(-1)).toMatchObject({ requestId: "s3", state: { level: 2 } });
  });

  it("answers with the last autosave when saveState throws", () => {
    const { bridge, sent, host } = load();
    bridge.start({
      saveState: () => {
        throw new Error("busy");
      },
    });
    host({ type: "init", restore: null, reducedMotion: false, theme: "light" });
    bridge.autosave({ step: 4 });
    host({ type: "save", requestId: "s1" });
    expect(sent.at(-1)).toMatchObject({ requestId: "s1", state: { step: 4 } });
  });

  it("never sends a null state, and stays silent with nothing known", () => {
    const { bridge, sent, host } = load();
    bridge.start({ saveState: () => null });
    host({ type: "init", restore: null, reducedMotion: false, theme: "light" });
    bridge.autosave(null);
    host({ type: "save", requestId: "s1" });
    expect(sent.filter((m) => m.type === "state")).toEqual([]);
  });

  it("ignores messages from anything but the parent, or with another nonce", () => {
    const { bridge, host } = load();
    let calls = 0;
    bridge.start({ onPause: () => (calls += 1) });
    host({ type: "pause" }, {});
    host({ type: "pause", nonce: "d".repeat(32) });
    expect(calls).toBe(0);
    host({ type: "pause" });
    expect(calls).toBe(1);
  });

  it("sends summaries as given; the host validates them", () => {
    const { bridge, sent } = load();
    bridge.summary([{ label: "Moves", value: 3 }]);
    expect(sent.at(-1)).toEqual({
      lullshift: 1,
      nonce: NONCE,
      type: "summary",
      items: [{ label: "Moves", value: 3 }],
    });
  });
});
