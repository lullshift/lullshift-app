// Rule tests for Stone Slide (activities/games/stone-slide/logic.js).
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// Load the kit and the logic as classic scripts, the way the activity page does.
const root = fileURLToPath(new URL("../../../../../", import.meta.url));
for (const file of [
  "packages/activity-kits/dist/lullshift-grid-kit-v1.js",
  "activities/games/stone-slide/logic.js",
]) {
  new Function("window", readFileSync(join(root, file), "utf8"))(globalThis);
}

interface Logic {
  SIZES: number[];
  solvedTiles(size: number): number[];
  isSolved(tiles: number[]): boolean;
  gapIndex(tiles: number[]): number;
  canSlide(tiles: number[], size: number, i: number): boolean;
  slide(tiles: number[], size: number, i: number): number[] | null;
  legalMoves(tiles: number[], size: number): number[];
  isSolvable(tiles: number[], size: number): boolean;
  generate(size: number, seed: number): number[];
  push(stack: number[], previousGap: number): number[];
  undo(tiles: number[], size: number, stack: number[]): { tiles: number[]; stack: number[] };
  validTiles(tiles: unknown, size: number): boolean;
  dailySeed(size: number, date?: Date): number;
}
const L = (globalThis as unknown as { StoneSlideLogic: Logic }).StoneSlideLogic;

describe("StoneSlideLogic", () => {
  it("builds the solved board with the gap last", () => {
    expect(L.solvedTiles(3)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 0]);
    expect(L.solvedTiles(4)).toHaveLength(16);
    expect(L.isSolved(L.solvedTiles(4))).toBe(true);
    expect(L.isSolved([2, 1, 3, 4, 5, 6, 7, 8, 0])).toBe(false);
  });

  it("generates deterministically per seed and size", () => {
    expect(L.generate(4, 123)).toEqual(L.generate(4, 123));
    expect(L.generate(3, 123)).toEqual(L.generate(3, 123));
    expect(L.generate(4, 1)).not.toEqual(L.generate(4, 2));
  });

  it("always generates solvable, unsolved, valid boards", () => {
    for (const size of L.SIZES) {
      for (let seed = 1; seed <= 200; seed++) {
        const t = L.generate(size, seed);
        expect(L.validTiles(t, size)).toBe(true);
        expect(L.isSolved(t)).toBe(false);
        expect(L.isSolvable(t, size)).toBe(true);
      }
    }
  });

  it("parity check rejects an unsolvable swap", () => {
    expect(L.isSolvable([2, 1, 3, 4, 5, 6, 7, 8, 0], 3)).toBe(false);
    expect(L.isSolvable(L.solvedTiles(4), 4)).toBe(true);
  });

  it("slides only tiles orthogonally beside the gap", () => {
    const t = L.solvedTiles(3); // gap at 8
    expect(L.legalMoves(t, 3)).toEqual([5, 7]);
    expect(L.canSlide(t, 3, 4)).toBe(false); // diagonal
    expect(L.canSlide(t, 3, 0)).toBe(false);
    expect(L.canSlide(t, 3, 8)).toBe(false); // the gap itself
    expect(L.slide(t, 3, 4)).toBeNull();
    expect(L.slide(t, 3, -1)).toBeNull();
    expect(L.slide(t, 3, 99)).toBeNull();
    expect(L.slide(t, 3, 7)).toEqual([1, 2, 3, 4, 5, 6, 7, 0, 8]);
    expect(L.slide(t, 3, 5)).toEqual([1, 2, 3, 4, 5, 0, 7, 8, 6]);
  });

  it("does not wrap across row ends", () => {
    const t = [1, 2, 0, 4, 5, 6, 7, 8, 3]; // gap at 2 (end of row 0)
    expect(L.canSlide(t, 3, 3)).toBe(false); // start of row 1 is not beside it
    expect(L.legalMoves(t, 3)).toEqual([1, 5]);
  });

  it("slide does not mutate its input", () => {
    const t = L.solvedTiles(3);
    const copy = t.slice();
    L.slide(t, 3, 7);
    expect(t).toEqual(copy);
  });

  it("detects a win after sliding back", () => {
    const t = L.solvedTiles(3);
    const moved = L.slide(t, 3, 7) as number[];
    expect(L.isSolved(moved)).toBe(false);
    expect(L.isSolved(L.slide(moved, 3, 8) as number[])).toBe(true);
  });

  it("undo reverses moves in order, back to the start", () => {
    const start = L.generate(4, 77);
    let tiles = start;
    let stack: number[] = [];
    for (let n = 0; n < 12; n++) {
      const moves = L.legalMoves(tiles, 4);
      const pick = moves[n % moves.length] as number;
      const gap = L.gapIndex(tiles);
      tiles = L.slide(tiles, 4, pick) as number[];
      stack = L.push(stack, gap);
    }
    expect(stack).toHaveLength(12);
    for (let n = 0; n < 12; n++) ({ tiles, stack } = L.undo(tiles, 4, stack));
    expect(tiles).toEqual(start);
    expect(stack).toEqual([]);
    expect(L.undo(tiles, 4, stack)).toEqual({ tiles: start, stack: [] });
  });

  it("validTiles rejects bad saved data", () => {
    expect(L.validTiles([1, 2, 3, 4, 5, 6, 7, 8, 0], 3)).toBe(true);
    expect(L.validTiles([1, 2, 3, 4, 5, 6, 7, 8], 3)).toBe(false);
    expect(L.validTiles([1, 1, 3, 4, 5, 6, 7, 8, 0], 3)).toBe(false);
    expect(L.validTiles([1, 2, 3, 4, 5, 6, 7, 8, 9], 3)).toBe(false);
    expect(L.validTiles("nope", 3)).toBe(false);
    expect(L.validTiles([1.5, 2, 3, 4, 5, 6, 7, 8, 0], 3)).toBe(false);
  });

  it("state snapshots survive a JSON round trip", () => {
    const start = L.generate(4, 5);
    const gap = L.gapIndex(start);
    const tiles = L.slide(start, 4, L.legalMoves(start, 4)[0] as number) as number[];
    const snap = { size: 4, seed: 5, start, tiles, stack: L.push([], gap) };
    const back = JSON.parse(JSON.stringify(snap)) as typeof snap;
    expect(back).toEqual(snap);
    expect(L.validTiles(back.tiles, 4)).toBe(true);
    expect(L.undo(back.tiles, 4, back.stack).tiles).toEqual(start);
  });

  it("daily seed is stable for a date and differs by size", () => {
    const d = new Date(2026, 10, 5);
    expect(L.dailySeed(4, d)).toBe(L.dailySeed(4, d));
    expect(L.dailySeed(4, d)).not.toBe(L.dailySeed(3, d));
    expect(L.dailySeed(4, d)).not.toBe(L.dailySeed(4, new Date(2026, 10, 6)));
  });
});
