// Rule tests for Moss Paths (S06-T04/T05).
/// <reference types="node" />
import { describe, expect, it } from "vitest";
import "../../../../../activities/games/moss-paths/logic.js";

interface Garden {
  size: number;
  src: number;
  solution: number[];
  cells: number[];
}
interface Move {
  i: number;
  cw: boolean;
}
interface Logic {
  SIZE: number;
  N: number;
  E: number;
  S: number;
  W: number;
  HISTORY_LIMIT: number;
  rotate(mask: number, cw?: boolean): number;
  pieceType(mask: number): string;
  openings(mask: number): string[];
  canRotate(mask: number): boolean;
  isMask(v: unknown): boolean;
  dateSeed(d: Date, size?: number): number;
  SIZES: number[];
  isSize(n: number): boolean;
  watered(cells: number[], size: number, src: number): boolean[];
  wateredCount(cells: number[], size: number, src: number): number;
  isSolved(cells: number[], size: number, src: number): boolean;
  generate(seed: number, size?: number): Garden;
  turn(cells: number[], index: number, cw?: boolean): number[];
  pushMove(history: Move[], index: number, cw?: boolean): Move[];
  undo(cells: number[], history: Move[]): { cells: number[]; history: Move[] };
}
const L = (globalThis as unknown as { MossLogic: Logic }).MossLogic;

describe("MossLogic", () => {
  it("rotates masks and round-trips", () => {
    expect(L.rotate(L.N)).toBe(L.E);
    expect(L.rotate(L.W)).toBe(L.N);
    expect(L.rotate(L.N, false)).toBe(L.W);
    expect(L.rotate(L.N | L.E)).toBe(L.E | L.S);
    for (let m = 0; m < 16; m++) {
      expect(L.rotate(L.rotate(m, true), false)).toBe(m);
      let r = m;
      for (let k = 0; k < 4; k++) r = L.rotate(r);
      expect(r).toBe(m);
    }
  });

  it("names every piece", () => {
    expect(L.pieceType(L.N)).toBe("end");
    expect(L.pieceType(L.N | L.S)).toBe("straight");
    expect(L.pieceType(L.E | L.W)).toBe("straight");
    expect(L.pieceType(L.N | L.E)).toBe("bend");
    expect(L.pieceType(L.N | L.E | L.S)).toBe("tee");
    expect(L.pieceType(15)).toBe("cross");
    expect(L.openings(L.N | L.W)).toEqual(["north", "west"]);
    expect(L.canRotate(15)).toBe(false);
    expect(L.canRotate(L.N | L.S)).toBe(true);
  });

  it("generates deterministically per seed", () => {
    expect(L.generate(20260101)).toEqual(L.generate(20260101));
    expect(L.generate(1).solution).not.toEqual(L.generate(2).solution);
    expect(L.generate(5, 5).cells).toHaveLength(25);
    expect(L.generate(5).cells).toHaveLength(36);
  });

  it("builds a spanning tree: the solution waters everything with size*size-1 links", () => {
    for (let seed = 1; seed <= 200; seed++) {
      const g = L.generate(seed);
      expect(L.isSolved(g.solution, g.size, g.src)).toBe(true);
      let ends = 0;
      for (const m of g.solution) {
        expect(L.isMask(m)).toBe(true);
        expect(m).not.toBe(0);
        ends += [1, 2, 4, 8].filter((b) => m & b).length;
      }
      expect(ends).toBe(2 * (g.size * g.size - 1));
    }
  });

  it("the tangled garden is never solved, and every tile can be turned back to solve it", () => {
    for (let seed = 1; seed <= 200; seed++) {
      const g = L.generate(seed);
      expect(L.isSolved(g.cells, g.size, g.src)).toBe(false);
      let cells = g.cells;
      for (let i = 0; i < cells.length; i++) {
        let turns = 0;
        while (cells[i] !== g.solution[i]) {
          cells = L.turn(cells, i, true);
          turns += 1;
          expect(turns).toBeLessThan(4);
        }
      }
      expect(L.isSolved(cells, g.size, g.src)).toBe(true);
    }
  });

  it("water only passes between tiles that open toward each other", () => {
    // 2x2: spring at 0 opens east; tile 1 opens west (joined) and south; tile 3 opens north.
    const cells = [L.E, L.W | L.S, L.N, 0];
    expect(L.watered(cells, 2, 0)).toEqual([true, true, false, false]);
    // Tile 1 turned away from the spring: nothing passes.
    const turned = [L.E, L.E | L.S, L.N, 0];
    expect(L.watered(turned, 2, 0)).toEqual([true, false, false, false]);
    expect(L.wateredCount(turned, 2, 0)).toBe(1);
    // A one-sided opening does not connect.
    expect(L.watered([L.E, 0, 0, 0], 2, 0)).toEqual([true, false, false, false]);
  });

  it("turn does not mutate and undo reverses a sequence", () => {
    const g = L.generate(9);
    const before = g.cells.slice();
    let cells = g.cells;
    let history: Move[] = [];
    const moves: [number, boolean][] = [
      [3, true],
      [3, false],
      [10, true],
      [20, false],
    ];
    for (const [i, cw] of moves) {
      cells = L.turn(cells, i, cw);
      history = L.pushMove(history, i, cw);
    }
    expect(g.cells).toEqual(before);
    for (let k = 0; k < moves.length; k++) ({ cells, history } = L.undo(cells, history));
    expect(cells).toEqual(before);
    expect(history).toEqual([]);
    expect(L.undo(cells, history)).toEqual({ cells: before, history: [] });
  });

  it("caps the undo history", () => {
    let h: Move[] = [];
    for (let i = 0; i < L.HISTORY_LIMIT + 50; i++) h = L.pushMove(h, i % 36, true);
    expect(h).toHaveLength(L.HISTORY_LIMIT);
  });

  it("dateSeed is stable per local date and differs by day", () => {
    const a = L.dateSeed(new Date(2026, 10, 5));
    expect(a).toBe(L.dateSeed(new Date(2026, 10, 5, 23, 59)));
    expect(a).not.toBe(L.dateSeed(new Date(2026, 10, 6)));
  });

  it("garden state survives a JSON round trip", () => {
    const g = L.generate(77);
    const state = {
      v: 1,
      seed: 77,
      src: g.src,
      start: g.cells,
      cells: L.turn(g.cells, 4, true),
      history: L.pushMove([], 4, true),
    };
    const back = JSON.parse(JSON.stringify(state)) as typeof state;
    expect(back).toEqual(state);
    expect(back.cells.every(L.isMask)).toBe(true);
    expect(L.isMask(16)).toBe(false);
    expect(L.isMask(1.5)).toBe(false);
  });
});

describe("Moss Paths sizes and crossings", () => {
  it("offers 4, 6 and 8", () => {
    expect(L.SIZES).toEqual([4, 6, 8]);
    expect(L.isSize(6)).toBe(true);
    expect(L.isSize(5)).toBe(false);
  });

  it("generates a solvable, unsolved garden at every size", () => {
    for (const size of L.SIZES) {
      for (let seed = 1; seed <= 25; seed++) {
        const g = L.generate(seed, size);
        expect(g.cells).toHaveLength(size * size);
        expect(L.isSolved(g.solution, size, g.src)).toBe(true);
        expect(L.isSolved(g.cells, size, g.src)).toBe(false);
      }
    }
  });

  it("keys the daily garden by size and keeps the 6 by 6 seed", () => {
    const d = new Date(2026, 10, 6);
    const seeds = L.SIZES.map((n) => L.dateSeed(d, n));
    expect(new Set(seeds).size).toBe(3);
    expect(L.dateSeed(d, 6)).toBe(L.dateSeed(d));
    for (const n of L.SIZES)
      expect(L.dateSeed(d, n)).toBe(L.dateSeed(new Date(2026, 10, 6, 20), n));
  });

  it("a crossing can be turned: it stays a crossing, counts as a move and undoes", () => {
    const cells = [15, 5, 5, 5];
    const turned = L.turn(cells, 0, true);
    expect(turned).toEqual(cells);
    const history = L.pushMove([], 0, true);
    expect(history).toHaveLength(1);
    expect(L.undo(turned, history)).toEqual({ cells, history: [] });
    expect(L.pieceType(L.rotate(15, false))).toBe("cross");
  });
});
