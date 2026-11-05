/// <reference types="node" />
import { describe, expect, it } from "vitest";
import "../../../../../activities/games/firefly-catch/logic.js";

interface Fly {
  x0: number;
  x: number;
  y: number;
  t: number;
  vy: number;
  vx: number;
  amp: number;
  freq: number;
  phase: number;
  away: boolean;
}
interface State {
  version: number;
  seed: number;
  rng: number;
  reduced: boolean;
  time: number;
  timeLeft: number;
  jarX: number;
  caught: number;
  best: number;
  over: boolean;
  spawnIn: number;
  flies: Fly[];
}
interface Logic {
  W: number;
  H: number;
  JAR_W: number;
  JAR_TOP: number;
  ROUND_SECONDS: number;
  create(seed: number, reduced: boolean): State;
  step(s: State, dt: number, dir: number): number;
  restart(s: State, seed: number): State;
  snapshot(s: State): State;
  validate(raw: unknown): State | null;
}
const L = (globalThis as unknown as { FireflyLogic: Logic }).FireflyLogic;
const DT = 1 / 60;

function run(s: State, seconds: number, dir: (t: number) => number = () => 0): void {
  const n = Math.round(seconds / DT);
  for (let i = 0; i < n; i++) L.step(s, DT, dir(s.time));
}

describe("FireflyLogic", () => {
  it("is deterministic per seed and differs between seeds", () => {
    const a = L.create(7, false);
    const b = L.create(7, false);
    run(a, 20, () => 1);
    run(b, 20, () => 1);
    expect(a).toEqual(b);
    const c = L.create(8, false);
    run(c, 20, () => 1);
    expect(c.flies).not.toEqual(a.flies);
  });

  it("spawns fireflies from the top and keeps them in range", () => {
    const s = L.create(3, false);
    run(s, 30);
    expect(s.flies.length).toBeGreaterThan(0);
    for (const f of s.flies) {
      expect(f.x).toBeGreaterThan(-100);
      expect(f.x).toBeLessThan(L.W + 100);
    }
  });

  it("moves the jar and clamps it inside the meadow", () => {
    const s = L.create(1, false);
    run(s, 5, () => -1);
    expect(s.jarX).toBe(L.JAR_W / 2);
    run(s, 5, () => 1);
    expect(s.jarX).toBe(L.W - L.JAR_W / 2);
    const before = s.jarX;
    L.step(s, DT, 0);
    expect(s.jarX).toBe(before);
  });

  it("catches a firefly that drifts into the jar", () => {
    const s = L.create(1, true);
    s.spawnIn = 100;
    s.flies = [
      {
        x0: s.jarX,
        x: s.jarX,
        y: L.JAR_TOP - 5,
        t: 0,
        vy: 40,
        vx: 0,
        amp: 0,
        freq: 1,
        phase: 0,
        away: false,
      },
    ];
    run(s, 1);
    expect(s.caught).toBe(1);
    expect(s.best).toBe(1);
    expect(s.flies).toHaveLength(0);
  });

  it("lets a missed firefly fly away without penalty", () => {
    const s = L.create(1, true);
    s.spawnIn = 100;
    s.jarX = 40;
    s.flies = [
      {
        x0: 300,
        x: 300,
        y: L.JAR_TOP - 5,
        t: 0,
        vy: 40,
        vx: 0,
        amp: 0,
        freq: 1,
        phase: 0,
        away: false,
      },
    ];
    run(s, 2);
    expect(s.flies[0]?.away).toBe(true);
    expect(s.caught).toBe(0);
    run(s, 15);
    expect(s.flies).toHaveLength(0);
    expect(s.over).toBe(false);
  });

  it("moves in straight lines and slower in reduced motion", () => {
    const s = L.create(5, true);
    run(s, 40);
    expect(s.flies.length).toBeGreaterThan(0);
    for (const f of s.flies) {
      expect(f.amp).toBe(0);
      expect(f.x).toBeCloseTo(f.x0 + f.vx * f.t, 6);
      expect(Math.abs(f.vy)).toBeLessThanOrEqual(45 * 0.7 * 1.5 + 1e-9);
    }
    const n = L.create(5, false);
    run(n, 40);
    expect(n.flies.some((f) => f.amp > 0)).toBe(true);
  });

  it("ends after 90 seconds of game time and then stops changing", () => {
    const s = L.create(2, false);
    run(s, 89);
    expect(s.over).toBe(false);
    expect(s.timeLeft).toBeGreaterThan(0);
    run(s, 2);
    expect(s.over).toBe(true);
    expect(s.timeLeft).toBe(0);
    const frozen = L.snapshot(s);
    L.step(s, DT, 1);
    expect(s).toEqual(frozen);
  });

  it("does not advance time without steps (pause freezes the round)", () => {
    const s = L.create(2, false);
    run(s, 10);
    const left = s.timeLeft;
    expect(left).toBeCloseTo(80, 3);
    expect(s.timeLeft).toBe(left);
  });

  it("restarts with the best score kept", () => {
    const s = L.create(2, false);
    s.caught = 5;
    s.best = 9;
    const n = L.restart(s, 99);
    expect(n.caught).toBe(0);
    expect(n.best).toBe(9);
    expect(n.timeLeft).toBe(L.ROUND_SECONDS);
    expect(n.seed).toBe(99);
  });

  it("restores exactly after a JSON round-trip and continues identically", () => {
    const a = L.create(11, false);
    run(a, 33.3, (t) => (Math.floor(t) % 2 === 0 ? 1 : -1));
    const snap = JSON.parse(JSON.stringify(L.snapshot(a))) as unknown;
    const b = L.validate(snap);
    expect(b).toEqual(a);
    run(a, 25, () => 1);
    run(b as State, 25, () => 1);
    expect(b).toEqual(a);
  });

  it("rejects malformed saved state", () => {
    const good = L.snapshot(L.create(4, false));
    expect(L.validate(null)).toBeNull();
    expect(L.validate({})).toBeNull();
    expect(L.validate({ ...good, version: 2 })).toBeNull();
    expect(L.validate({ ...good, timeLeft: 500 })).toBeNull();
    expect(L.validate({ ...good, caught: -1 })).toBeNull();
    expect(L.validate({ ...good, jarX: Number.NaN })).toBeNull();
    expect(L.validate({ ...good, flies: [{ x: 1 }] })).toBeNull();
    expect(L.validate({ ...good, flies: new Array(500).fill(0) })).toBeNull();
  });
});
