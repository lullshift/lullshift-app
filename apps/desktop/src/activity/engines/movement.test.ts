// Rule, schema and fixture tests for the Movement Routine engine (lullshift.movement@1).
/// <reference types="node" />
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { hostDataCheck } from "./hostDataCheck";
import { describe, expect, it } from "vitest";

const root = fileURLToPath(new URL("../../../../../", import.meta.url));
new Function(readFileSync(join(root, "activities/engines/movement/logic.js"), "utf8"))();

interface Move {
  name: string;
  steps: string[];
  seconds?: number;
  reps?: number;
  side?: string;
  position?: string;
}
interface Data {
  title: string;
  moves: Move[];
  restSeconds?: number;
  wayOut: string;
}
interface Stage {
  type: "move" | "rest";
  move: number;
  part: number;
  parts: number;
  seconds: number | null;
  reps: number | null;
}
interface State {
  stage: number;
  remaining: number | null;
  running: boolean;
  status: number[];
}
interface Logic {
  SAMPLE: Data;
  validData(c: unknown): boolean;
  buildStages(d: Data): Stage[];
  newState(s: Stage[], n: number): State;
  finished(s: State, st: Stage[]): boolean;
  next(s: State, st: Stage[]): State;
  skip(s: State, st: Stage[]): State;
  back(s: State, st: Stage[]): State;
  timeUp(s: State): boolean;
  toggle(s: State): State;
  tick(s: State, dt: number): State;
  snapshot(s: State): unknown;
  restore(raw: unknown, st: Stage[], n: number): State;
  tally(s: State): { done: number; skipped: number; total: number };
  clock(sec: number): string;
  amount(m: Move, st: Stage): string;
  sideLabel(m: Move, st: Stage): string;
}
const L = (globalThis as unknown as { MovementLogic: Logic }).MovementLogic;

const at = <T>(a: T[], i: number): T => a[i] as T;

const data: Data = {
  title: "Test",
  wayOut: "Stop any time.",
  restSeconds: 5,
  moves: [
    { name: "A", steps: ["a"], seconds: 20, side: "left-right" },
    { name: "B", steps: ["b"], reps: 3 },
    { name: "C", steps: ["c"], seconds: 10 },
  ],
};

describe("MovementLogic", () => {
  it("builds stages: two parts for left-right and a rest between moves", () => {
    const st = L.buildStages(data);
    expect(st.map((s) => `${s.type}${s.move}.${s.part}`)).toEqual([
      "move0.0",
      "move0.1",
      "rest1.0",
      "move1.0",
      "rest2.0",
      "move2.0",
    ]);
    expect(L.buildStages({ ...data, restSeconds: 0 }).some((s) => s.type === "rest")).toBe(false);
  });

  it("starts a timed move stopped and a rest running", () => {
    const st = L.buildStages(data);
    let s = L.newState(st, 3);
    expect(s).toMatchObject({ stage: 0, remaining: 20, running: false });
    s = L.next(L.next(s, st), st);
    expect(at(st, s.stage).type).toBe("rest");
    expect(s).toMatchObject({ remaining: 5, running: true });
  });

  it("counts down, stops at zero and never goes negative or jumps", () => {
    const st = L.buildStages(data);
    let s = L.toggle(L.newState(st, 3));
    expect(s.running).toBe(true);
    s = L.tick(s, 1.5);
    expect(s.remaining).toBeCloseTo(18.5);
    s = L.tick(s, 600); // a long sleep is clamped
    expect(s.remaining).toBeCloseTo(16.5);
    for (let i = 0; i < 10; i++) s = L.tick(s, 2);
    expect(s.remaining).toBe(0);
    expect(s.running).toBe(false);
    expect(L.timeUp(s)).toBe(true);
    expect(L.toggle(s)).toBe(s);
  });

  it("does not tick when stopped", () => {
    const st = L.buildStages(data);
    const s = L.newState(st, 3);
    expect(L.tick(s, 1)).toBe(s);
  });

  it("marks done on the last part only, skips whole moves and passes over the rest", () => {
    const st = L.buildStages(data);
    let s = L.newState(st, 3);
    s = L.next(s, st);
    expect(s.status).toEqual([0, 0, 0]);
    s = L.next(s, st);
    expect(s.status).toEqual([1, 0, 0]);
    s = L.skip(s, st); // skip the rest: land on move B
    expect(st[s.stage]).toMatchObject({ type: "move", move: 1 });
    s = L.skip(s, st); // skip B and the rest after it: land on C
    expect(st[s.stage]).toMatchObject({ type: "move", move: 2 });
    expect(s.status).toEqual([1, 2, 0]);
    s = L.next(s, st);
    expect(L.finished(s, st)).toBe(true);
    expect(L.tally(s)).toEqual({ done: 2, skipped: 1, total: 3 });
    expect(L.next(s, st)).toBe(s);
    expect(L.skip(s, st)).toBe(s);
  });

  it("skipping the first part of a left-right move skips the whole move", () => {
    const st = L.buildStages(data);
    const s = L.skip(L.newState(st, 3), st);
    expect(st[s.stage]).toMatchObject({ type: "move", move: 1 });
    expect(s.status).toEqual([2, 0, 0]);
  });

  it("goes back without losing counts and resets the stage timer", () => {
    const st = L.buildStages(data);
    let s = L.next(L.next(L.newState(st, 3), st), st);
    s = L.back(s, st);
    expect(s.stage).toBe(1);
    expect(s.status).toEqual([1, 0, 0]);
    expect(s).toMatchObject({ remaining: 20, running: false });
    expect(L.back(L.newState(st, 3), st).stage).toBe(0);
  });

  it("saves and restores exactly, always stopped", () => {
    const st = L.buildStages(data);
    let s = L.toggle(L.newState(st, 3));
    s = L.tick(s, 1.25);
    const back = L.restore(JSON.parse(JSON.stringify(L.snapshot(s))), st, 3);
    expect(back).toMatchObject({ stage: s.stage, remaining: s.remaining, running: false });
    expect(back.status).toEqual(s.status);
    const mid = L.next(L.next(L.newState(st, 3), st), st);
    const again = L.restore(L.snapshot(mid), st, 3);
    expect(again.stage).toBe(mid.stage);
    expect(again.status).toEqual([1, 0, 0]);
  });

  it("falls back to a fresh start for bad saved state", () => {
    const st = L.buildStages(data);
    const fresh = L.newState(st, 3);
    for (const bad of [
      null,
      "x",
      {},
      { v: 2 },
      { v: 1, stage: 99, remaining: 1, status: [0, 0, 0] },
      { v: 1, stage: -1, remaining: 1, status: [0, 0, 0] },
      { v: 1, stage: 0, remaining: 1, status: [0, 0] },
      { v: 1, stage: 0, remaining: 1, status: [0, 0, 7] },
    ]) {
      expect(L.restore(bad, st, 3)).toEqual(fresh);
    }
    const clamped = L.restore({ v: 1, stage: 0, remaining: 9999, status: [0, 0, 0] }, st, 3);
    expect(clamped.remaining).toBe(20);
    const ended = L.restore({ v: 1, stage: st.length, remaining: null, status: [1, 1, 1] }, st, 3);
    expect(L.finished(ended, st)).toBe(true);
  });

  it("formats time and labels", () => {
    expect(L.clock(0)).toBe("0:00");
    expect(L.clock(65)).toBe("1:05");
    expect(L.clock(4.2)).toBe("0:05");
    const st = L.buildStages(data);
    expect(L.amount(at(data.moves, 0), at(st, 0))).toBe("20 seconds");
    expect(L.amount(at(data.moves, 1), at(st, 3))).toBe("3 times");
    expect(L.amount({ name: "x", steps: ["y"], reps: 1 }, { ...at(st, 3), reps: 1 })).toBe(
      "1 time",
    );
    expect(L.sideLabel(at(data.moves, 0), at(st, 0))).toBe("Left side");
    expect(L.sideLabel(at(data.moves, 0), at(st, 1))).toBe("Right side");
    expect(L.sideLabel(at(data.moves, 1), at(st, 3))).toBe("");
  });

  it("accepts the built-in sample and rejects broken data", () => {
    expect(L.validData(L.SAMPLE)).toBe(true);
    expect(L.validData(null)).toBe(false);
    expect(L.validData({ ...data, wayOut: undefined })).toBe(false);
    expect(L.validData({ ...data, moves: [] })).toBe(false);
    expect(L.validData({ ...data, moves: [{ name: "A", steps: ["a"] }] })).toBe(false);
    expect(L.validData({ ...data, moves: [{ name: "A", steps: ["a"], seconds: 5 }] })).toBe(false);
    expect(
      L.validData({ ...data, moves: [{ name: "A", steps: ["a"], seconds: 20, reps: 2 }] }),
    ).toBe(false);
    expect(
      L.validData({
        ...data,
        moves: [{ name: "A", steps: ["a"], seconds: 20, position: "lying" }],
      }),
    ).toBe(false);
  });
});

describe("lullshift.movement@1 data schema and fixtures", () => {
  const schema = JSON.parse(
    readFileSync(join(root, "activities/engines/movement/data.schema.json"), "utf8"),
  );
  const validate = hostDataCheck(schema as object);
  const dir = join(root, "tests/fixtures/engines/lullshift.movement@1");
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

  it("every bundled routine pack validates and keeps its way out", () => {
    const base = join(root, "activities/content/movement");
    const packs = readdirSync(base);
    expect(packs.length).toBeGreaterThanOrEqual(5);
    for (const p of packs) {
      const manifest = JSON.parse(readFileSync(join(base, p, "manifest.json"), "utf8"));
      expect(manifest.engine).toBe("lullshift.movement@1");
      const d = JSON.parse(readFileSync(join(base, p, manifest.data), "utf8"));
      expect(validate(d), `${p}: ${JSON.stringify(validate.errors)}`).toBe(true);
      expect(L.validData(d)).toBe(true);
    }
  });
});
