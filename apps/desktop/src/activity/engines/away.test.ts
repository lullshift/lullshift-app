// Rule, schema and fixture tests for the Away Prompts engine (lullshift.away@1).
/// <reference types="node" />
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { hostDataCheck } from "./hostDataCheck";
import { describe, expect, it } from "vitest";

const root = fileURLToPath(new URL("../../../../../", import.meta.url));
new Function(readFileSync(join(root, "activities/engines/away/logic.js"), "utf8"))();

interface State {
  order: number[];
  pos: number;
  seen: boolean[];
  elapsed: number;
  back: boolean;
}
interface Logic {
  SAMPLE: unknown;
  validData(c: unknown): boolean;
  makeOrder(n: number, shuffle: boolean, rand: () => number): number[];
  newState(n: number, shuffle: boolean, rand: () => number): State;
  current(s: State): number;
  move(s: State, dir: number): State;
  seenCount(s: State): number;
  addTime(s: State, dt: number, running: boolean): State;
  setBack(s: State, back: boolean): State;
  snapshot(s: State): unknown;
  restore(raw: unknown, n: number, shuffle: boolean, rand: () => number): State;
  clock(sec: number): string;
  awayText(sec: number): string;
}
const L = (globalThis as unknown as { AwayLogic: Logic }).AwayLogic;

// A small deterministic random source.
function seeded(seed: number): () => number {
  let a = seed;
  return () => {
    a = (a * 1664525 + 1013904223) % 4294967296;
    return a / 4294967296;
  };
}

describe("AwayLogic", () => {
  it("keeps the given order unless shuffling, and shuffles into a permutation", () => {
    expect(L.makeOrder(5, false, Math.random)).toEqual([0, 1, 2, 3, 4]);
    for (let seed = 1; seed < 30; seed++) {
      const o = L.makeOrder(12, true, seeded(seed));
      expect([...o].sort((a, b) => a - b)).toEqual([...Array(12).keys()]);
    }
    expect(L.makeOrder(12, true, seeded(7))).toEqual(L.makeOrder(12, true, seeded(7)));
    expect(L.makeOrder(12, true, seeded(7))).not.toEqual(L.makeOrder(12, false, Math.random));
    expect(L.makeOrder(1, true, () => 0.999)).toEqual([0]);
  });

  it("counts ideas seen once each and wraps round in both directions", () => {
    let s = L.newState(3, false, Math.random);
    expect(L.seenCount(s)).toBe(1);
    s = L.move(s, 1);
    s = L.move(s, 1);
    expect(L.seenCount(s)).toBe(3);
    s = L.move(s, 1);
    expect(L.current(s)).toBe(0);
    expect(L.seenCount(s)).toBe(3);
    expect(L.current(L.move(s, -1))).toBe(2);
  });

  it("counts time only while away and while the host is running", () => {
    let s = L.newState(3, false, Math.random);
    s = L.addTime(s, 1.5, true);
    expect(s.elapsed).toBeCloseTo(1.5);
    expect(L.addTime(s, 1, false).elapsed).toBeCloseTo(1.5);
    s = L.setBack(s, true);
    expect(L.addTime(s, 1, true).elapsed).toBeCloseTo(1.5);
    s = L.setBack(s, false);
    expect(L.addTime(s, 1, true).elapsed).toBeCloseTo(2.5);
  });

  it("clamps a long gap and ignores negative time", () => {
    const s = L.newState(2, false, Math.random);
    expect(L.addTime(s, 3600, true).elapsed).toBe(2);
    expect(L.addTime(s, -5, true).elapsed).toBe(0);
  });

  it("saves and restores the order, place, seen set and time", () => {
    let s = L.newState(6, true, seeded(3));
    s = L.move(L.move(s, 1), 1);
    s = L.addTime(s, 2, true);
    s = L.setBack(s, true);
    const back = L.restore(JSON.parse(JSON.stringify(L.snapshot(s))), 6, true, seeded(99));
    expect(back.order).toEqual(s.order);
    expect(back.pos).toBe(s.pos);
    expect(back.seen).toEqual(s.seen);
    expect(back.elapsed).toBe(2);
    expect(back.back).toBe(true);
  });

  it("falls back to a fresh start for bad saved state", () => {
    const base = L.snapshot(L.newState(3, false, Math.random)) as Record<string, unknown>;
    for (const bad of [
      null,
      "x",
      {},
      { ...base, v: 2 },
      { ...base, order: [0, 0, 1] },
      { ...base, order: [0, 1] },
      { ...base, order: [0, 1, 5] },
      { ...base, pos: 3 },
      { ...base, pos: 1.5 },
      { ...base, seen: [true] },
    ]) {
      const s = L.restore(bad, 3, false, Math.random);
      expect(s).toMatchObject({ order: [0, 1, 2], pos: 0, elapsed: 0, back: false });
    }
    expect(L.restore({ ...base, elapsed: -4 }, 3, false, Math.random).elapsed).toBe(0);
    expect(L.restore({ ...base, elapsed: "9" }, 3, false, Math.random).elapsed).toBe(0);
  });

  it("saves whole seconds so saves stay stable between ticks", () => {
    const s = L.addTime(L.newState(2, false, Math.random), 1.9, true);
    expect((L.snapshot(s) as { elapsed: number }).elapsed).toBe(1);
  });

  it("formats the clock and the paused-view text", () => {
    expect(L.clock(0)).toBe("00:00");
    expect(L.clock(75.9)).toBe("01:15");
    expect(L.clock(3725)).toBe("1:02:05");
    expect(L.awayText(59)).toBe("Under a minute");
    expect(L.awayText(185)).toBe("3 min");
  });

  it("accepts the built-in sample and rejects broken data", () => {
    expect(L.validData(L.SAMPLE)).toBe(true);
    expect(L.validData(null)).toBe(false);
    expect(L.validData({ title: "x", prompts: [] })).toBe(false);
    expect(L.validData({ title: "x", prompts: [{ text: "" }] })).toBe(false);
    expect(L.validData({ title: "x", prompts: [{ text: "a", category: "party" }] })).toBe(false);
    expect(L.validData({ title: "x", prompts: [{ text: "a" }], shuffle: "yes" })).toBe(false);
  });
});

describe("lullshift.away@1 data schema and fixtures", () => {
  const schema = JSON.parse(
    readFileSync(join(root, "activities/engines/away/data.schema.json"), "utf8"),
  );
  const validate = hostDataCheck(schema as object);
  const dir = join(root, "tests/fixtures/engines/lullshift.away@1");
  const load = (kind: string) =>
    readdirSync(join(dir, kind)).map((f) => ({
      f,
      json: JSON.parse(readFileSync(join(dir, kind, f), "utf8")) as unknown,
    }));

  it("has the minimum number of fixtures", () => {
    expect(load("valid").length).toBeGreaterThanOrEqual(3);
    expect(load("invalid").length).toBeGreaterThanOrEqual(4);
  });
  for (const kind of ["valid", "invalid"]) {
    for (const { f, json } of load(kind)) {
      it(`${kind}/${f}`, () => {
        expect(validate(json), JSON.stringify(validate.errors)).toBe(kind === "valid");
      });
    }
  }

  it("the cheap in-page check agrees on valid fixtures", () => {
    for (const { json } of load("valid")) expect(L.validData(json)).toBe(true);
  });

  it("every bundled prompt pack validates", () => {
    const base = join(root, "activities/content/away");
    const packs = readdirSync(base);
    expect(packs.length).toBeGreaterThanOrEqual(5);
    for (const p of packs) {
      const manifest = JSON.parse(readFileSync(join(base, p, "manifest.json"), "utf8"));
      expect(manifest.engine).toBe("lullshift.away@1");
      const d = JSON.parse(readFileSync(join(base, p, manifest.data), "utf8"));
      expect(validate(d), `${p}: ${JSON.stringify(validate.errors)}`).toBe(true);
      expect(L.validData(d)).toBe(true);
    }
  });
});
