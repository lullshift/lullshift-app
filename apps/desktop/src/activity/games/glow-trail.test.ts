// Rule tests for Glow Trail (S10-T03).
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// Load the kit and the logic as classic scripts, the way the activity page does.
const root = fileURLToPath(new URL("../../../../../", import.meta.url));
for (const file of [
  "packages/activity-kits/dist/lullshift-arcade-kit-v1.js",
  "activities/games/glow-trail/logic.js",
]) {
  new Function("window", readFileSync(join(root, file), "utf8"))(globalThis);
}

type Cell = [number, number];
interface State {
  version: number;
  seed: number;
  rng: number;
  slower: boolean;
  ticks: number;
  cool: number;
  dir: number;
  want: number;
  trail: Cell[];
  dew: Cell[];
  length: number;
  longest: number;
  collected: number;
  best: number;
  over: boolean;
  newBest: boolean;
}
interface Logic {
  COLS: number;
  ROWS: number;
  TPS: number;
  ROUND_TICKS: number;
  MOVE_TICKS: number;
  MOVE_TICKS_SLOW: number;
  START_LEN: number;
  DEW_COUNT: number;
  COLLECTED: number;
  EASED: number;
  create(seed: number, slower?: boolean, best?: number): State;
  step(s: State): number;
  steer(s: State, d: number): boolean;
  setSlower(s: State, slower: boolean): void;
  restart(s: State, seed: number): State;
  snapshot(s: State): State;
  validate(raw: unknown): State | null;
  secondsLeft(s: State): number;
}
interface Loop {
  paused: boolean;
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
const L = (globalThis as unknown as { GlowTrailLogic: Logic }).GlowTrailLogic;
const K = (globalThis as unknown as { LullshiftKits: { arcade: Kit } }).LullshiftKits.arcade;
const UP = 0;
const RIGHT = 1;
const DOWN = 2;
const LEFT = 3;

// One firefly move (a full move interval of ticks); returns the events of that move.
function move(s: State): number {
  let ev = 0;
  for (let i = 0; i < (s.slower ? L.MOVE_TICKS_SLOW : L.MOVE_TICKS); i++) ev |= L.step(s);
  return ev;
}
function ticks(s: State, n: number): void {
  for (let i = 0; i < n; i++) L.step(s);
}
function key(c: Cell): string {
  return c[0] + "," + c[1];
}
function sane(s: State): void {
  const seen = new Set<string>();
  for (const c of [...s.trail, ...s.dew]) {
    expect(c[0]).toBeGreaterThanOrEqual(0);
    expect(c[0]).toBeLessThan(L.COLS);
    expect(c[1]).toBeGreaterThanOrEqual(0);
    expect(c[1]).toBeLessThan(L.ROWS);
    expect(seen.has(key(c))).toBe(false);
    seen.add(key(c));
  }
  expect(s.trail.length).toBeLessThanOrEqual(s.length);
  expect(s.dew.length).toBe(L.DEW_COUNT);
}

describe("Glow Trail setup", () => {
  it("is deterministic per seed and differs between seeds", () => {
    expect(L.create(5)).toEqual(L.create(5));
    expect(L.create(5).dew).not.toEqual(L.create(6).dew);
  });

  it("starts with a short trail, free dew drops and the whole round to play", () => {
    for (let seed = 1; seed < 50; seed++) {
      const s = L.create(seed);
      sane(s);
      expect(s.trail.length).toBe(L.START_LEN);
      expect(s.length).toBe(L.START_LEN);
      expect(s.ticks).toBe(0);
      expect(L.secondsLeft(s)).toBe(120);
    }
  });

  it("is a 2 minute round of game time", () => {
    expect(L.ROUND_TICKS).toBe(120 * L.TPS);
  });
});

describe("Glow Trail movement", () => {
  it("moves one cell at a steady pace and never sooner", () => {
    const s = L.create(1);
    const head = s.trail[0]!;
    ticks(s, L.MOVE_TICKS - 1);
    expect(s.trail[0]).toEqual(head);
    L.step(s);
    expect(s.trail[0]).toEqual([head[0] + 1, head[1]]);
    expect(s.trail.length).toBe(L.START_LEN);
  });

  it("has a slower option with a longer interval", () => {
    expect(L.MOVE_TICKS_SLOW).toBeGreaterThan(L.MOVE_TICKS);
    const s = L.create(1, true);
    const head = s.trail[0]!;
    ticks(s, L.MOVE_TICKS_SLOW - 1);
    expect(s.trail[0]).toEqual(head);
    L.step(s);
    expect(s.trail[0]).toEqual([head[0] + 1, head[1]]);
    const t = L.create(1, false);
    L.setSlower(t, true);
    expect(t.slower).toBe(true);
    L.setSlower(t, false);
    expect(t.slower).toBe(false);
  });

  it("turns when steered and ignores a straight reversal", () => {
    const s = L.create(1);
    expect(L.steer(s, LEFT)).toBe(false);
    expect(L.steer(s, UP)).toBe(true);
    const [x, y] = s.trail[0]!;
    move(s);
    expect(s.trail[0]).toEqual([x, y - 1]);
    expect(L.steer(s, DOWN)).toBe(false);
    expect(L.steer(s, LEFT)).toBe(true);
    expect(L.steer(s, 7)).toBe(false);
    expect(L.steer(s, 1.5)).toBe(false);
  });
});

describe("Glow Trail growth", () => {
  it("collects a dew drop, grows by one, and respawns a drop on a free cell", () => {
    const s = L.create(3);
    const [hx, hy] = s.trail[0]!;
    s.dew = [[hx + 1, hy], s.dew[1]!, s.dew[2]!];
    sane(s);
    const ev = move(s);
    expect(ev & L.COLLECTED).toBeTruthy();
    expect(s.collected).toBe(1);
    expect(s.length).toBe(L.START_LEN + 1);
    expect(s.trail.length).toBe(L.START_LEN + 1);
    expect(s.longest).toBe(L.START_LEN + 1);
    sane(s);
  });

  it("makes the trail longer with each drop", () => {
    const s = L.create(3);
    for (let n = 1; n <= 5; n++) {
      const [hx, hy] = s.trail[0]!;
      s.want = RIGHT;
      s.dir = RIGHT;
      s.dew[0] = [hx + 1, hy];
      // keep the way clear of the edge by stepping back to the middle each time
      move(s);
      expect(s.length).toBe(L.START_LEN + n);
      s.trail = s.trail.map((_c, i) => [10 - i, 7] as Cell);
      s.dew = s.dew.filter((d) => !s.trail.some((t) => key(t) === key(d)));
      while (s.dew.length < L.DEW_COUNT) s.dew.push([n, 1 + s.dew.length + n]);
    }
    expect(s.longest).toBe(L.START_LEN + 5);
  });

  it("uses seeded spawns, so the same play gives the same garden", () => {
    const a = L.create(11);
    const b = L.create(11);
    for (let i = 0; i < 40; i++) {
      const [hx, hy] = a.trail[0]!;
      a.dew[0] = [Math.min(L.COLS - 1, hx + 1), hy];
      b.dew[0] = [Math.min(L.COLS - 1, hx + 1), hy];
      move(a);
      move(b);
    }
    expect(a).toEqual(b);
  });
});

describe("Glow Trail soft resets", () => {
  it("eases the trail back at the edge, stays put, and keeps playing", () => {
    const s = L.create(2);
    s.length = 8;
    s.longest = 8;
    s.trail = [];
    for (let i = 0; i < 8; i++) s.trail.push([L.COLS - 1 - i, 5]);
    s.dew = [
      [1, 1],
      [2, 1],
      [3, 1],
    ];
    s.dir = RIGHT;
    s.want = RIGHT;
    const ev = move(s);
    expect(ev & L.EASED).toBeTruthy();
    expect(s.trail.length).toBe(L.START_LEN);
    expect(s.length).toBe(L.START_LEN);
    expect(s.trail[0]).toEqual([L.COLS - 1, 5]);
    expect(s.over).toBe(false);
    expect(s.longest).toBe(8);
    // carries on after turning
    L.steer(s, UP);
    move(s);
    expect(s.trail[0]).toEqual([L.COLS - 1, 4]);
    sane(s);
  });

  it("eases back when the firefly touches its own trail, and keeps its head moving", () => {
    const s = L.create(2);
    // A hook shape: heading left into a cell the trail already holds.
    s.trail = [
      [5, 5],
      [6, 5],
      [6, 4],
      [5, 4],
      [4, 4],
      [4, 5],
    ] as Cell[];
    s.length = 6;
    s.longest = 6;
    s.dir = LEFT;
    s.want = UP;
    s.dew = [
      [1, 1],
      [2, 1],
      [3, 1],
    ];
    const ev = move(s); // up into [5,4], part of its own trail
    expect(ev & L.EASED).toBeTruthy();
    expect(s.length).toBe(L.START_LEN);
    expect(s.trail.length).toBe(L.START_LEN);
    expect(s.trail[0]).toEqual([5, 4]);
    expect(s.longest).toBe(6);
    expect(s.over).toBe(false);
    sane(s);
  });

  it("does not count moving into the cell the tail just left as a hit", () => {
    const s = L.create(2);
    // A tight loop of four cells: the head chases its own tail.
    s.trail = [
      [5, 5],
      [5, 6],
      [4, 6],
      [4, 5],
    ] as Cell[];
    s.length = 4;
    s.longest = 4;
    s.dir = UP;
    s.want = LEFT;
    s.dew = [
      [1, 1],
      [2, 1],
      [3, 1],
    ];
    const ev = move(s);
    expect(ev & L.EASED).toBe(0);
    expect(s.length).toBe(4);
    expect(s.trail[0]).toEqual([4, 5]);
  });

  it("never ends the round on a bump and never leaves a duplicate cell, over a long wild run", () => {
    const s = L.create(77);
    let state = 12345;
    const rnd = (): number => {
      state = (Math.imul(state, 1103515245) + 12345) >>> 0;
      return state / 4294967296;
    };
    while (!s.over) {
      if (rnd() < 0.3) L.steer(s, Math.floor(rnd() * 4));
      L.step(s);
      sane(s);
    }
    expect(s.ticks).toBe(L.ROUND_TICKS);
  });
});

describe("Glow Trail round and timing", () => {
  it("ends after exactly 2 minutes of steps and records the longest trail as the best", () => {
    const s = L.create(4, false, 0);
    s.longest = 9;
    ticks(s, L.ROUND_TICKS - 1);
    expect(s.over).toBe(false);
    expect(L.secondsLeft(s)).toBe(1);
    L.step(s);
    expect(s.over).toBe(true);
    expect(L.secondsLeft(s)).toBe(0);
    expect(s.best).toBe(9);
    expect(s.newBest).toBe(true);
  });

  it("keeps a higher earlier best", () => {
    const s = L.create(4, false, 40);
    ticks(s, L.ROUND_TICKS);
    expect(s.best).toBe(40);
    expect(s.newBest).toBe(false);
  });

  it("stops completely once over", () => {
    const s = L.create(4);
    ticks(s, L.ROUND_TICKS);
    const done = L.snapshot(s);
    ticks(s, 500);
    expect(L.steer(s, UP)).toBe(false);
    expect(s).toEqual(done);
  });

  it("restarts with a fresh garden and keeps the best and the pace", () => {
    const s = L.create(1, true, 12);
    const next = L.restart(s, 2);
    expect(next.best).toBe(12);
    expect(next.slower).toBe(true);
    expect(next.ticks).toBe(0);
    expect(next.dew).toEqual(L.create(2, true).dew);
  });

  it("passes no time while the host loop is paused", () => {
    let cb: ((t: number) => void) | null = null;
    const s = L.create(6);
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
        L.step(s);
      },
    });
    loop.resume();
    let now = 0;
    const frame = (ms: number): void => {
      now += ms;
      if (cb) cb(now);
    };
    frame(0);
    frame(200);
    const before = L.snapshot(s);
    expect(before.ticks).toBeGreaterThan(0);
    loop.pause();
    expect(loop.paused).toBe(true);
    frame(10000);
    frame(10000);
    expect(s).toEqual(before);
    loop.resume();
    frame(0);
    frame(100);
    expect(s.ticks).toBeGreaterThan(before.ticks);
  });
});

describe("Glow Trail saved state", () => {
  it("survives a JSON round trip mid-round and then plays identically", () => {
    const a = L.create(21, true, 9);
    for (let i = 0; i < 30; i++) {
      if (i % 4 === 0) L.steer(a, (i / 4) % 4);
      move(a);
    }
    const saved = JSON.parse(JSON.stringify(L.snapshot(a))) as unknown;
    const b = L.validate(saved) as State;
    expect(b).toEqual(a);
    for (let i = 0; i < 400; i++) {
      L.step(a);
      L.step(b);
    }
    expect(b).toEqual(a);
  });

  it("rejects malformed saves", () => {
    const good = L.snapshot(L.create(8));
    const bad: unknown[] = [
      null,
      "x",
      {},
      { ...good, version: 2 },
      { ...good, ticks: -1 },
      { ...good, ticks: L.ROUND_TICKS + 1 },
      { ...good, dir: 4 },
      { ...good, length: 2 },
      { ...good, longest: 1 },
      { ...good, trail: [[1, 1]] },
      {
        ...good,
        trail: [
          [1, 1],
          [1, 1],
          [1, 2],
        ],
      },
      {
        ...good,
        trail: [
          [1, 1],
          [5, 5],
          [5, 6],
        ],
      },
      {
        ...good,
        trail: [
          [-1, 1],
          [0, 1],
          [1, 1],
        ],
      },
      { ...good, dew: [] },
      { ...good, dew: [good.trail[0], [0, 0], [1, 1]] },
      { ...good, over: true },
      { ...good, best: 1e9 },
    ];
    for (const raw of bad) expect(L.validate(raw)).toBeNull();
  });
});
