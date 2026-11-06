// Rule tests for Fog Garden (S06-T04/T05).
/// <reference types="node" />
import { describe, expect, it } from "vitest";
import "../../../../../activities/games/fog-garden/logic.js";

interface Game {
  seed: number;
  brambles: number[] | null;
  first: number | null;
  revealed: number[];
  marks: number[];
  status: "playing" | "hit" | "won";
  size?: number;
}
interface Logic {
  SIZE: number;
  CELLS: number;
  BRAMBLES: number;
  neighbours(i: number, size?: number): number[];
  placeBrambles(seed: number, safe: number, size?: number): number[];
  countAround(brambles: number[], i: number, size?: number): number;
  CENTRE: number;
  newGame(seed: number, size?: number): Game;
  dailyGame(seed: number, size?: number): Game;
  reveal(g: Game, i: number): { game: Game; changed: boolean };
  toggleMark(g: Game, i: number): { game: Game; changed: boolean };
  tryAgain(g: Game): Game;
  isWon(g: Game): boolean;
  bramblesLeft(g: Game): number;
  revealedCount(g: Game): number;
  safeTotal(size?: number): number;
  SIZES: { size: number; brambles: number }[];
  bramblesFor(size: number): number;
  centreOf(size: number): number;
  validate(r: unknown): Game | null;
}
const L = (globalThis as unknown as { FogLogic: Logic }).FogLogic;
const sum = (a: number[]) => a.reduce((n, v) => n + v, 0);

describe("FogLogic", () => {
  it("knows the neighbours of corners, edges and the middle", () => {
    expect(L.neighbours(0).sort((a, b) => a - b)).toEqual([1, 9, 10]);
    expect(L.neighbours(4)).toHaveLength(5);
    expect(L.neighbours(40)).toHaveLength(8);
    expect(L.neighbours(80)).toHaveLength(3);
  });

  it("places brambles deterministically, never on or around the first tile", () => {
    expect(L.placeBrambles(42, 40)).toEqual(L.placeBrambles(42, 40));
    expect(L.placeBrambles(1, 40)).not.toEqual(L.placeBrambles(2, 40));
    for (let seed = 1; seed <= 200; seed++) {
      const safe = (seed * 7) % L.CELLS;
      const b = L.placeBrambles(seed, safe);
      expect(sum(b)).toBe(L.BRAMBLES);
      expect(b[safe]).toBe(0);
      for (const n of L.neighbours(safe)) expect(b[n]).toBe(0);
    }
  });

  it("the first reveal is always safe, wherever it lands, and opens up the field", () => {
    for (let seed = 1; seed <= 100; seed++) {
      for (const i of [0, 8, 40, 72, 80, 13]) {
        const r = L.reveal(L.newGame(seed), i);
        expect(r.changed).toBe(true);
        expect(r.game.status).toBe("playing");
        expect(r.game.first).toBe(i);
        expect((r.game.brambles as number[])[i]).toBe(0);
        // The first tile has no brambles around it, so its neighbours open too.
        for (const n of L.neighbours(i)) expect(r.game.revealed[n]).toBe(1);
      }
    }
  });

  it("counts brambles around a tile", () => {
    const b = new Array<number>(81).fill(0);
    b[0] = 1;
    b[2] = 1;
    b[11] = 1;
    expect(L.countAround(b, 1)).toBe(3);
    expect(L.countAround(b, 10)).toBe(3);
    expect(L.countAround(b, 40)).toBe(0);
  });

  it("a zero spreads, numbered tiles stop the spread, and brambles are never revealed by it", () => {
    const g = L.reveal(L.newGame(7), 40).game;
    const b = g.brambles as number[];
    for (let i = 0; i < L.CELLS; i++) {
      if (g.revealed[i]) {
        expect(b[i]).toBe(0);
        if (L.countAround(b, i) === 0) {
          for (const n of L.neighbours(i)) expect(g.revealed[n]).toBe(1);
        }
      }
    }
  });

  it("marked tiles block reveal and the spread, and marks toggle", () => {
    let g = L.newGame(3);
    g = L.toggleMark(g, 40).game;
    expect(L.bramblesLeft(g)).toBe(L.BRAMBLES - 1);
    const blocked = L.reveal(g, 40);
    expect(blocked.changed).toBe(false);
    expect(blocked.game).toBe(g);
    g = L.toggleMark(g, 40).game;
    expect(L.bramblesLeft(g)).toBe(L.BRAMBLES);
    // A mark beside the first reveal is not opened by the spread.
    g = L.toggleMark(g, 41).game;
    g = L.reveal(g, 40).game;
    expect(g.revealed[41]).toBe(0);
    expect(g.marks[41]).toBe(1);
  });

  it("cannot mark a revealed tile", () => {
    const g = L.reveal(L.newGame(3), 40).game;
    expect(L.toggleMark(g, 40).changed).toBe(false);
  });

  it("revealing a bramble ends the field gently and shows every bramble", () => {
    let g = L.reveal(L.newGame(11), 40).game;
    const b = g.brambles as number[];
    const target = b.indexOf(1);
    g = L.reveal(g, target).game;
    expect(g.status).toBe("hit");
    expect(L.reveal(g, 0).changed).toBe(false);
    expect(L.toggleMark(g, 0).changed).toBe(false);
    // The bramble tile is not counted as a revealed safe tile.
    expect(L.revealedCount(g)).toBe(g.revealed.filter((v, i) => v && !b[i]).length);
  });

  it("try again keeps the same field, covers it, and opens the first safe tile again", () => {
    let g = L.reveal(L.newGame(11), 40).game;
    const b = (g.brambles as number[]).slice();
    g = L.reveal(g, b.indexOf(1)).game;
    const again = L.tryAgain(g);
    expect(again.brambles).toEqual(b);
    expect(again.status).toBe("playing");
    expect(again.marks.every((m) => m === 0)).toBe(true);
    expect(again.revealed[40]).toBe(1);
    expect(again.revealed[b.indexOf(1)]).toBe(0);
    // Before any reveal, try again is just a covered field.
    const fresh = L.tryAgain(L.newGame(5));
    expect(fresh.brambles).toBeNull();
    expect(fresh.revealed.every((v) => v === 0)).toBe(true);
  });

  it("is won exactly when every safe tile is clear", () => {
    let g = L.reveal(L.newGame(21), 40).game;
    const b = g.brambles as number[];
    expect(g.status).toBe("playing");
    expect(L.isWon(g)).toBe(false);
    for (let i = 0; i < L.CELLS && g.status === "playing"; i++) {
      if (!b[i]) g = L.reveal(g, i).game;
    }
    expect(g.status).toBe("won");
    expect(L.revealedCount(g)).toBe(L.safeTotal());
    expect(L.safeTotal()).toBe(69);
    expect(g.revealed.some((v, i) => v && b[i])).toBe(false);
  });

  it("any generated field can be solved by revealing only safe tiles", () => {
    for (let seed = 1; seed <= 50; seed++) {
      let g = L.reveal(L.newGame(seed), (seed * 11) % L.CELLS).game;
      const b = g.brambles as number[];
      for (let i = 0; i < L.CELLS; i++) if (!b[i]) g = L.reveal(g, i).game;
      expect(g.status).toBe("won");
    }
  });

  it("game state survives a JSON round trip and validates", () => {
    let g = L.reveal(L.newGame(99), 12).game;
    g = L.toggleMark(g, 79).game;
    const back = JSON.parse(JSON.stringify(g)) as Game;
    expect(back).toEqual(g);
    expect(L.validate(back)).toEqual(g);
    const hit = L.reveal(g, (g.brambles as number[]).indexOf(1)).game;
    expect(L.validate(JSON.parse(JSON.stringify(hit)))?.status).toBe("hit");
    expect(L.validate(JSON.parse(JSON.stringify(L.newGame(4))))).toEqual(L.newGame(4));
  });

  it("rejects malformed saved state", () => {
    const g = L.reveal(L.newGame(99), 12).game;
    expect(L.validate(null)).toBeNull();
    expect(L.validate({ ...g, seed: 1.5 })).toBeNull();
    expect(L.validate({ ...g, revealed: [1, 0] })).toBeNull();
    expect(L.validate({ ...g, marks: g.marks.map(() => 2) })).toBeNull();
    expect(L.validate({ ...g, brambles: new Array(81).fill(0) })).toBeNull();
    expect(L.validate({ ...g, brambles: null })).toBeNull();
  });
});

describe("Fog Garden daily field", () => {
  it("is the same field for the same seed, opened at the centre", () => {
    const a = L.dailyGame(20261106);
    const b = L.dailyGame(20261106);
    expect(a).toEqual(b);
    expect(a.first).toBe(L.CENTRE);
    expect(a.revealed[L.CENTRE]).toBe(1);
    expect(a.status).toBe("playing");
    for (const n of L.neighbours(L.CENTRE)) expect((a.brambles as number[])[n]).toBe(0);
    expect(L.dailyGame(20261107).brambles).not.toEqual(a.brambles);
  });

  it("try again keeps the daily opening and the state round-trips", () => {
    const a = L.dailyGame(42);
    const again = L.tryAgain(a);
    expect(again.brambles).toEqual(a.brambles);
    expect(again.revealed).toEqual(a.revealed);
    expect(L.validate(JSON.parse(JSON.stringify(a)))).toEqual(a);
  });
});

describe("Fog Garden sizes", () => {
  it("offers three sizes with calm bramble counts", () => {
    expect(L.SIZES).toEqual([
      { size: 7, brambles: 8 },
      { size: 9, brambles: 12 },
      { size: 12, brambles: 24 },
    ]);
  });

  it("places the right number of brambles and keeps the first tile and its ring clear", () => {
    for (const { size, brambles } of L.SIZES) {
      for (let seed = 1; seed <= 60; seed++) {
        const safe = (seed * 13) % (size * size);
        const b = L.placeBrambles(seed, safe, size);
        expect(b).toHaveLength(size * size);
        expect(sum(b)).toBe(brambles);
        expect(b[safe]).toBe(0);
        for (const n of L.neighbours(safe, size)) expect(b[n]).toBe(0);
      }
    }
  });

  it("the first reveal is safe and opens an area at every size", () => {
    for (const { size } of L.SIZES) {
      for (let seed = 1; seed <= 40; seed++) {
        for (const i of [0, size - 1, size * size - 1, Math.floor(size / 2) * size + 3]) {
          const g = L.reveal(L.newGame(seed, size), i).game;
          expect(g.status).toBe("playing");
          expect((g.brambles as number[])[i]).toBe(0);
          expect(g.revealed.filter(Boolean).length).toBeGreaterThan(1);
        }
      }
    }
  });

  it("every size is winnable by revealing only safe tiles and counts are right", () => {
    for (const { size, brambles } of L.SIZES) {
      let g = L.reveal(L.newGame(5, size), 0).game;
      const b = g.brambles as number[];
      for (let i = 0; i < size * size; i++) if (!b[i]) g = L.reveal(g, i).game;
      expect(g.status).toBe("won");
      expect(L.safeTotal(size)).toBe(size * size - brambles);
      expect(L.revealedCount(g)).toBe(size * size - brambles);
      expect(L.bramblesLeft(L.newGame(5, size))).toBe(brambles);
    }
  });

  it("daily field opens the centre at each size and is stable", () => {
    for (const { size } of L.SIZES) {
      const a = L.dailyGame(20261106, size);
      expect(a).toEqual(L.dailyGame(20261106, size));
      expect(a.first).toBe(L.centreOf(size));
      expect(a.revealed[L.centreOf(size)]).toBe(1);
      expect(L.validate(JSON.parse(JSON.stringify(a)))).toEqual(a);
      expect(L.tryAgain(a).revealed).toEqual(a.revealed);
    }
  });

  it("restores older saves without a size as 9 by 9, and rejects bad sizes", () => {
    const g = L.reveal(L.newGame(99), 12).game;
    const old = JSON.parse(JSON.stringify(g)) as Record<string, unknown>;
    delete old.size;
    expect(L.validate(old)?.size).toBe(9);
    expect(L.validate({ ...g, size: 8 })).toBeNull();
    expect(L.validate({ ...g, size: 7 })).toBeNull(); // arrays are 81 long
    expect(L.validate({ ...g, size: "9" })).toBeNull();
  });
});
