// Rule tests for Breeze Bounce (S10-T02).
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// Load the kit and the logic as classic scripts, the way the activity page does.
const root = fileURLToPath(new URL("../../../../../", import.meta.url));
for (const file of [
  "packages/activity-kits/dist/lullshift-arcade-kit-v1.js",
  "activities/games/breeze-bounce/logic.js",
]) {
  new Function("window", readFileSync(join(root, file), "utf8"))(globalThis);
}

interface Dand {
  x: number;
  y: number;
  released: boolean;
}
interface Ball {
  x: number;
  y: number;
  vx: number;
  vy: number;
  wait: number;
  side: number;
}
interface State {
  version: number;
  seed: number;
  rng: number;
  reduced: boolean;
  ticks: number;
  drifts: number;
  best: number;
  over: boolean;
  newBest: boolean;
  paddleX: number;
  ball: Ball;
  dands: Dand[];
}
interface Logic {
  W: number;
  H: number;
  STEP: number;
  TPS: number;
  PADDLE_W: number;
  PADDLE_Y: number;
  BALL_R: number;
  DAND_R: number;
  DRIFT_WAIT: number;
  create(seed: number, reduced: boolean, best?: number): State;
  step(s: State, dir: number): number;
  restart(s: State, seed: number): State;
  snapshot(s: State): State;
  validate(raw: unknown): State | null;
  released(s: State): number;
  remaining(s: State): number;
  seconds(s: State): number;
  formatTime(ticks: number): string;
}
interface Loop {
  paused: boolean;
  steps: number;
  resume(): void;
  pause(): void;
}
interface Kit {
  Loop: new (o: {
    update(dt: number): void;
    stepMs?: number;
    visibility: null;
    frames: {
      request(cb: (t: number) => void): number;
      cancel(id: number): void;
    };
  }) => Loop;
}
const L = (globalThis as unknown as { BreezeBounceLogic: Logic }).BreezeBounceLogic;
const K = (globalThis as unknown as { LullshiftKits: { arcade: Kit } }).LullshiftKits.arcade;

function run(s: State, ticks: number, dir = 0): void {
  for (let i = 0; i < ticks; i++) L.step(s, dir);
}
// A state with a free-flying seed in the middle and a single far dandelion, so rules test in isolation.
function flying(): State {
  const s = L.create(7, false, 0);
  s.dands = [
    { x: 40, y: 30, released: false },
    { x: 360, y: 30, released: true },
  ];
  s.ball = { x: 200, y: 150, vx: 0, vy: 0, wait: 0, side: 1 };
  return s;
}

describe("Breeze Bounce layout", () => {
  it("is deterministic per seed and differs between seeds", () => {
    expect(L.create(42, false)).toEqual(L.create(42, false));
    expect(L.create(42, false).dands).not.toEqual(L.create(43, false).dands);
  });

  it("lays out dandelions in rows inside the field, none released", () => {
    for (let seed = 1; seed < 40; seed++) {
      const s = L.create(seed, false);
      expect(s.dands.length).toBeGreaterThanOrEqual(8);
      expect(s.dands.length).toBeLessThanOrEqual(32);
      expect(L.remaining(s)).toBe(s.dands.length);
      for (const d of s.dands) {
        expect(d.x).toBeGreaterThan(L.DAND_R);
        expect(d.x).toBeLessThan(L.W - L.DAND_R);
        expect(d.y).toBeGreaterThan(L.DAND_R);
        expect(d.y).toBeLessThan(L.PADDLE_Y - 40);
      }
    }
  });

  it("starts the seed resting at the top with no round time used", () => {
    const s = L.create(5, false);
    expect(s.ticks).toBe(0);
    expect(s.ball.wait).toBe(L.DRIFT_WAIT);
    expect(s.ball.y).toBeLessThan(30);
    expect(s.drifts).toBe(0);
  });
});

describe("Breeze Bounce physics", () => {
  it("launches downward after the short rest", () => {
    const s = L.create(5, false);
    run(s, L.DRIFT_WAIT - 1);
    expect(s.ball.vy).toBe(0);
    run(s, 1);
    expect(s.ball.vy).toBeGreaterThan(0);
  });

  it("bounces off the side walls and the ceiling", () => {
    const s = flying();
    s.ball = { x: L.BALL_R + 1, y: 150, vx: -150, vy: 0, wait: 0, side: 1 };
    run(s, 2);
    expect(s.ball.vx).toBeGreaterThan(0);
    s.ball = { x: L.W - L.BALL_R - 1, y: 150, vx: 150, vy: 0, wait: 0, side: 1 };
    run(s, 2);
    expect(s.ball.vx).toBeLessThan(0);
    s.ball = { x: 200, y: L.BALL_R + 1, vx: 0, vy: -150, wait: 0, side: 1 };
    run(s, 2);
    expect(s.ball.vy).toBeGreaterThan(0);
  });

  it("bounces off the leaf and steers by where it lands", () => {
    const s = flying();
    s.paddleX = 200;
    s.ball = { x: 200, y: L.PADDLE_Y - L.BALL_R - 1, vx: 0, vy: 150, wait: 0, side: 1 };
    run(s, 1);
    expect(s.ball.vy).toBeLessThan(0);
    expect(Math.abs(s.ball.vx)).toBeLessThan(1);

    const right = flying();
    right.paddleX = 200;
    right.ball = { x: 225, y: L.PADDLE_Y - L.BALL_R - 1, vx: 0, vy: 150, wait: 0, side: 1 };
    run(right, 1);
    expect(right.ball.vx).toBeGreaterThan(30);
    expect(right.ball.vy).toBeLessThan(0);

    const left = flying();
    left.paddleX = 200;
    left.ball = { x: 175, y: L.PADDLE_Y - L.BALL_R - 1, vx: 0, vy: 150, wait: 0, side: 1 };
    run(left, 1);
    expect(left.ball.vx).toBeLessThan(-30);
  });

  it("keeps a constant speed through bounces", () => {
    const s = flying();
    s.paddleX = 200;
    s.ball = { x: 230, y: L.PADDLE_Y - L.BALL_R - 1, vx: 0, vy: 150, wait: 0, side: 1 };
    run(s, 1);
    expect(Math.hypot(s.ball.vx, s.ball.vy)).toBeCloseTo(150, 5);
  });

  it("moves the leaf with the keys and keeps it inside the field", () => {
    const s = flying();
    s.ball = { x: 200, y: 20, vx: 0, vy: 0, wait: 100000, side: 1 };
    const x0 = s.paddleX;
    run(s, 10, 1);
    expect(s.paddleX).toBeGreaterThan(x0);
    run(s, 600, 1);
    expect(s.paddleX).toBe(L.W - L.PADDLE_W / 2);
    run(s, 600, -1);
    expect(s.paddleX).toBe(L.PADDLE_W / 2);
  });

  it("runs slower in reduced motion", () => {
    const a = L.create(9, false);
    const b = L.create(9, true);
    run(a, L.DRIFT_WAIT);
    run(b, L.DRIFT_WAIT);
    expect(Math.hypot(b.ball.vx, b.ball.vy)).toBeLessThan(Math.hypot(a.ball.vx, a.ball.vy));
  });
});

describe("Breeze Bounce dandelions and drifts", () => {
  it("releases a dandelion on touch and bounces the seed away", () => {
    const s = flying();
    s.dands.push({ x: 200, y: 100, released: false });
    s.ball = { x: 200, y: 100 + L.DAND_R + L.BALL_R + 1, vx: 0, vy: -150, wait: 0, side: 1 };
    const got = L.step(s, 0);
    expect(got).toBe(1);
    expect(s.dands[2]!.released).toBe(true);
    expect(s.ball.vy).toBeGreaterThan(0);
  });

  it("does not hit a dandelion that is already released", () => {
    const s = flying();
    s.dands[1] = { x: 200, y: 100, released: true };
    s.ball = { x: 200, y: 100 + L.DAND_R + L.BALL_R + 1, vx: 0, vy: -150, wait: 0, side: 1 };
    expect(L.step(s, 0)).toBe(0);
    expect(s.ball.vy).toBeLessThan(0);
  });

  it("counts a miss as a drift and sends a new seed from the top, with no other penalty", () => {
    const s = flying();
    s.paddleX = 40;
    s.ball = { x: 360, y: L.H + L.BALL_R - 1, vx: 0, vy: 150, wait: 0, side: 1 };
    run(s, 1);
    expect(s.drifts).toBe(1);
    expect(s.over).toBe(false);
    expect(s.ball.y).toBeLessThan(30);
    expect(s.ball.wait).toBe(L.DRIFT_WAIT);
    expect(L.remaining(s)).toBe(1);
    run(s, L.DRIFT_WAIT);
    expect(s.ball.vy).toBeGreaterThan(0);
  });

  it("finishes the round when the last dandelion is released and records the best time", () => {
    const s = flying();
    s.ticks = 600;
    s.ball = { x: 40, y: 30 + L.DAND_R + L.BALL_R + 1, vx: 0, vy: -150, wait: 0, side: 1 };
    L.step(s, 0);
    expect(s.over).toBe(true);
    expect(s.best).toBe(601);
    expect(s.newBest).toBe(true);
    expect(L.seconds(s)).toBeCloseTo(601 / 60, 6);
  });

  it("keeps a faster best and replaces a slower one", () => {
    const finish = (best: number, ticks: number): State => {
      const s = flying();
      s.best = best;
      s.ticks = ticks;
      s.ball = { x: 40, y: 30 + L.DAND_R + L.BALL_R + 1, vx: 0, vy: -150, wait: 0, side: 1 };
      L.step(s, 0);
      return s;
    };
    const slower = finish(100, 900);
    expect(slower.best).toBe(100);
    expect(slower.newBest).toBe(false);
    const faster = finish(1000, 900);
    expect(faster.best).toBe(901);
    expect(faster.newBest).toBe(true);
  });

  it("restarts with a new seeded layout and keeps the best", () => {
    const s = L.create(1, false, 500);
    const next = L.restart(s, 2);
    expect(next.best).toBe(500);
    expect(next.ticks).toBe(0);
    expect(next.dands).toEqual(L.create(2, false).dands);
  });

  it("can be cleared by simple play on many seeds", () => {
    for (const seed of [1, 2, 3, 11, 99]) {
      const s = L.create(seed, false);
      let hits = 0;
      let prevVy = 0;
      for (let t = 0; t < 60 * 600 && !s.over; t++) {
        // A bot that keeps under the seed and varies where it lands, to send it around.
        const half = L.PADDLE_W / 2;
        const off = ((hits * 0.37) % 1.6) - 0.8;
        s.paddleX = Math.min(L.W - half, Math.max(half, s.ball.x - off * half));
        L.step(s, 0);
        if (prevVy > 0 && s.ball.vy < 0 && s.ball.y > L.PADDLE_Y - 20) hits++;
        prevVy = s.ball.vy;
      }
      expect(s.over).toBe(true);
    }
  });
});

describe("Breeze Bounce timing and pause", () => {
  it("counts game time only in steps", () => {
    const s = L.create(3, false);
    run(s, 120);
    expect(s.ticks).toBe(120);
    expect(L.seconds(s)).toBe(2);
    expect(L.formatTime(s.ticks)).toBe("0:02.0");
    expect(L.formatTime(3600 + 90)).toBe("1:01.5");
  });

  it("does not advance once the round is over", () => {
    const s = flying();
    s.ball = { x: 40, y: 30 + L.DAND_R + L.BALL_R + 1, vx: 0, vy: -150, wait: 0, side: 1 };
    L.step(s, 0);
    const done = L.snapshot(s);
    run(s, 50, 1);
    expect(s).toEqual(done);
  });

  it("passes no time while the host loop is paused", () => {
    let cb: ((t: number) => void) | null = null;
    const s = L.create(4, false);
    const loop = new K.Loop({
      stepMs: 1000 / 60,
      visibility: null,
      frames: {
        request(f) {
          cb = f;
          return 1;
        },
        cancel() {
          cb = null;
        },
      },
      update: () => {
        L.step(s, 0);
      },
    });
    loop.resume();
    let now = 0;
    const frame = (ms: number): void => {
      now += ms;
      if (cb) cb(now);
    };
    frame(0);
    frame(100);
    const before = L.snapshot(s);
    expect(before.ticks).toBeGreaterThan(0);
    loop.pause();
    expect(loop.paused).toBe(true);
    frame(5000);
    frame(5000);
    expect(s).toEqual(before);
    loop.resume();
    frame(0);
    frame(50);
    expect(s.ticks).toBeGreaterThan(before.ticks);
  });
});

describe("Breeze Bounce saved state", () => {
  it("survives a JSON round trip and then plays identically", () => {
    const a = L.create(21, false, 700);
    run(a, 500, 1);
    const saved = JSON.parse(JSON.stringify(L.snapshot(a))) as unknown;
    const b = L.validate(saved);
    expect(b).toEqual(a);
    run(a, 300);
    run(b as State, 300);
    expect(b).toEqual(a);
  });

  it("rejects malformed saves", () => {
    const good = L.snapshot(L.create(8, false));
    const bad: unknown[] = [
      null,
      "x",
      {},
      { ...good, version: 2 },
      { ...good, ticks: -1 },
      { ...good, ticks: 1.5 },
      { ...good, paddleX: Number.NaN },
      { ...good, paddleX: 9999 },
      { ...good, ball: null },
      { ...good, ball: { ...good.ball, vx: 1e9 } },
      { ...good, ball: { ...good.ball, side: 0 } },
      { ...good, dands: [] },
      { ...good, dands: new Array(500).fill(good.dands[0]) },
      { ...good, dands: [{ x: "a", y: 1, released: false }] },
      { ...good, over: true },
      { ...good, best: -3 },
    ];
    for (const raw of bad) expect(L.validate(raw)).toBeNull();
  });
});
