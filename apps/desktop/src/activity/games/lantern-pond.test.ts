// Rule tests for Lantern Pond (activities/games/lantern-pond/logic.js).
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const root = fileURLToPath(new URL("../../../../../", import.meta.url));
new Function("window", readFileSync(join(root, "activities/games/lantern-pond/logic.js"), "utf8"))(
  globalThis,
);

interface Logic {
  SIZE: number;
  CELLS: number;
  solvedBoard(): boolean[];
  isSolved(board: boolean[]): boolean;
  press(board: boolean[], index: number): boolean[];
  generate(seed: number): { board: boolean[]; presses: number[] };
  pushMove(stack: number[], index: number): number[];
  undo(board: boolean[], stack: number[]): { board: boolean[]; stack: number[] };
  dateSeed(date?: Date): number;
}
const L = (globalThis as unknown as { LanternLogic: Logic }).LanternLogic;

const changed = (a: boolean[], b: boolean[]) =>
  b.map((v, i) => (v !== a[i] ? i : -1)).filter((i) => i >= 0);

describe("LanternLogic", () => {
  it("flips a cell and its in-bounds neighbours only", () => {
    const lit = L.solvedBoard();
    expect(changed(lit, L.press(lit, 12))).toEqual([7, 11, 12, 13, 17]);
    expect(changed(lit, L.press(lit, 0))).toEqual([0, 1, 5]);
  });

  it("does not wrap across row edges", () => {
    const lit = L.solvedBoard();
    expect(changed(lit, L.press(lit, 4))).toEqual([3, 4, 9]);
  });

  it("generates deterministic, solvable, unsolved ponds", () => {
    for (const seed of [1, 7, 20261106]) {
      const a = L.generate(seed);
      expect(L.generate(seed)).toEqual(a);
      expect(a.board).toHaveLength(L.CELLS);
      expect(L.isSolved(a.board)).toBe(false);
      // Replaying the generating presses solves it (a press is its own inverse).
      let b = a.board;
      for (const p of a.presses) b = L.press(b, p);
      expect(L.isSolved(b)).toBe(true);
    }
  });

  it("undo reverses the last press and keeps the rest of the stack", () => {
    const start = L.generate(5).board;
    let board = start;
    let stack: number[] = [];
    for (const i of [3, 14, 20]) {
      board = L.press(board, i);
      stack = L.pushMove(stack, i);
    }
    const r = L.undo(board, stack);
    expect(r.stack).toEqual([3, 14]);
    expect(r.board).toEqual(L.press(L.press(start, 3), 14));
    expect(L.undo(start, [])).toEqual({ board: start, stack: [] });
  });

  it("builds the daily seed from the local date", () => {
    expect(L.dateSeed(new Date(2026, 10, 6))).toBe(20261106);
  });
});

describe("Lantern Pond kit port", () => {
  it("loads the grid kit and keeps the saved-state field names", () => {
    const main = readFileSync(join(root, "activities/games/lantern-pond/main.js"), "utf8");
    const html = readFileSync(join(root, "activities/games/lantern-pond/index.html"), "utf8");
    expect(html).toContain("lullshift-grid-kit-v1.js");
    expect(main).toContain("bindGrid");
    for (const field of ["board", "history", "moves", "seed", "mode", "solved"]) {
      expect(main).toMatch(new RegExp(`\\b${field}:`));
    }
  });
});
