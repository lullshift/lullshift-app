// Rule tests for Brook Links' pure logic.
/// <reference types="node" />
import { describe, expect, it } from "vitest";
import "../../../../../activities/games/brook-links/logic.js";

type Paths = number[][];
interface Puzzle {
  size: number;
  seed: number;
  stones: { a: number; b: number }[];
  solution: Paths;
}
type Dir = "up" | "right" | "down" | "left";
interface StepResult {
  ok: boolean;
  paths?: Paths;
  event?: string;
  reason?: string;
}
interface Logic {
  SIZES: number[];
  HISTORY_LIMIT: number;
  dateSeed(d: Date, size: number): number;
  isSize(n: number): boolean;
  generate(seed: number, size: number): Puzzle;
  blankPaths(p: Puzzle): Paths;
  owners(p: Puzzle, paths: Paths): number[];
  stoneColor(p: Puzzle, cell: number): number;
  isLinked(p: Puzzle, paths: Paths, c: number): boolean;
  linkedCount(p: Puzzle, paths: Paths): number;
  coveredCount(p: Puzzle, paths: Paths): number;
  isSolved(p: Puzzle, paths: Paths): boolean;
  isFullGarden(p: Puzzle, paths: Paths): boolean;
  startPath(p: Puzzle, paths: Paths, cell: number): { paths: Paths; color: number } | null;
  step(p: Puzzle, paths: Paths, c: number, dir: Dir): StepResult;
  retract(paths: Paths, c: number): { ok: boolean; paths: Paths };
  truncateTo(paths: Paths, c: number, cell: number): { ok: boolean; paths: Paths };
  samePaths(a: Paths, b: Paths): boolean;
  pushSnapshot(h: Paths[], paths: Paths): Paths[];
  undo(paths: Paths, h: Paths[]): { paths: Paths; history: Paths[] };
  validPaths(p: Puzzle, paths: unknown): boolean;
}
const L = (globalThis as unknown as { BrookLogic: Logic }).BrookLogic;

const dirBetween = (a: number, b: number, size: number): Dir =>
  b === a - size ? "up" : b === a + 1 ? "right" : b === a + size ? "down" : "left";

// Plays a whole stream by key steps, as the game does.
function drawStream(p: Puzzle, paths: Paths, c: number, cells: number[]): Paths {
  let cur = L.startPath(p, paths, cells[0]!)!.paths;
  for (let i = 1; i < cells.length; i++) {
    const r = L.step(p, cur, c, dirBetween(cells[i - 1]!, cells[i]!, p.size));
    expect(r.ok).toBe(true);
    cur = r.paths!;
  }
  return cur;
}

describe("BrookLogic", () => {
  it("offers sizes 5 to 7", () => {
    expect(L.SIZES).toEqual([5, 6, 7]);
    expect(() => L.generate(1, 4)).toThrow();
  });

  it("generates deterministically per seed and size", () => {
    for (const size of L.SIZES) {
      expect(L.generate(20260101, size)).toEqual(L.generate(20260101, size));
      expect(L.generate(1, size)).not.toEqual(L.generate(2, size));
    }
  });

  it("dateSeed is stable per local date and differs by size and day", () => {
    const d = new Date(2026, 10, 5);
    expect(L.dateSeed(d, 6)).toBe(L.dateSeed(new Date(2026, 10, 5, 20, 0), 6));
    expect(L.dateSeed(d, 6)).not.toBe(L.dateSeed(d, 7));
    expect(L.dateSeed(d, 6)).not.toBe(L.dateSeed(new Date(2026, 10, 6), 6));
  });

  it("every generated puzzle has non-crossing streams that fill the grid", () => {
    for (const size of L.SIZES) {
      for (let seed = 1; seed <= 80; seed++) {
        const p = L.generate(seed, size);
        const all = p.solution.flat();
        expect(all).toHaveLength(size * size);
        expect(new Set(all).size).toBe(size * size);
        expect(p.stones.length).toBeGreaterThanOrEqual(4);
        expect(p.stones.length).toBeLessThanOrEqual(8);
        p.solution.forEach((run, c) => {
          expect(run.length).toBeGreaterThanOrEqual(3);
          for (let i = 1; i < run.length; i++) {
            const d = Math.abs(run[i]! - run[i - 1]!);
            expect(
              d === size ||
                (d === 1 && Math.floor(run[i]! / size) === Math.floor(run[i - 1]! / size)),
            ).toBe(true);
          }
          expect(p.stones[c]).toEqual({ a: run[0], b: run[run.length - 1] });
        });
        // Only the endpoints are given: the solution is a valid win and covers everything.
        expect(L.isSolved(p, p.solution)).toBe(true);
        expect(L.isFullGarden(p, p.solution)).toBe(true);
      }
    }
  });

  it("drawing the solution with key steps links every pair and wins", () => {
    for (let seed = 1; seed <= 20; seed++) {
      const p = L.generate(seed, 6);
      let paths = L.blankPaths(p);
      p.solution.forEach((run, c) => {
        paths = drawStream(p, paths, c, run);
        expect(L.isLinked(p, paths, c)).toBe(true);
      });
      expect(L.linkedCount(p, paths)).toBe(p.stones.length);
      expect(L.isSolved(p, paths)).toBe(true);
      expect(L.coveredCount(p, paths)).toBe(36);
    }
  });

  it("the win check accepts a valid answer that does not cover the grid", () => {
    // Hand-made puzzle: two pairs on a 5 by 5 board with short direct streams.
    const p: Puzzle = {
      size: 5,
      seed: 0,
      stones: [
        { a: 0, b: 2 },
        { a: 10, b: 12 },
      ],
      solution: [[], []],
    };
    const paths: Paths = [
      [0, 1, 2],
      [10, 11, 12],
    ];
    expect(L.isSolved(p, paths)).toBe(true);
    expect(L.isFullGarden(p, paths)).toBe(false);
    expect(L.coveredCount(p, paths)).toBe(6);
    // The same pairs joined by a longer detour are also fine.
    expect(
      L.isSolved(p, [
        [0, 5, 6, 7, 2],
        [10, 11, 12],
      ]),
    ).toBe(true);
    expect(
      L.isSolved(p, [
        [0, 1, 2],
        [10, 15, 16, 17, 12],
      ]),
    ).toBe(true);
  });

  it("rejects crossing, branching, gaps, stolen stones and wrong ends", () => {
    const p: Puzzle = {
      size: 5,
      seed: 0,
      stones: [
        { a: 0, b: 4 },
        { a: 2, b: 22 },
      ],
      solution: [[], []],
    };
    // Stream 0 along the top passes through stone 2 of the other colour.
    expect(
      L.isSolved(p, [
        [0, 1, 2, 3, 4],
        [2, 7, 12, 17, 22],
      ]),
    ).toBe(false);
    // Shared interior cell.
    expect(
      L.isSolved(p, [
        [0, 5, 6, 7, 8, 9, 4],
        [2, 7, 12, 17, 22],
      ]),
    ).toBe(false);
    // Gap in a stream.
    expect(
      L.isSolved(p, [
        [0, 1, 3, 4],
        [2, 7, 12, 17, 22],
      ]),
    ).toBe(false);
    // Wrong start stone.
    expect(
      L.isSolved(p, [
        [1, 2, 3],
        [2, 7, 12, 17, 22],
      ]),
    ).toBe(false);
    // Unlinked.
    expect(
      L.isSolved(p, [
        [0, 1],
        [2, 7, 12, 17, 22],
      ]),
    ).toBe(false);
    // Revisiting a cell.
    expect(
      L.isSolved(p, [
        [0, 5, 6, 1, 0, 5, 6, 7, 8, 9, 4],
        [2, 7, 12, 17, 22],
      ]),
    ).toBe(false);
  });

  const small = (): Puzzle => ({
    size: 5,
    seed: 0,
    stones: [
      { a: 0, b: 2 },
      { a: 1, b: 21 },
    ],
    solution: [[], []],
  });

  it("start needs a stone; a stone of the other colour restarts that colour only", () => {
    const p = small();
    expect(L.startPath(p, L.blankPaths(p), 7)).toBeNull();
    const r = L.startPath(p, L.blankPaths(p), 0)!;
    expect(r.color).toBe(0);
    expect(r.paths).toEqual([[0], []]);
    // Pressing the stream's start again keeps it.
    const s = L.step(p, r.paths, 0, "down");
    expect(L.startPath(p, s.paths!, 0)!.paths[0]).toEqual([0, 5]);
    // Pressing its head keeps it too (to retract a linked stream), the partner stone otherwise restarts.
    expect(L.startPath(p, s.paths!, 2)!.paths[0]).toEqual([2]);
  });

  it("streams cannot cross, branch or enter other stones", () => {
    const p = small();
    let paths = L.blankPaths(p);
    paths = L.startPath(p, paths, 1)!.paths; // colour 1 at (1,0)
    // Step right from 0 would enter stone 1 of another colour.
    const a = L.startPath(p, paths, 0)!.paths;
    const blocked = L.step(p, a, 0, "right");
    expect(blocked).toMatchObject({ ok: false, reason: "another stream" });
    // Edge.
    expect(L.step(p, a, 0, "up")).toMatchObject({ ok: false, reason: "edge" });
    expect(L.step(p, a, 0, "left")).toMatchObject({ ok: false, reason: "edge" });
    // Build colour 1 down column 1; colour 0 may not cross it.
    let q = L.startPath(p, L.blankPaths(p), 1)!.paths;
    for (let i = 0; i < 3; i++) q = L.step(p, q, 1, "down").paths!; // 6, 11, 16
    q = L.startPath(p, q, 0)!.paths;
    q = L.step(p, q, 0, "down").paths!; // 5
    q = L.step(p, q, 0, "down").paths!; // 10
    expect(L.owners(p, q)[11]).toBe(1);
    expect(L.step(p, q, 0, "right")).toMatchObject({ ok: false, reason: "another stream" });
  });

  it("a stream cannot step onto itself, but stepping back retracts", () => {
    const p = small();
    let q = L.startPath(p, L.blankPaths(p), 0)!.paths;
    q = L.step(p, q, 0, "down").paths!; // 5
    q = L.step(p, q, 0, "right").paths!; // 6
    expect(L.step(p, q, 0, "up")).toMatchObject({ ok: false, reason: "another stream" }); // 1
    expect(q[0]).toEqual([0, 5, 6]);
    const loop = L.step(p, q, 0, "left"); // back to 5: retract
    expect(loop).toMatchObject({ ok: true, event: "retracted" });
    expect(loop.paths![0]).toEqual([0, 5]);
    // Own-stream collision: 0,5,6,11,10,5 would revisit 5.
    let r = L.startPath(p, L.blankPaths(p), 0)!.paths;
    for (const d of ["down", "right", "down", "left"] as Dir[]) r = L.step(p, r, 0, d).paths!;
    expect(r[0]).toEqual([0, 5, 6, 11, 10]);
    expect(L.step(p, r, 0, "up")).toMatchObject({ ok: false, reason: "own stream" });
  });

  it("reaching the partner stone links; a linked stream only retracts", () => {
    const p = small();
    let q = L.startPath(p, L.blankPaths(p), 0)!.paths;
    q = L.step(p, q, 0, "down").paths!; // 5
    q = L.step(p, q, 0, "right").paths!; // 6
    q = L.step(p, q, 0, "right").paths!; // 7
    const last = L.step(p, q, 0, "up"); // 2
    expect(last).toMatchObject({ ok: true, event: "linked" });
    expect(L.isLinked(p, last.paths!, 0)).toBe(true);
    expect(L.step(p, last.paths!, 0, "up")).toMatchObject({ ok: false, reason: "edge" });
    expect(L.step(p, last.paths!, 0, "left")).toMatchObject({
      ok: false,
      reason: "already linked",
    });
    expect(L.step(p, last.paths!, 0, "down")).toMatchObject({ event: "retracted" });
  });

  it("retract and truncate", () => {
    const p = small();
    let q = L.startPath(p, L.blankPaths(p), 0)!.paths;
    q = L.step(p, q, 0, "down").paths!;
    q = L.step(p, q, 0, "down").paths!; // 0, 5, 10
    expect(L.retract(q, 0).paths[0]).toEqual([0, 5]);
    expect(L.retract(L.retract(L.retract(q, 0).paths, 0).paths, 0).paths[0]).toEqual([]);
    expect(L.retract(L.blankPaths(p), 0).ok).toBe(false);
    expect(L.truncateTo(q, 0, 5).paths[0]).toEqual([0, 5]);
    expect(L.truncateTo(q, 0, 17).ok).toBe(false);
  });

  it("undo restores whole strokes and does not mutate snapshots", () => {
    const p = L.generate(5, 5);
    let paths = L.blankPaths(p);
    let h: Paths[] = [];
    h = L.pushSnapshot(h, paths);
    paths = drawStream(p, paths, 0, p.solution[0]!);
    h = L.pushSnapshot(h, paths);
    paths = drawStream(p, paths, 1, p.solution[1]!);
    expect(L.linkedCount(p, paths)).toBe(2);
    let r = L.undo(paths, h);
    expect(L.linkedCount(p, r.paths)).toBe(1);
    r = L.undo(r.paths, r.history);
    expect(L.linkedCount(p, r.paths)).toBe(0);
    expect(L.undo(r.paths, r.history).history).toEqual([]);
    expect(L.samePaths(r.paths, L.blankPaths(p))).toBe(true);
  });

  it("history is capped", () => {
    const p = L.generate(1, 5);
    let h: Paths[] = [];
    for (let i = 0; i < L.HISTORY_LIMIT + 20; i++) h = L.pushSnapshot(h, L.blankPaths(p));
    expect(h).toHaveLength(L.HISTORY_LIMIT);
  });

  it("validPaths accepts real play and refuses hostile saves", () => {
    const p = L.generate(9, 6);
    expect(L.validPaths(p, L.blankPaths(p))).toBe(true);
    expect(L.validPaths(p, p.solution)).toBe(true);
    expect(L.validPaths(p, null)).toBe(false);
    expect(L.validPaths(p, [])).toBe(false);
    expect(
      L.validPaths(
        p,
        p.solution.map(() => [999]),
      ),
    ).toBe(false);
    expect(
      L.validPaths(
        p,
        p.solution.map(() => ["a"]),
      ),
    ).toBe(false);
    const dup = L.blankPaths(p);
    dup[0] = [p.stones[0]!.a];
    dup[1] = [p.stones[0]!.a];
    expect(L.validPaths(p, dup)).toBe(false);
    const jump = L.blankPaths(p);
    jump[0] = [p.stones[0]!.a, p.stones[0]!.a === 35 ? 0 : 35];
    expect(L.validPaths(p, jump)).toBe(false);
  });

  it("a play state survives a JSON round trip", () => {
    const p = L.generate(13, 7);
    let paths = L.blankPaths(p);
    paths = drawStream(p, paths, 0, p.solution[0]!);
    const half = L.startPath(p, paths, p.stones[1]!.a)!.paths;
    const saved = JSON.parse(
      JSON.stringify({ v: 1, seed: p.seed, size: 7, paths: half, history: [L.blankPaths(p)] }),
    );
    expect(saved.paths).toEqual(half);
    expect(L.validPaths(L.generate(saved.seed, saved.size), saved.paths)).toBe(true);
    expect(L.generate(saved.seed, saved.size)).toEqual(p);
  });
});
