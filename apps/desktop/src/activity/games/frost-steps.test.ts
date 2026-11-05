// Frost Steps rules: sliding, the search, generation guarantees and saved-puzzle checks.
/// <reference types="node" />
import { describe, expect, it } from "vitest";
import "../../../../../packages/activity-kits/dist/lullshift-grid-kit-v1.js";
import "../../../../../activities/games/frost-steps/logic.js";

interface Puzzle {
  w: number;
  h: number;
  rocks: number[];
  start: number;
  goal: number;
  best: number;
}
type Dir = "up" | "down" | "left" | "right";
interface Logic {
  SIZES: number[];
  MIN_MOVES: Record<number, number>;
  DIRS: Dir[];
  dateSeed(d: Date): number;
  sizeForSeed(seed: number): number;
  slide(p: Puzzle, pos: number, dir: Dir): { pos: number; moved: boolean; stop: string };
  solve(p: Puzzle): { best: number; path: Dir[] } | null;
  generate(seed: number, size?: number): Puzzle | null;
  validatePuzzle(raw: unknown): Puzzle | null;
  copyPuzzle(p: Puzzle): Puzzle;
}
const L = (globalThis as unknown as { FrostLogic: Logic }).FrostLogic;

// A hand-made 5 by 5 pond is not a legal game size, but slide() and solve() work on any size.
function pond(w: number, rocks: number[], start: number, goal: number): Puzzle {
  return { w, h: w, rocks, start, goal, best: 0 };
}

// Independent check: plays a path with the slide rules and reports where it ends.
function walk(p: Puzzle, path: Dir[]): number {
  let at = p.start;
  for (const d of path) {
    const r = L.slide(p, at, d);
    expect(r.moved).toBe(true);
    at = r.pos;
  }
  return at;
}

describe("Frost Steps sliding", () => {
  // 4 by 4, row by row:  . . . .   rock at 6 (row 1, col 2), lantern at 15
  const p = pond(4, [6], 0, 15);

  it("slides to the edge on an open lane", () => {
    expect(L.slide(p, 0, "right")).toEqual({ pos: 3, moved: true, stop: "edge" });
    expect(L.slide(p, 0, "down")).toEqual({ pos: 12, moved: true, stop: "edge" });
  });

  it("stops in front of a rock", () => {
    expect(L.slide(p, 4, "right")).toEqual({ pos: 5, moved: true, stop: "rock" });
    expect(L.slide(p, 2, "down")).toEqual({ pos: 2, moved: false, stop: "rock" });
    expect(L.slide(p, 14, "up")).toEqual({ pos: 10, moved: true, stop: "rock" });
  });

  it("does not move into an edge or a rock it already touches", () => {
    expect(L.slide(p, 0, "up")).toEqual({ pos: 0, moved: false, stop: "edge" });
    expect(L.slide(p, 0, "left")).toEqual({ pos: 0, moved: false, stop: "edge" });
    expect(L.slide(p, 5, "right")).toEqual({ pos: 5, moved: false, stop: "rock" });
  });

  it("the lantern stops the walker", () => {
    expect(L.slide(p, 12, "right")).toEqual({ pos: 15, moved: true, stop: "lantern" });
    const mid = pond(4, [], 0, 2);
    expect(L.slide(mid, 0, "right")).toEqual({ pos: 2, moved: true, stop: "lantern" });
  });

  it("never mutates the rocks", () => {
    const rocks = [6];
    L.slide(pond(4, rocks, 0, 15), 0, "right");
    expect(rocks).toEqual([6]);
  });
});

describe("Frost Steps search", () => {
  it("finds the exact fewest moves on a small pond", () => {
    // Open 3 by 3 with a rock at 4 (centre). Start 0, lantern 8. Slide right to 2, down to 8.
    const p = pond(3, [4], 0, 8);
    const s = L.solve(p);
    expect(s?.best).toBe(2);
    expect(walk(p, (s as { path: Dir[] }).path)).toBe(8);
  });

  it("reports an unreachable lantern", () => {
    // The lantern at 4 is boxed in: rocks at 1, 3, 5, 7 mean nothing can enter it.
    expect(L.solve(pond(3, [1, 3, 5, 7], 0, 4))).toBeNull();
  });

  it("is minimal: no shorter path exists (checked by brute force)", () => {
    for (let seed = 1; seed <= 12; seed++) {
      const p = L.generate(seed * 31, 6) as Puzzle;
      // Brute-force breadth over move sequences up to best - 1 must never reach the lantern.
      let layer = new Set<number>([p.start]);
      for (let depth = 1; depth < p.best; depth++) {
        const next = new Set<number>();
        for (const at of layer) {
          for (const d of L.DIRS) {
            const r = L.slide(p, at, d);
            if (r.moved) next.add(r.pos);
          }
        }
        expect(next.has(p.goal)).toBe(false);
        layer = next;
      }
    }
  });
});

describe("Frost Steps generation", () => {
  it("is deterministic per seed and differs between seeds", () => {
    expect(L.generate(2026, 7)).toEqual(L.generate(2026, 7));
    expect(L.generate(1, 7)).not.toEqual(L.generate(2, 7));
    expect(L.generate(555)).toEqual(L.generate(555));
  });

  it("today's seed is stable for a date, salted, and changes with the date", () => {
    const a = L.dateSeed(new Date(2026, 10, 6, 7));
    expect(L.dateSeed(new Date(2026, 10, 6, 23))).toBe(a);
    expect(L.dateSeed(new Date(2026, 10, 7))).not.toBe(a);
  });

  it("minimum lengths run from 6 to 12 moves with the size", () => {
    expect(L.SIZES).toEqual([6, 7, 8, 9]);
    expect(L.MIN_MOVES[6]).toBe(6);
    expect(L.MIN_MOVES[9]).toBe(12);
  });

  it("every puzzle is solvable, verified, and at least the minimum length for its size", () => {
    for (const size of L.SIZES) {
      for (let seed = 1; seed <= 60; seed++) {
        const p = L.generate(seed * 7919, size) as Puzzle;
        expect(p).not.toBeNull();
        expect(p.w).toBe(size);
        expect(p.h).toBe(size);
        expect(p.rocks).not.toContain(p.start);
        expect(p.rocks).not.toContain(p.goal);
        expect(p.start).not.toBe(p.goal);
        expect(new Set(p.rocks).size).toBe(p.rocks.length);
        const s = L.solve(p);
        expect(s).not.toBeNull();
        expect(s?.best).toBe(p.best);
        expect(p.best).toBeGreaterThanOrEqual(L.MIN_MOVES[size]!);
        // The reported path is a real solution under the slide rules.
        expect(walk(p, (s as { path: Dir[] }).path)).toBe(p.goal);
      }
    }
  });

  it("picks the size from the seed when none is given, covering every size", () => {
    const seen = new Set<number>();
    for (let seed = 0; seed < 400; seed++) {
      const p = L.generate(seed) as Puzzle;
      expect(p.w).toBe(L.sizeForSeed(seed));
      seen.add(p.w);
    }
    expect([...seen].sort()).toEqual([6, 7, 8, 9]);
  });
});

describe("Frost Steps saved puzzle", () => {
  it("survives a JSON round trip and recomputes the best", () => {
    const p = L.generate(31337, 8) as Puzzle;
    const back = L.validatePuzzle(JSON.parse(JSON.stringify(L.copyPuzzle(p))));
    expect(back).toEqual(p);
    // A forged "best" is ignored.
    const forged = { ...JSON.parse(JSON.stringify(p)), best: 1 };
    expect(L.validatePuzzle(forged)?.best).toBe(p.best);
  });

  it("rejects malformed or hostile puzzles", () => {
    const p = JSON.parse(JSON.stringify(L.generate(8, 6)));
    const bad = (mut: (q: Record<string, unknown>) => void) => {
      const q = JSON.parse(JSON.stringify(p)) as Record<string, unknown>;
      mut(q);
      return L.validatePuzzle(q);
    };
    expect(L.validatePuzzle(null)).toBeNull();
    expect(L.validatePuzzle("x")).toBeNull();
    expect(bad((q) => (q.w = 5))).toBeNull();
    expect(bad((q) => (q.h = 7))).toBeNull();
    expect(bad((q) => (q.w = 1e6))).toBeNull();
    expect(bad((q) => (q.rocks = [0, 0]))).toBeNull();
    expect(bad((q) => (q.rocks = [-1]))).toBeNull();
    expect(bad((q) => (q.rocks = [99]))).toBeNull();
    expect(bad((q) => (q.rocks = "none"))).toBeNull();
    expect(bad((q) => (q.rocks = Array.from({ length: 36 }, (_, i) => i)))).toBeNull();
    expect(bad((q) => (q.start = q.goal))).toBeNull();
    expect(bad((q) => (q.start = (q.rocks as number[])[0]))).toBeNull();
    expect(bad((q) => (q.goal = 1.5))).toBeNull();
    // A pond whose lantern cannot be reached is not playable.
    expect(
      L.validatePuzzle({ w: 6, h: 6, rocks: [1, 6, 8, 13], start: 0, goal: 7, best: 0 }),
    ).toBeNull();
  });
});
