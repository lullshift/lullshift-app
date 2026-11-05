// Sea Glass rules: seeded boards, line detection, swap rules, clearing and falling, reshuffles.
/// <reference types="node" />
import { describe, expect, it } from "vitest";
import "../../../../../activities/games/sea-glass/logic.js";

interface Game {
  board: number[];
  rng: number;
  score: number;
  swapsLeft: number;
}
type Played = Game & { cleared: number; cascades: number; points: number; reshuffled: boolean };
interface Logic {
  SIZE: number;
  CELLS: number;
  PIECES: number;
  ROUND_SWAPS: number;
  FORMS: string[];
  COLOURS: string[];
  pieceName(p: number): string;
  fillBoard(rng: number): { board: number[]; rng: number };
  neighbour(i: number, dir: number): number;
  areNeighbours(a: number, b: number): boolean;
  findLines(board: number[]): boolean[];
  anyLine(board: number[]): boolean;
  swapMakesLine(board: number[], a: number, b: number): boolean;
  hasMove(board: number[]): boolean;
  resolve(
    board: number[],
    rng: number,
  ): { board: number[]; rng: number; points: number; cleared: number; cascades: number };
  reshuffle(board: number[], rng: number): { board: number[]; rng: number };
  newGame(seed: number): Game;
  swap(game: Game, a: number, b: number): Played | null;
  roundOver(game: Game): boolean;
  validBoard(v: unknown): boolean;
  validGame(v: unknown): boolean;
}
const L = (globalThis as unknown as { GlassLogic: Logic }).GlassLogic;

/** The first allowed swap on a board, scanning right then down. */
function firstSwap(board: number[]): [number, number] | null {
  for (let i = 0; i < L.CELLS; i++) {
    for (const dir of [1, 2]) {
      const j = L.neighbour(i, dir);
      if (j >= 0 && L.swapMakesLine(board, i, j)) return [i, j];
    }
  }
  return null;
}

/** A line-free 8x8 pattern where pieces cycle so no three in a row or column match. */
function calmBoard(): number[] {
  return Array.from({ length: 64 }, (_, i) => 1 + (((i % 8) + 2 * Math.floor(i / 8)) % 3));
}

describe("GlassLogic", () => {
  it("makes six pieces that differ by form and by colour", () => {
    expect(new Set(L.FORMS.slice(1)).size).toBe(6);
    expect(new Set(L.COLOURS.slice(1)).size).toBe(6);
    expect(L.pieceName(2)).toBe("green diamond");
    expect(L.pieceName(0)).toBe("");
  });

  it("generates deterministically per seed, never with a ready line, always with a swap", () => {
    expect(L.newGame(20260101)).toEqual(L.newGame(20260101));
    expect(L.newGame(1).board).not.toEqual(L.newGame(2).board);
    for (let seed = 1; seed <= 200; seed++) {
      const g = L.newGame(seed);
      expect(L.validGame(g)).toBe(true);
      expect(g.swapsLeft).toBe(25);
      expect(g.score).toBe(0);
      expect(L.anyLine(g.board)).toBe(false);
      expect(L.hasMove(g.board)).toBe(true);
    }
  });

  it("finds horizontal and vertical lines of three or more", () => {
    const b = calmBoard();
    expect(L.anyLine(b)).toBe(false);
    const h = b.slice();
    h[0] = h[1] = h[2] = h[3] = 5;
    expect(L.findLines(h).slice(0, 5)).toEqual([true, true, true, true, b[4] === 5]);
    const v = b.slice();
    v[2] = v[10] = v[18] = 6;
    const hit = L.findLines(v);
    expect([2, 10, 18].every((i) => hit[i])).toBe(true);
    expect(hit.filter(Boolean)).toHaveLength(3);
    const two = b.slice();
    two[0] = two[1] = 6;
    two[2] = 4;
    expect(L.findLines(two)[0]).toBe(false);
  });

  it("only allows swaps of touching pieces that make a line", () => {
    const b = calmBoard();
    // Row 0 is 1,2,3,1,2,3,1,2. Put 3,3 at cells 0,1 and a 3 at cell 10 under cell 2.
    b[0] = 3;
    b[1] = 3;
    b[2] = 1;
    b[10] = 3;
    expect(L.anyLine(b)).toBe(false);
    expect(L.swapMakesLine(b, 2, 10)).toBe(true);
    expect(L.swapMakesLine(b, 10, 2)).toBe(true);
    expect(L.swapMakesLine(b, 0, 2)).toBe(false); // not touching
    expect(L.swapMakesLine(b, 5, 6)).toBe(false); // touching but no line
    expect(L.swapMakesLine(b, 7, 7)).toBe(false);
  });

  it("neighbours stop at the edges", () => {
    expect(L.neighbour(0, 0)).toBe(-1);
    expect(L.neighbour(0, 3)).toBe(-1);
    expect(L.neighbour(0, 1)).toBe(1);
    expect(L.neighbour(0, 2)).toBe(8);
    expect(L.neighbour(63, 1)).toBe(-1);
    expect(L.neighbour(63, 2)).toBe(-1);
    expect(L.neighbour(7, 1)).toBe(-1); // does not wrap to the next row
    expect(L.areNeighbours(7, 8)).toBe(false);
  });

  it("clears a line, drops the pieces above and refills from the seed", () => {
    const b = calmBoard();
    b[56] = b[57] = b[58] = 6; // bottom row, three in a row
    const above = b.slice();
    const r = L.resolve(b, 1234);
    expect(r.cleared).toBeGreaterThanOrEqual(3);
    expect(r.points).toBeGreaterThanOrEqual(30);
    expect(L.anyLine(r.board)).toBe(false);
    expect(r.board.every((p) => p >= 1 && p <= 6)).toBe(true);
    // Column 0: everything above the cleared cell moved down one row (unless a chain began).
    if (r.cascades === 1) {
      for (let y = 1; y < 8; y++) expect(r.board[y * 8]).toBe(above[(y - 1) * 8]);
    }
    expect(L.resolve(b, 1234)).toEqual(r);
  });

  it("scores chains a little more at each step", () => {
    const b = calmBoard();
    b[56] = b[57] = b[58] = 6;
    const r = L.resolve(b, 1);
    expect(r.points).toBeGreaterThanOrEqual(r.cleared * 10);
    if (r.cascades > 1) expect(r.points).toBeGreaterThan(r.cleared * 10);
  });

  it("swap plays the round: spends a swap, scores, and is deterministic", () => {
    const g = L.newGame(5);
    const [a, b] = firstSwap(g.board)!;
    const r1 = L.swap(g, a, b)!;
    const r2 = L.swap(g, a, b)!;
    expect(r1).toEqual(r2);
    expect(r1.swapsLeft).toBe(24);
    expect(r1.score).toBeGreaterThanOrEqual(30);
    expect(r1.cleared).toBeGreaterThanOrEqual(3);
    expect(L.anyLine(r1.board)).toBe(false);
    expect(L.hasMove(r1.board)).toBe(true);
    expect(g.swapsLeft).toBe(25); // input untouched
  });

  it("refuses a swap that makes no line, or after the round", () => {
    const g = L.newGame(5);
    const bad = Array.from({ length: 63 }, (_, i) => i)
      .map((i) => [i, i + 1] as [number, number])
      .find(([a, b]) => L.areNeighbours(a, b) && !L.swapMakesLine(g.board, a, b))!;
    expect(L.swap(g, bad[0], bad[1])).toBeNull();
    expect(L.swap(g, -1, 0)).toBeNull();
    expect(L.swap(g, 0, 99)).toBeNull();
    const [a, b] = firstSwap(g.board)!;
    const done = { ...g, swapsLeft: 0 };
    expect(L.roundOver(done)).toBe(true);
    expect(L.swap(done, a, b)).toBeNull();
  });

  it("plays a whole 25-swap round, always legal, ending exactly at zero", () => {
    let g: Game = L.newGame(2026);
    let sawReshuffle = false;
    for (let n = 0; n < 25; n++) {
      expect(L.hasMove(g.board)).toBe(true);
      const [a, b] = firstSwap(g.board)!;
      const r = L.swap(g, a, b)!;
      expect(r).not.toBeNull();
      sawReshuffle = sawReshuffle || r.reshuffled;
      expect(L.validGame(r)).toBe(true);
      g = r;
    }
    void sawReshuffle;
    expect(g.swapsLeft).toBe(0);
    expect(L.roundOver(g)).toBe(true);
    expect(g.score).toBeGreaterThanOrEqual(25 * 30);
  });

  it("reshuffles a stuck board into one with a swap and no ready line", () => {
    // 6-colour stripe pattern has no swap that makes a line.
    const stuck = Array.from({ length: 64 }, (_, i) => 1 + (((i % 8) + 3 * Math.floor(i / 8)) % 6));
    expect(L.anyLine(stuck)).toBe(false);
    expect(L.hasMove(stuck)).toBe(false);
    const r = L.reshuffle(stuck, 77);
    expect(L.anyLine(r.board)).toBe(false);
    expect(L.hasMove(r.board)).toBe(true);
    expect([...r.board].sort()).toEqual([...stuck].sort());
    expect(L.reshuffle(stuck, 77)).toEqual(r);
  });

  it("many random swaps never leave a stuck board", () => {
    for (let seed = 1; seed <= 20; seed++) {
      let g: Game = L.newGame(seed);
      for (let n = 0; n < 25; n++) {
        const moves: [number, number][] = [];
        for (let i = 0; i < 64; i++) {
          for (const d of [1, 2]) {
            const j = L.neighbour(i, d);
            if (j >= 0 && L.swapMakesLine(g.board, i, j)) moves.push([i, j]);
          }
        }
        expect(moves.length).toBeGreaterThan(0);
        const pick = moves[(seed * 7 + n * 13) % moves.length]!;
        g = L.swap(g, pick[0], pick[1])!;
        expect(L.anyLine(g.board)).toBe(false);
      }
    }
  });

  it("validates saved games and survives a JSON round-trip", () => {
    const g = L.newGame(8);
    const [a, b] = firstSwap(g.board)!;
    const r = L.swap(g, a, b)!;
    const snap = { board: r.board, rng: r.rng, score: r.score, swapsLeft: r.swapsLeft };
    const back = JSON.parse(JSON.stringify(snap));
    expect(back).toEqual(snap);
    expect(L.validGame(back)).toBe(true);
    // A restored game continues exactly as the original would.
    const next = firstSwap(back.board)!;
    expect(L.swap(back, next[0], next[1])).toEqual(L.swap(snap, next[0], next[1]));
    expect(L.validGame({ ...back, board: back.board.slice(1) })).toBe(false);
    expect(L.validGame({ ...back, board: back.board.map(() => 0) })).toBe(false);
    expect(L.validGame({ ...back, swapsLeft: 26 })).toBe(false);
    expect(L.validGame({ ...back, score: -1 })).toBe(false);
    expect(L.validGame({ ...back, rng: 2 ** 32 })).toBe(false);
    expect(L.validGame(null)).toBe(false);
  });
});
