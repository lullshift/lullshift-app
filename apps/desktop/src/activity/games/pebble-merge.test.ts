// Pebble Merge rules: seeded spawns, sliding, merging, full-tray detection and JSON round-trips.
/// <reference types="node" />
import { describe, expect, it } from "vitest";
import "../../../../../activities/games/pebble-merge/logic.js";

interface Game {
  board: number[];
  rng: number;
  moves: number;
  merges: number;
}
interface Logic {
  SIZE: number;
  CELLS: number;
  MAX_TIER: number;
  NAMES: string[];
  nextRandom(state: number): { value: number; state: number };
  emptyBoard(): number[];
  spawn(board: number[], rng: number): { board: number[]; rng: number; index: number };
  newGame(seed: number): Game;
  slide(
    board: number[],
    dir: number,
  ): { board: number[]; moved: boolean; merges: number; largest: number };
  move(game: Game, dir: number): (Game & { spawned: number; largestMade: number }) | null;
  canMove(board: number[]): boolean;
  largest(board: number[]): number;
  tierName(t: number): string;
  validBoard(v: unknown): boolean;
  validGame(v: unknown): boolean;
}
const L = (globalThis as unknown as { PebbleLogic: Logic }).PebbleLogic;
const UP = 0;
const RIGHT = 1;
const DOWN = 2;
const LEFT = 3;

const rows = (b: number[]) => [0, 1, 2, 3].map((r) => b.slice(r * 4, r * 4 + 4));

describe("PebbleLogic", () => {
  it("starts deterministically with two small pebbles", () => {
    expect(L.newGame(20260101)).toEqual(L.newGame(20260101));
    expect(L.newGame(1).board).not.toEqual(L.newGame(2).board);
    for (let seed = 1; seed <= 100; seed++) {
      const g = L.newGame(seed);
      const filled = g.board.filter((t) => t > 0);
      expect(filled).toHaveLength(2);
      expect(filled.every((t) => t === 1 || t === 2)).toBe(true);
    }
  });

  it("random numbers continue from a saved state", () => {
    const a = L.nextRandom(42);
    const b = L.nextRandom(a.state);
    const again = L.nextRandom(L.nextRandom(42).state);
    expect(again).toEqual(b);
    expect(a.value).toBeGreaterThanOrEqual(0);
    expect(a.value).toBeLessThan(1);
  });

  it("spawns only into empty cells, and nowhere on a full tray", () => {
    const b = L.emptyBoard().map((_, i) => (i === 5 ? 0 : 3));
    expect(L.spawn(b, 7).index).toBe(5);
    const full = L.emptyBoard().map(() => 3);
    const r = L.spawn(full, 7);
    expect(r.index).toBe(-1);
    expect(r.board).toEqual(full);
  });

  it("slides left, right, up and down", () => {
    const b = [0, 0, 0, 1, 0, 2, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0];
    expect(rows(L.slide(b, LEFT).board)).toEqual([
      [1, 0, 0, 0],
      [2, 0, 0, 0],
      [0, 0, 0, 0],
      [3, 0, 0, 0],
    ]);
    expect(rows(L.slide(b, RIGHT).board)).toEqual([
      [0, 0, 0, 1],
      [0, 0, 0, 2],
      [0, 0, 0, 0],
      [0, 0, 0, 3],
    ]);
    expect(rows(L.slide(b, UP).board)).toEqual([
      [3, 2, 0, 1],
      [0, 0, 0, 0],
      [0, 0, 0, 0],
      [0, 0, 0, 0],
    ]);
    expect(rows(L.slide(b, DOWN).board)).toEqual([
      [0, 0, 0, 0],
      [0, 0, 0, 0],
      [0, 0, 0, 0],
      [3, 2, 0, 1],
    ]);
  });

  it("merges two equal pebbles into the next size, once per slide", () => {
    const row = (r: number[]) => [...r, ...new Array(12).fill(0)];
    expect(L.slide(row([1, 1, 0, 0]), LEFT).board.slice(0, 4)).toEqual([2, 0, 0, 0]);
    expect(L.slide(row([1, 1, 1, 1]), LEFT).board.slice(0, 4)).toEqual([2, 2, 0, 0]);
    expect(L.slide(row([2, 1, 1, 0]), LEFT).board.slice(0, 4)).toEqual([2, 2, 0, 0]);
    expect(L.slide(row([1, 1, 1, 0]), LEFT).board.slice(0, 4)).toEqual([2, 1, 0, 0]);
    expect(L.slide(row([1, 1, 1, 0]), RIGHT).board.slice(0, 4)).toEqual([0, 0, 1, 2]);
    expect(L.slide(row([1, 2, 1, 2]), LEFT).moved).toBe(false);
    const r = L.slide(row([1, 1, 3, 3]), LEFT);
    expect(r.board.slice(0, 4)).toEqual([2, 4, 0, 0]);
    expect(r.merges).toBe(2);
    expect(r.largest).toBe(4);
  });

  it("does not merge past the largest size", () => {
    const row = [L.MAX_TIER, L.MAX_TIER, ...new Array(14).fill(0)];
    const r = L.slide(row, LEFT);
    expect(r.board.slice(0, 2)).toEqual([L.MAX_TIER, L.MAX_TIER]);
    expect(r.moved).toBe(false);
  });

  it("slide does not mutate its input", () => {
    const b = [1, 1, ...new Array(14).fill(0)];
    const copy = b.slice();
    L.slide(b, LEFT);
    expect(b).toEqual(copy);
  });

  it("a move that changes nothing is refused", () => {
    const g: Game = { board: [1, ...new Array(15).fill(0)], rng: 5, moves: 0, merges: 0 };
    expect(L.move(g, LEFT)).toBeNull();
    expect(L.move(g, UP)).toBeNull();
    expect(L.move(g, RIGHT)).not.toBeNull();
  });

  it("a move adds exactly one new pebble, counts, and is deterministic", () => {
    const g = L.newGame(3);
    const a = L.move(g, LEFT) ?? L.move(g, RIGHT)!;
    const b = L.move(g, LEFT) ?? L.move(g, RIGHT)!;
    expect(a).toEqual(b);
    expect(a.moves).toBe(1);
    expect(a.board.filter((t) => t > 0)).toHaveLength(3);
    expect(a.board[a.spawned]).toBeGreaterThan(0);
  });

  it("the same seed and the same key presses give the same tray", () => {
    const play = () => {
      let g = L.newGame(99);
      for (let i = 0; i < 200; i++) {
        g = L.move(g, i % 4) ?? L.move(g, (i + 1) % 4) ?? L.move(g, (i + 2) % 4) ?? g;
      }
      return g;
    };
    expect(play()).toEqual(play());
  });

  it("finds a full tray with no moves left, and a tray with a pair", () => {
    const stuck = [1, 2, 1, 2, 2, 1, 2, 1, 1, 2, 1, 2, 2, 1, 2, 1];
    expect(L.canMove(stuck)).toBe(false);
    expect(L.canMove(stuck.map((t, i) => (i === 1 ? 1 : t)))).toBe(true);
    expect(L.canMove(L.emptyBoard())).toBe(false);
  });

  it("random play keeps the tray valid and never loses count of pebbles' size sum", () => {
    let g = L.newGame(12345);
    for (let i = 0; i < 500 && L.canMove(g.board); i++) {
      const before = g.board.reduce((s, t) => s + (t ? 2 ** t : 0), 0);
      const next = L.move(g, (i * 7) % 4) ?? L.move(g, (i * 3 + 1) % 4);
      if (!next) continue;
      const after = next.board.reduce((s, t) => s + (t ? 2 ** t : 0), 0);
      // Merging keeps the total; the new pebble adds 2 or 4.
      expect([after - before]).toContainEqual(next.board[next.spawned] === 1 ? 2 : 4);
      expect(L.validBoard(next.board)).toBe(true);
      g = next;
    }
  });

  it("has a name for every size", () => {
    expect(L.NAMES[1]).toBe("Grain");
    expect(L.tierName(3)).toBe("Pebble");
    expect(L.tierName(99)).toBe("");
    expect(L.largest([0, 4, 2, ...new Array(13).fill(0)])).toBe(4);
  });

  it("validates saved games and survives a JSON round-trip", () => {
    let g = L.newGame(8);
    g = L.move(g, LEFT) ?? L.move(g, RIGHT)!;
    const back = JSON.parse(JSON.stringify(g));
    expect(back).toEqual(g);
    expect(L.validGame(back)).toBe(true);
    expect(L.validGame({ ...back, board: back.board.slice(1) })).toBe(false);
    expect(L.validGame({ ...back, rng: -1 })).toBe(false);
    expect(L.validGame({ ...back, moves: 1.5 })).toBe(false);
    expect(L.validGame({ ...back, board: back.board.map(() => 99) })).toBe(false);
    expect(L.validGame(null)).toBe(false);
  });
});
