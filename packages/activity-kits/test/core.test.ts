import { describe, expect, it } from "vitest";
import {
  HISTORY_RESTORE_BUDGET,
  History,
  cloneJson,
  finiteIn,
  createRng,
  dailySeed,
  hashString,
  isJson,
  keyAction,
  localDateKey,
  toUint32,
} from "../src/core/index.ts";

// mulberry32 as published by its author, for comparison.
function reference(seed: number) {
  let a = seed;
  return () => {
    let t = (a += 0x6d2b79f5);
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

describe("seeded random numbers", () => {
  it("matches mulberry32 and known vectors", () => {
    const r = createRng(1);
    const first = [r.next(), r.next(), r.next()].map((x) => x * 4294967296);
    expect(first).toEqual([2693262067, 11749833, 2265367787]);
    for (const seed of [0, 1, 42, 123456789, 4294967295]) {
      const ours = createRng(seed);
      const ref = reference(seed);
      for (let i = 0; i < 50; i++) expect(ours.next()).toBe(ref());
    }
  });

  it("is deterministic and resumable from its state", () => {
    const a = createRng(42);
    expect(a.shuffle([1, 2, 3, 4, 5, 6, 7, 8, 9, 10])).toEqual([1, 8, 4, 6, 3, 2, 9, 10, 5, 7]);
    const saved = a.state();
    const rest = [a.next(), a.int(1, 6), a.pick(["x", "y", "z"])];
    const b = createRng(saved);
    expect([b.next(), b.int(1, 6), b.pick(["x", "y", "z"])]).toEqual(rest);
  });

  it("keeps int in range and leaves the input of shuffle alone", () => {
    const r = createRng(7);
    const seen = new Set<number>();
    for (let i = 0; i < 2000; i++) {
      const n = r.int(1, 6);
      expect(n).toBeGreaterThanOrEqual(1);
      expect(n).toBeLessThanOrEqual(6);
      seen.add(n);
    }
    expect(seen.size).toBe(6);
    const input = [1, 2, 3];
    r.shuffle(input);
    expect(input).toEqual([1, 2, 3]);
    expect(() => r.pick([])).toThrow(RangeError);
  });

  it("reduces any seed to 32 bits", () => {
    expect(toUint32(-1)).toBe(4294967295);
    expect(toUint32(Number.NaN)).toBe(0);
    expect(toUint32(Infinity)).toBe(0);
    expect(createRng(2 ** 32 + 5).state()).toBe(5);
  });
});

describe("daily seed", () => {
  it("hashes with FNV-1a", () => {
    expect(hashString("")).toBe(0x811c9dc5);
    expect(hashString("a")).toBe(0xe40c292c);
    expect(hashString("foobar")).toBe(0xbf9cf968);
  });

  it("uses the local calendar date", () => {
    // Late evening local time: the local date, not UTC, decides.
    const late = new Date(2026, 10, 5, 23, 59, 30);
    expect(localDateKey(late)).toBe("2026-11-05");
    expect(dailySeed(late)).toBe(dailySeed("2026-11-05"));
    expect(dailySeed(new Date(2026, 10, 5, 0, 0, 1))).toBe(dailySeed("2026-11-05"));
    expect(dailySeed("2026-11-05")).not.toBe(dailySeed("2026-11-06"));
    expect(dailySeed("2026-11-05")).toBe(hashString("2026-11-05"));
  });

  it("salts per game and rejects bad input", () => {
    expect(dailySeed("2026-11-05", "lantern")).not.toBe(dailySeed("2026-11-05", "tiles"));
    expect(() => dailySeed("5 Nov")).toThrow(RangeError);
    expect(() => dailySeed(new Date("nope"))).toThrow(RangeError);
  });
});

describe("history", () => {
  it("undoes and redoes snapshots", () => {
    const h = new History<number>();
    let state = 0;
    for (const next of [1, 2, 3]) {
      h.record(state);
      state = next;
    }
    expect(h.size()).toBe(3);
    state = h.undo(state)!;
    expect(state).toBe(2);
    state = h.undo(state)!;
    expect(state).toBe(1);
    state = h.redo(state)!;
    expect(state).toBe(2);
    expect(h.canRedo()).toBe(true);
    h.record(state);
    state = 9;
    expect(h.canRedo()).toBe(false);
    expect(h.redo(state)).toBeUndefined();
  });

  it("drops the oldest entries past its cap", () => {
    const h = new History<number>(3);
    for (let i = 0; i < 10; i++) h.record(i);
    expect(h.snapshot().undo).toEqual([7, 8, 9]);
    expect(h.undo(10)).toBe(9);
    expect(h.undo(9)).toBe(8);
    expect(h.undo(8)).toBe(7);
    expect(h.undo(7)).toBeUndefined();
  });

  it("copies entries so later edits don't leak in", () => {
    const h = new History<{ n: number[] }>();
    const s = { n: [1] };
    h.record(s);
    s.n.push(2);
    expect(h.undo(s)).toEqual({ n: [1] });
  });

  it("restores a snapshot and rejects malformed ones", () => {
    const h = new History<number>(5);
    h.record(1);
    const isNum = (e: unknown) => typeof e === "number";
    expect(h.restore({ undo: [1, 2], redo: [3] }, isNum)).toBe(true);
    expect(h.snapshot()).toEqual({ undo: [1, 2], redo: [3] });
    for (const bad of [
      null,
      "x",
      { undo: [1] },
      { undo: ["a"], redo: [] },
      { undo: [], redo: {} },
    ]) {
      expect(h.restore(bad, isNum)).toBe(false);
    }
    expect(h.snapshot()).toEqual({ undo: [1, 2], redo: [3] });
  });

  it("trims an oversized saved history before checking any entry", () => {
    const h = new History<number>(3);
    let checked = 0;
    const isNum = (e: unknown) => (checked++, typeof e === "number");
    // Garbage older than the limit is never looked at.
    const undo = [...Array.from({ length: 1_000_000 }, () => "junk"), 1, 2, 3, 4];
    expect(h.restore({ undo, redo: [9] }, isNum)).toBe(true);
    expect(h.snapshot()).toEqual({ undo: [2, 3, 4], redo: [9] });
    expect(checked).toBe(4);
  });

  it("keeps only the newest entries that fit the restore budget", () => {
    const h = new History<number[]>(10_000);
    const big = Array.from({ length: HISTORY_RESTORE_BUDGET / 4 }, () => 0);
    const undo = Array.from({ length: 10_000 }, () => big);
    let checked = 0;
    const ok = (e: unknown) => (checked++, Array.isArray(e));
    const weigh = (e: unknown) => (Array.isArray(e) ? e.length : 0);
    expect(h.restore({ undo, redo: [big] }, ok, weigh)).toBe(true);
    expect(h.size()).toBe(4);
    expect(h.canRedo()).toBe(false); // undo used the whole budget
    expect(checked).toBe(4);
  });
});

describe("finiteIn", () => {
  it("clamps finite numbers and falls back for anything else", () => {
    expect(finiteIn(5, 0, 10, 3)).toBe(5);
    expect(finiteIn(-1, 0, 10, 3)).toBe(0);
    expect(finiteIn(99, 0, 10, 3)).toBe(10);
    for (const bad of [NaN, Infinity, -Infinity, "5", null, undefined]) {
      expect(finiteIn(bad, 0, 10, 3)).toBe(3);
    }
  });
});

describe("keys", () => {
  it("maps named keys, undo and redo", () => {
    expect(keyAction({ key: "ArrowUp" })).toBe("up");
    expect(keyAction({ key: "PageDown" })).toBe("pageDown");
    expect(keyAction({ key: " " })).toBe("activate");
    expect(keyAction({ key: "Enter" })).toBe("activate");
    expect(keyAction({ key: "Escape" })).toBe("cancel");
    expect(keyAction({ key: "z", ctrlKey: true })).toBe("undo");
    expect(keyAction({ key: "Z", metaKey: true, shiftKey: true })).toBe("redo");
    expect(keyAction({ key: "y", ctrlKey: true })).toBe("redo");
    expect(keyAction({ key: "Home", ctrlKey: true })).toBe("first");
    expect(keyAction({ key: "End", ctrlKey: true })).toBe("last");
  });

  it("leaves other keys and shortcuts alone", () => {
    expect(keyAction({ key: "c", ctrlKey: true })).toBeNull();
    expect(keyAction({ key: "ArrowLeft", altKey: true })).toBeNull();
    expect(keyAction({ key: "z" })).toBeNull();
    expect(keyAction({ key: "constructor" })).toBeNull();
    expect(keyAction({ key: "toString" })).toBeNull();
  });

  it("uses physical WASD positions only when asked", () => {
    // AZERTY: the key in the W position types "z".
    const azertyUp = { key: "z", code: "KeyW" };
    expect(keyAction(azertyUp)).toBeNull();
    expect(keyAction(azertyUp, { wasd: true })).toBe("up");
    expect(keyAction({ key: "d", code: "KeyD" }, { wasd: true })).toBe("right");
    expect(keyAction({ key: "a", code: "KeyQ" }, { wasd: true })).toBeNull();
  });
});

describe("json helpers", () => {
  it("accepts JSON and rejects everything else", () => {
    expect(isJson({ a: [1, "x", null, true, { b: 2 }] })).toBe(true);
    expect(isJson(Number.NaN)).toBe(false);
    expect(isJson(undefined)).toBe(false);
    expect(isJson(() => 1)).toBe(false);
    expect(isJson(new Date())).toBe(false);
    expect(isJson(JSON.parse('{"__proto__": {"x": 1}}'))).toBe(false);
    let deep: unknown = 1;
    for (let i = 0; i < 50; i++) deep = [deep];
    expect(isJson(deep)).toBe(false);
    expect(cloneJson({ a: [1] })).toEqual({ a: [1] });
  });
});
