// Rule tests for Thread Picture's pure logic.
/// <reference types="node" />
import { describe, expect, it } from "vitest";
import "../../../../../activities/games/thread-picture/logic.js";

type Cells = number[];
interface Puzzle {
  size: number;
  seed: number;
  picture: number[];
  rows: number[][];
  cols: number[][];
  total: number;
  attempts: number;
}
interface Logic {
  SIZES: number[];
  HISTORY_LIMIT: number;
  dateSeed(d: Date, size: number): number;
  isSize(n: number): boolean;
  runsOf(line: number[]): number[];
  cluesOf(picture: number[], size: number): { rows: number[][]; cols: number[][] };
  solveLine(clue: number[], line: number[]): number[] | null;
  lineSolve(
    size: number,
    rows: number[][],
    cols: number[][],
  ): { cells: Cells; solved: boolean; contradiction: boolean };
  generate(seed: number, size: number): Puzzle;
  blankCells(size: number): Cells;
  toggleFill(cells: Cells, i: number): Cells;
  toggleMark(cells: Cells, i: number): Cells;
  isSolved(cells: Cells, size: number, rows: number[][], cols: number[][]): boolean;
  filledCount(cells: Cells): number;
  pushMove(
    h: { i: number; from: number; to: number }[],
    i: number,
    from: number,
    to: number,
  ): { i: number; from: number; to: number }[];
  undo(
    cells: Cells,
    h: { i: number; from: number; to: number }[],
  ): { cells: Cells; history: { i: number; from: number; to: number }[] };
}
const L = (globalThis as unknown as { ThreadLogic: Logic }).ThreadLogic;

// Counts pictures that fit the clues by trying every row pattern, then checking the columns.
function countSolutions(p: Puzzle): number {
  const n = p.size;
  const patterns: number[][][] = p.rows.map((clue) => {
    const out: number[][] = [];
    for (let m = 0; m < 1 << n; m++) {
      const line = Array.from({ length: n }, (_, i) => (m >> i) & 1);
      const runs = L.runsOf(line);
      const want = clue.length === 1 && clue[0] === 0 ? [] : clue;
      if (runs.length === want.length && runs.every((r, i) => r === want[i])) out.push(line);
    }
    return out;
  });
  let count = 0;
  const picked: number[][] = [];
  const colRuns = p.cols.map((c) => (c.length === 1 && c[0] === 0 ? [] : c));
  const rec = (r: number) => {
    if (r === n) {
      for (let x = 0; x < n; x++) {
        const runs = L.runsOf(picked.map((row) => row[x]!));
        if (runs.length !== colRuns[x]!.length || runs.some((v, i) => v !== colRuns[x]![i])) return;
      }
      count += 1;
      return;
    }
    for (const line of patterns[r]!) {
      picked.push(line);
      rec(r + 1);
      picked.pop();
    }
  };
  rec(0);
  return count;
}

describe("ThreadLogic", () => {
  it("offers 5 and 10", () => {
    expect(L.SIZES).toEqual([5, 10]);
    expect(L.isSize(5)).toBe(true);
    expect(L.isSize(7)).toBe(false);
    expect(() => L.generate(1, 7)).toThrow();
  });

  it("generates deterministically per seed and size", () => {
    for (const size of L.SIZES) {
      expect(L.generate(20260101, size)).toEqual(L.generate(20260101, size));
      expect(L.generate(1, size).picture).not.toEqual(L.generate(2, size).picture);
    }
  });

  it("dateSeed is stable per local date, differs by size and by day", () => {
    const d = new Date(2026, 10, 5);
    expect(L.dateSeed(d, 5)).toBe(L.dateSeed(new Date(2026, 10, 5, 23, 59), 5));
    expect(L.dateSeed(d, 5)).not.toBe(L.dateSeed(d, 10));
    expect(L.dateSeed(d, 5)).not.toBe(L.dateSeed(new Date(2026, 10, 6), 5));
  });

  it("clues describe the picture's runs", () => {
    const clues = L.cluesOf(
      [1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1, 0, 1, 1],
      5,
    );
    expect(clues.rows).toEqual([[2, 1], [0], [5], [0], [1, 2]]);
    expect(clues.cols).toEqual([[1, 1], [1, 1, 1], [1], [1, 1, 1], [1, 1]]);
  });

  it("line solver decides overlaps, edges and contradictions", () => {
    expect(L.solveLine([5], [0, 0, 0, 0, 0])).toEqual([1, 1, 1, 1, 1]);
    expect(L.solveLine([3], [0, 0, 0, 0, 0])).toEqual([0, 0, 1, 0, 0]);
    expect(L.solveLine([4, 1], [0, 0, 0, 0, 0, 0])).toEqual([1, 1, 1, 1, 2, 1]);
    expect(L.solveLine([0], [0, 0, 0])).toEqual([2, 2, 2]);
    expect(L.solveLine([2], [1, 0, 0, 0])).toEqual([1, 1, 2, 2]);
    expect(L.solveLine([2, 2], [0, 0, 0])).toBeNull();
    expect(L.solveLine([2], [2, 2, 2])).toBeNull();
  });

  it("generated pictures are fully solved by the line solver and match their clues", () => {
    for (const size of L.SIZES) {
      for (let seed = 1; seed <= 60; seed++) {
        const p = L.generate(seed, size);
        expect(p.picture).toHaveLength(size * size);
        const r = L.lineSolve(size, p.rows, p.cols);
        expect(r.solved).toBe(true);
        expect(r.cells.map((v) => (v === 1 ? 1 : 0))).toEqual(p.picture);
        expect(L.cluesOf(p.picture, size)).toEqual({ rows: p.rows, cols: p.cols });
        expect(L.filledCount(p.picture)).toBe(p.total);
        expect(L.filledCount(p.picture.map((v) => (v ? 1 : 0)))).toBeGreaterThan(0);
      }
    }
  });

  it("5 by 5 puzzles have exactly one solution (brute force)", () => {
    for (let seed = 1; seed <= 12; seed++) {
      expect(countSolutions(L.generate(seed, 5))).toBe(1);
    }
  });

  it("fill, mark and clear cells without mutating the input", () => {
    const b = L.blankCells(5);
    const f = L.toggleFill(b, 3);
    expect(b[3]).toBe(0);
    expect(f[3]).toBe(1);
    expect(L.toggleFill(f, 3)[3]).toBe(0);
    const m = L.toggleMark(b, 3);
    expect(m[3]).toBe(2);
    expect(L.toggleMark(m, 3)[3]).toBe(0);
    expect(L.toggleFill(m, 3)[3]).toBe(1);
    expect(L.toggleMark(f, 3)[3]).toBe(2);
  });

  it("detects a win, ignoring marks, and rejects near misses", () => {
    const p = L.generate(7, 5);
    let cells = L.blankCells(5);
    expect(L.isSolved(cells, 5, p.rows, p.cols)).toBe(false);
    cells = p.picture.map((v) => (v ? 1 : 2));
    expect(L.isSolved(cells, 5, p.rows, p.cols)).toBe(true);
    cells = p.picture.map((v) => (v ? 1 : 0));
    expect(L.isSolved(cells, 5, p.rows, p.cols)).toBe(true);
    const i = p.picture.indexOf(1);
    const broken = cells.slice();
    broken[i] = 0;
    expect(L.isSolved(broken, 5, p.rows, p.cols)).toBe(false);
    const extra = cells.slice();
    extra[p.picture.indexOf(0)] = 1;
    expect(L.isSolved(extra, 5, p.rows, p.cols)).toBe(false);
  });

  it("undo reverses moves in order and stops at the start", () => {
    let cells = L.blankCells(5);
    let h: ReturnType<Logic["pushMove"]> = [];
    const play = (i: number, mark: boolean) => {
      const next = mark ? L.toggleMark(cells, i) : L.toggleFill(cells, i);
      h = L.pushMove(h, i, cells[i]!, next[i]!);
      cells = next;
    };
    play(2, false);
    play(7, true);
    play(2, true);
    expect(cells[2]).toBe(2);
    let r = L.undo(cells, h);
    expect(r.cells[2]).toBe(1);
    r = L.undo(r.cells, r.history);
    expect(r.cells[7]).toBe(0);
    r = L.undo(r.cells, r.history);
    expect(r.cells).toEqual(L.blankCells(5));
    expect(L.undo(r.cells, r.history)).toEqual({ cells: L.blankCells(5), history: [] });
  });

  it("history is capped", () => {
    let h: ReturnType<Logic["pushMove"]> = [];
    for (let k = 0; k < L.HISTORY_LIMIT + 50; k++) h = L.pushMove(h, k % 25, 0, 1);
    expect(h).toHaveLength(L.HISTORY_LIMIT);
  });

  it("a play state survives a JSON round trip", () => {
    const p = L.generate(11, 10);
    let cells = L.blankCells(10);
    let h: ReturnType<Logic["pushMove"]> = [];
    for (const i of [3, 14, 55]) {
      const next = L.toggleFill(cells, i);
      h = L.pushMove(h, i, cells[i]!, next[i]!);
      cells = next;
    }
    const saved = JSON.parse(JSON.stringify({ v: 1, seed: p.seed, size: 10, cells, history: h }));
    expect(saved.cells).toEqual(cells);
    expect(saved.history).toEqual(h);
    expect(L.generate(saved.seed, saved.size)).toEqual(p);
  });
});
