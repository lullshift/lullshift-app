/// <reference types="node" />
import { describe, expect, it } from "vitest";
import "../../../../../activities/games/kite-drift/logic.js";

interface Cloud {
  x: number;
  gapY: number;
  done: boolean;
}
interface State {
  version: number;
  seed: number;
  rng: number;
  reduced: boolean;
  pace: string;
  time: number;
  scroll: number;
  y: number;
  vy: number;
  gaps: number;
  streak: number;
  bestStreak: number;
  bump: number;
  clouds: Cloud[];
}
interface Logic {
  W: number;
  H: number;
  KITE_X: number;
  KITE_R: number;
  BANK_W: number;
  GAP_H: number;
  SPACING: number;
  MAX_SHIFT: number;
  VMIN: number;
  VMAX: number;
  PACES: string[];
  PACE_SCROLL: Record<string, number>;
  isPace(p: string): boolean;
  nextPace(p: string): string;
  create(seed: number, reduced: boolean, pace?: string): State;
  step(s: State, dt: number, hold: boolean): { passed: number; bumped: number };
  restart(s: State, seed: number): State;
  snapshot(s: State): State;
  validate(raw: unknown): State | null;
}
const L = (globalThis as unknown as { KiteLogic: Logic }).KiteLogic;
const DT = 1 / 60;

function run(s: State, seconds: number, hold: (s: State) => boolean = () => false): void {
  const n = Math.round(seconds / DT);
  for (let i = 0; i < n; i++) L.step(s, DT, hold(s));
}

/** A simple autopilot: aim at the middle of the next gap that is still ahead. */
function pilot(s: State): boolean {
  const wx = s.scroll + L.KITE_X;
  const next = s.clouds.find((c) => !c.done && c.x + L.BANK_W > wx - L.KITE_R);
  const target = next ? next.gapY : L.H / 2;
  return s.y > target + 4;
}

describe("KiteLogic", () => {
  it("generates the same banks per seed and different banks per seed", () => {
    expect(L.create(5, false)).toEqual(L.create(5, false));
    expect(L.create(5, false).clouds).not.toEqual(L.create(6, false).clouds);
  });

  it("makes every gap wide, in range and reachable from its neighbour", () => {
    for (let seed = 1; seed <= 100; seed++) {
      const s = L.create(seed, false);
      run(s, 120, pilot);
      for (let i = 0; i < s.clouds.length; i++) {
        const c = s.clouds[i] as Cloud;
        expect(c.gapY - L.GAP_H / 2).toBeGreaterThanOrEqual(0);
        expect(c.gapY + L.GAP_H / 2).toBeLessThanOrEqual(L.H);
        const p = s.clouds[i - 1];
        if (p) {
          expect(Math.abs(c.gapY - p.gapY)).toBeLessThanOrEqual(L.MAX_SHIFT + 1e-9);
          expect(c.x - p.x).toBe(L.SPACING);
        }
      }
    }
  });

  it("lifts while held, sinks when released, and caps velocity", () => {
    const s = L.create(1, false);
    run(s, 0.5, () => true);
    expect(s.vy).toBeLessThan(0);
    const up = s.y;
    expect(up).toBeLessThan(L.H / 2);
    run(s, 10, () => true);
    expect(s.vy).toBeGreaterThanOrEqual(L.VMIN);
    run(s, 10, () => false);
    expect(s.vy).toBeLessThanOrEqual(L.VMAX);
    expect(s.y).toBeLessThanOrEqual(L.H - L.KITE_R);
    expect(s.y).toBeGreaterThanOrEqual(L.KITE_R);
  });

  it("counts a gap and builds a streak when the kite passes through", () => {
    const s = L.create(9, false);
    const first = s.clouds[0] as Cloud;
    s.y = first.gapY;
    let passed = 0;
    for (let i = 0; i < 60 * 20 && passed === 0; i++) {
      // hold the kite level at the gap height by hand
      s.vy = 0;
      s.y = first.gapY;
      passed += L.step(s, DT, false).passed;
    }
    expect(passed).toBe(1);
    expect(s.gaps).toBe(1);
    expect(s.streak).toBe(1);
    expect(s.bestStreak).toBe(1);
  });

  it("bumps to the middle of the gap and ends the streak when a cloud is touched", () => {
    const s = L.create(9, false);
    s.streak = 4;
    s.bestStreak = 4;
    s.gaps = 4;
    const first = s.clouds[0] as Cloud;
    // Put the kite far from the gap, just before the bank.
    s.scroll = first.x - L.KITE_X - L.KITE_R - 1;
    s.y =
      first.gapY + L.GAP_H / 2 + 60 > L.H - L.KITE_R
        ? first.gapY - L.GAP_H / 2 - 30
        : first.gapY + L.GAP_H / 2 + 30;
    s.vy = 0;
    let bumped = 0;
    for (let i = 0; i < 120 && bumped === 0; i++) bumped += L.step(s, DT, false).bumped;
    expect(bumped).toBe(1);
    expect(s.streak).toBe(0);
    expect(s.bestStreak).toBe(4);
    expect(s.gaps).toBe(4);
    expect(s.y).toBe(first.gapY);
    expect(s.vy).toBe(0);
    expect(s.bump).toBeGreaterThan(0);
    // The same bank neither bumps again nor counts as passed.
    run(s, 5);
    expect(s.gaps).toBe(4);
    expect(s.streak).toBe(0);
  });

  it("never ends: a long flight with no input only bumps", () => {
    const s = L.create(3, false);
    run(s, 300);
    expect(s.gaps).toBeGreaterThanOrEqual(0);
    expect(s.clouds.length).toBeGreaterThan(0);
    expect(s.clouds.length).toBeLessThanOrEqual(12);
  });

  it("an autopilot can fly long streaks, so gaps are passable", () => {
    const s = L.create(21, false);
    run(s, 200, pilot);
    expect(s.gaps).toBeGreaterThan(10);
    expect(s.bestStreak).toBeGreaterThan(5);
  });

  it("restarts keeping the best streak", () => {
    const s = L.create(1, false);
    s.bestStreak = 7;
    s.gaps = 12;
    const n = L.restart(s, 4);
    expect(n.gaps).toBe(0);
    expect(n.streak).toBe(0);
    expect(n.bestStreak).toBe(7);
  });

  it("restores exactly after a JSON round-trip and continues identically", () => {
    const a = L.create(13, false);
    run(a, 47.7, (s) => Math.floor(s.time * 1.3) % 2 === 0);
    const snap = JSON.parse(JSON.stringify(L.snapshot(a))) as unknown;
    const b = L.validate(snap) as State;
    expect(b).toEqual(a);
    run(a, 40, pilot);
    run(b, 40, pilot);
    expect(b).toEqual(a);
  });

  it("rejects malformed saved state", () => {
    const good = L.snapshot(L.create(4, false));
    expect(L.validate(null)).toBeNull();
    expect(L.validate({})).toBeNull();
    expect(L.validate({ ...good, version: 9 })).toBeNull();
    expect(L.validate({ ...good, vy: 9999 })).toBeNull();
    expect(L.validate({ ...good, gaps: 1.5 })).toBeNull();
    expect(L.validate({ ...good, streak: 5, bestStreak: 2 })).toBeNull();
    expect(L.validate({ ...good, clouds: [] })).toBeNull();
    expect(L.validate({ ...good, clouds: [{ x: 1, gapY: 0, done: false }] })).toBeNull();
  });
});

describe("Kite Drift pace", () => {
  it("cycles slow, normal, brisk", () => {
    expect(L.PACES).toEqual(["slow", "normal", "brisk"]);
    expect(L.nextPace("slow")).toBe("normal");
    expect(L.nextPace("normal")).toBe("brisk");
    expect(L.nextPace("brisk")).toBe("slow");
    expect(L.create(1, false).pace).toBe("normal");
    expect(L.create(1, false, "nope").pace).toBe("normal");
  });

  it("scrolls at a speed set by the pace and nothing else changes in the layout", () => {
    const distance = (pace: string) => {
      const s = L.create(8, false, pace);
      for (let i = 0; i < 60; i++) L.step(s, 1 / 60, false);
      return s.scroll;
    };
    const slow = distance("slow");
    const normal = distance("normal");
    const brisk = distance("brisk");
    expect(slow).toBeLessThan(normal);
    expect(normal).toBeLessThan(brisk);
    expect(normal).toBeCloseTo(60, 5);
    expect(L.create(8, false, "slow").clouds).toEqual(L.create(8, false, "brisk").clouds);
  });

  it("the next gap stays reachable even at the brisk pace", () => {
    const maxClimb = L.MAX_SHIFT;
    const seconds = L.SPACING / L.PACE_SCROLL.brisk!;
    // lift acceleration 240 px/s^2 up to 100 px/s: easily more than a 90 px change in the time
    expect(100 * seconds - 100).toBeGreaterThan(maxClimb);
  });

  it("a flight keeps its pace on restart and never ends on a bump", () => {
    for (const pace of L.PACES) {
      let s = L.create(3, false, pace);
      for (let i = 0; i < 60 * 60; i++) L.step(s, 1 / 60, i % 90 < 30);
      expect(s.gaps + s.clouds.length).toBeGreaterThan(0);
      s = L.restart(s, 4);
      expect(s.pace).toBe(pace);
    }
  });

  it("round-trips pace and restores older saves without one as normal", () => {
    const s = L.create(6, false, "brisk");
    for (let i = 0; i < 100; i++) L.step(s, 1 / 60, true);
    expect(L.validate(JSON.parse(JSON.stringify(s)))).toEqual(s);
    const old = JSON.parse(JSON.stringify(L.create(6, false))) as Record<string, unknown>;
    delete old.pace;
    expect(L.validate(old)?.pace).toBe("normal");
    expect(L.validate({ ...s, pace: "warp" })).toBeNull();
  });
});
