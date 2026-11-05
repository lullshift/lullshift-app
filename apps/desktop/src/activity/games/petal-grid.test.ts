// Rule tests for Petal Grid (activities/games/petal-grid/logic.js).
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// Load the kit and the logic as classic scripts, the way the activity page does.
const root = fileURLToPath(new URL("../../../../../", import.meta.url));
for (const file of [
  "packages/activity-kits/dist/lullshift-grid-kit-v1.js",
  "activities/games/petal-grid/logic.js",
]) {
  new Function("window", readFileSync(join(root, file), "utf8"))(globalThis);
}

interface Puzzle {
  size: number;
  seed: number;
  solution: number[];
  givens: number[];
}
interface Logic {
  SIZES: number[];
  MIN_GIVENS: Record<number, number>;
  petalName(kind: number): string;
  countSolutions(size: number, cells: number[], limit: number): number;
  generate(size: number, seed: number): Puzzle;
  conflicts(size: number, cells: number[]): number[];
  filledCount(cells: number[]): number;
  isSolved(size: number, cells: number[]): boolean;
  place(p: Puzzle, cells: number[], i: number, value: number): number[] | null;
  push(
    stack: { i: number; prev: number }[],
    i: number,
    prev: number,
  ): { i: number; prev: number }[];
  undo(
    cells: number[],
    stack: { i: number; prev: number }[],
  ): { cells: number[]; stack: { i: number; prev: number }[] };
  startCells(p: Puzzle): number[];
  validPuzzle(p: unknown): boolean;
  validCells(p: Puzzle, cells: unknown): boolean;
  dailySeed(size: number, date?: Date): number;
}
const L = (globalThis as unknown as { PetalGridLogic: Logic }).PetalGridLogic;

function isLatin(size: number, cells: number[]): boolean {
  for (let a = 0; a < size; a++) {
    const row = new Set<number>();
    const col = new Set<number>();
    for (let b = 0; b < size; b++) {
      row.add(cells[a * size + b] as number);
      col.add(cells[b * size + a] as number);
    }
    if (row.size !== size || col.size !== size) return false;
  }
  return cells.every((v) => v >= 1 && v <= size);
}

describe("PetalGridLogic", () => {
  it("generates deterministically per seed and size", () => {
    expect(L.generate(4, 99)).toEqual(L.generate(4, 99));
    expect(L.generate(6, 99)).toEqual(L.generate(6, 99));
    expect(L.generate(4, 1).solution).not.toEqual(L.generate(4, 2).solution);
  });

  it("solutions are Latin squares and givens come from them", () => {
    for (const size of L.SIZES) {
      for (let seed = 1; seed <= 25; seed++) {
        const p = L.generate(size, seed);
        expect(isLatin(size, p.solution)).toBe(true);
        expect(L.validPuzzle(p)).toBe(true);
        p.givens.forEach((g, i) => {
          if (g !== 0) expect(g).toBe(p.solution[i]);
        });
        expect(L.filledCount(p.givens)).toBeGreaterThanOrEqual(L.MIN_GIVENS[size] as number);
        expect(L.filledCount(p.givens)).toBeLessThan(size * size);
      }
    }
  });

  it("every puzzle has exactly one solution (the stored one)", () => {
    for (const size of L.SIZES) {
      for (let seed = 1; seed <= 25; seed++) {
        const p = L.generate(size, seed);
        expect(L.countSolutions(size, p.givens, 5)).toBe(1);
      }
    }
  });

  it("the solver counts several solutions when the puzzle is open", () => {
    expect(L.countSolutions(4, new Array(16).fill(0), 3)).toBe(3);
    // Conflicting givens have no solution.
    expect(L.countSolutions(4, [1, 1, ...new Array(14).fill(0)], 2)).toBe(0);
  });

  it("finds conflicts in rows and columns only", () => {
    const e = new Array(16).fill(0);
    expect(L.conflicts(4, e)).toEqual([]);
    const row = e.slice();
    row[0] = 2;
    row[3] = 2;
    expect(L.conflicts(4, row)).toEqual([0, 3]);
    const col = e.slice();
    col[1] = 3;
    col[9] = 3;
    expect(L.conflicts(4, col)).toEqual([1, 9]);
    const diag = e.slice();
    diag[0] = 4;
    diag[5] = 4;
    expect(L.conflicts(4, diag)).toEqual([]);
  });

  it("place respects givens, ranges and no-ops, and does not mutate", () => {
    const p = L.generate(4, 3);
    const cells = L.startCells(p);
    const given = p.givens.findIndex((g) => g !== 0);
    const free = p.givens.findIndex((g) => g === 0);
    expect(L.place(p, cells, given, 1)).toBeNull();
    expect(L.place(p, cells, free, 5)).toBeNull();
    expect(L.place(p, cells, free, -1)).toBeNull();
    expect(L.place(p, cells, free, 0)).toBeNull(); // already empty
    expect(L.place(p, cells, 99, 1)).toBeNull();
    const next = L.place(p, cells, free, 2) as number[];
    expect(next[free]).toBe(2);
    expect(cells[free]).toBe(0);
    expect((L.place(p, next, free, 0) as number[])[free]).toBe(0);
  });

  it("win detection: solving from the givens completes, a wrong fill does not", () => {
    for (const size of L.SIZES) {
      const p = L.generate(size, 11);
      expect(L.isSolved(size, L.startCells(p))).toBe(false);
      expect(L.isSolved(size, p.solution)).toBe(true);
      const wrong = p.solution.slice();
      const free = p.givens.findIndex((g) => g === 0);
      wrong[free] = ((wrong[free] as number) % size) + 1;
      expect(L.isSolved(size, wrong)).toBe(false);
    }
  });

  it("filling blanks one by one with the solution reaches a win", () => {
    const p = L.generate(6, 8);
    let cells = L.startCells(p);
    p.givens.forEach((g, i) => {
      if (g === 0) cells = L.place(p, cells, i, p.solution[i] as number) as number[];
    });
    expect(L.isSolved(6, cells)).toBe(true);
  });

  it("undo restores previous values in order", () => {
    const p = L.generate(4, 21);
    const base = L.startCells(p);
    const [a, b] = p.givens.flatMap((g, i) => (g === 0 ? [i] : [])).slice(0, 2) as [number, number];
    let cells = base;
    let stack: { i: number; prev: number }[] = [];
    cells = L.place(p, cells, a, 1) as number[];
    stack = L.push(stack, a, 0);
    cells = L.place(p, cells, b, 2) as number[];
    stack = L.push(stack, b, 0);
    cells = L.place(p, cells, a, 3) as number[];
    stack = L.push(stack, a, 1);
    let r = L.undo(cells, stack);
    expect(r.cells[a]).toBe(1);
    r = L.undo(r.cells, r.stack);
    expect(r.cells[b]).toBe(0);
    r = L.undo(r.cells, r.stack);
    expect(r.cells).toEqual(base);
    expect(L.undo(r.cells, r.stack)).toEqual({ cells: base, stack: [] });
  });

  it("names the petals", () => {
    expect([1, 2, 3, 4, 5, 6].map(L.petalName)).toEqual([
      "rose",
      "iris",
      "tulip",
      "daisy",
      "lily",
      "violet",
    ]);
  });

  it("state snapshots survive a JSON round trip and bad data is rejected", () => {
    const p = L.generate(6, 4);
    const free = p.givens.findIndex((g) => g === 0);
    const cells = L.place(p, L.startCells(p), free, 2) as number[];
    const snap = { puzzle: p, cells, stack: L.push([], free, 0) };
    const back = JSON.parse(JSON.stringify(snap)) as typeof snap;
    expect(back).toEqual(snap);
    expect(L.validPuzzle(back.puzzle)).toBe(true);
    expect(L.validCells(back.puzzle, back.cells)).toBe(true);

    const given = p.givens.findIndex((g) => g !== 0);
    const tampered = cells.slice();
    tampered[given] = ((tampered[given] as number) % 6) + 1;
    expect(L.validCells(p, tampered)).toBe(false);
    expect(L.validCells(p, cells.slice(1))).toBe(false);
    expect(L.validPuzzle({ ...p, size: 5 })).toBe(false);
    expect(L.validPuzzle({ ...p, solution: p.solution.map(() => 1) })).toBe(false);
    expect(L.validPuzzle(null)).toBe(false);
  });

  it("daily seed is stable for a date and differs by size", () => {
    const d = new Date(2026, 10, 5);
    expect(L.dailySeed(4, d)).toBe(L.dailySeed(4, d));
    expect(L.dailySeed(4, d)).not.toBe(L.dailySeed(6, d));
  });
});
