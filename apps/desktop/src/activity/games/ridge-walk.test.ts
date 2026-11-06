// Rule tests for Ridge Walk (S06-T04/T05).
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// Load the kit and the logic as classic scripts, the way the activity page does.
const root = fileURLToPath(new URL("../../../../../", import.meta.url));
for (const file of [
  "packages/activity-kits/dist/lullshift-card-kit-v1.js",
  "activities/games/ridge-walk/logic.js",
]) {
  new Function("window", readFileSync(join(root, file), "utf8"))(globalThis);
}

interface State {
  seed: number;
  ridges: number[][];
  stock: number[];
  path: number[];
  moves: number[];
  wrap: boolean;
}
interface Logic {
  RIDGES: number;
  FLIP: number;
  rankOf(card: number): number;
  suitOf(card: number): number;
  cardName(card: number): string;
  adjacent(a: number, b: number, wrap?: boolean): boolean;
  deal(seed: number, wrap?: boolean): State;
  layOut(order: number[], seed: number, wrap?: boolean): State;
  shuffledOrder(rng: unknown): number[];
  canPlay(s: State, ridge: number): boolean;
  apply(s: State, move: number): State | null;
  undo(s: State): State | null;
  cardsLeft(s: State): number;
  isWon(s: State): boolean;
  isStuck(s: State): boolean;
  solve(s: State, limit?: number): number[] | null;
  restore(raw: unknown): State | null;
  snapshot(s: State): State;
}
const L = (globalThis as unknown as { RidgeWalkLogic: Logic }).RidgeWalkLogic;

const allCards = (s: State) => [...s.ridges.flat(), ...s.stock, ...s.path];
const order = (): number[] => Array.from({ length: 52 }, (_, i) => i);
// Card helper: rank 1..13 in suit 0.
const c = (rank: number, suit = 0) => suit * 13 + rank - 1;

describe("RidgeWalkLogic deal", () => {
  it("is deterministic per seed and differs between seeds", () => {
    expect(L.deal(9)).toEqual(L.deal(9));
    expect(L.deal(9).ridges).not.toEqual(L.deal(10).ridges);
  });

  it("deals 7 ridges of 5, one path card and 16 stock cards, using each card once", () => {
    for (const seed of [9, 10, 12]) {
      const s = L.deal(seed);
      expect(s.ridges).toHaveLength(7);
      for (const r of s.ridges) expect(r).toHaveLength(5);
      expect(s.path).toHaveLength(1);
      expect(s.stock).toHaveLength(16);
      expect([...allCards(s)].sort((a, b) => a - b)).toEqual(order());
      expect(s.moves).toEqual([]);
    }
  });

  it("deals instantly: the deal is the seed's own shuffle, with no solver check", () => {
    const rng = (
      globalThis as unknown as { LullshiftKits: { card: { createRng(n: number): unknown } } }
    ).LullshiftKits.card.createRng;
    expect(typeof rng).toBe("function");
    const t = Date.now();
    for (let seed = 1; seed <= 200; seed++) L.deal(seed);
    expect(Date.now() - t).toBeLessThan(1000);
    const s = L.deal(9);
    expect(s).toEqual(L.layOut(L.shuffledOrder(rng(9)), 9));
  });

  it("the solver still finds a winning line when one exists", () => {
    let found = 0;
    for (let seed = 1; seed <= 6; seed++) {
      const s = L.deal(seed);
      const solution = L.solve(s);
      if (!solution) continue;
      found += 1;
      let cur: State | null = s;
      for (const m of solution) cur = L.apply(cur as State, m);
      expect(L.isWon(cur as State)).toBe(true);
    }
    expect(found).toBeGreaterThan(0);
  });
});

describe("RidgeWalkLogic rules", () => {
  it("adjacent is one step up or down, and King wraps to Ace", () => {
    expect(L.adjacent(5, 6)).toBe(true);
    expect(L.adjacent(6, 5)).toBe(true);
    expect(L.adjacent(5, 7)).toBe(false);
    expect(L.adjacent(5, 5)).toBe(false);
    expect(L.adjacent(13, 1)).toBe(true);
    expect(L.adjacent(1, 13)).toBe(true);
    expect(L.adjacent(13, 1, false)).toBe(false);
    expect(L.adjacent(13, 12)).toBe(true);
    expect(L.adjacent(13, 2)).toBe(false);
  });

  // A hand-built layout: ridge 0 top is a 6, ridge 1 top is a King, path card is a 5.
  const handmade = (): State => {
    const ord = order();
    const s = L.layOut(ord, 1);
    s.ridges = [[c(2), c(6)], [c(9), c(13)], [], [], [], [], []];
    s.path = [c(5)];
    s.stock = [c(1, 1), c(7, 1)];
    return s;
  };

  it("plays only the top card of a ridge when it is one away from the path card", () => {
    const s = handmade();
    expect(L.canPlay(s, 0)).toBe(true);
    expect(L.canPlay(s, 1)).toBe(false);
    expect(L.canPlay(s, 2)).toBe(false); // empty
    const n = L.apply(s, 0) as State;
    expect(n.path).toEqual([c(5), c(6)]);
    expect(n.ridges[0]).toEqual([c(2)]);
    expect(n.moves).toEqual([0]);
    expect(L.apply(s, 1)).toBeNull();
    expect(L.apply(s, 9)).toBeNull();
  });

  it("does not change its input", () => {
    const s = handmade();
    const copy = JSON.stringify(s);
    L.apply(s, 0);
    L.apply(s, L.FLIP);
    L.undo(L.apply(s, 0) as State);
    expect(JSON.stringify(s)).toBe(copy);
  });

  it("a King can follow an Ace and an Ace a King", () => {
    const s = handmade();
    expect(s.wrap).toBe(true);
    s.path = [c(1)];
    s.ridges[1] = [c(13)];
    expect(L.canPlay(s, 1)).toBe(true);
    s.path = [c(13)];
    s.ridges[1] = [c(1)];
    expect(L.canPlay(s, 1)).toBe(true);
    s.wrap = false;
    expect(L.canPlay(s, 1)).toBe(false);
  });

  it("flips the next stock card onto the path, and refuses when the stock is empty", () => {
    const s = handmade();
    const f = L.apply(s, L.FLIP) as State;
    expect(f.path).toEqual([c(5), c(7, 1)]);
    expect(f.stock).toEqual([c(1, 1)]);
    const g = L.apply(f, L.FLIP) as State;
    expect(g.stock).toEqual([]);
    expect(L.apply(g, L.FLIP)).toBeNull();
  });

  it("detects a win when every ridge is clear", () => {
    const s = handmade();
    s.ridges = [[c(6)], [], [], [], [], [], []];
    expect(L.isWon(s)).toBe(false);
    const n = L.apply(s, 0) as State;
    expect(L.isWon(n)).toBe(true);
    expect(L.isStuck(n)).toBe(false);
  });

  it("detects a stuck position: no play and no stock", () => {
    const s = handmade();
    s.ridges = [[c(9)], [c(13)], [], [], [], [], []];
    s.stock = [];
    expect(L.isStuck(s)).toBe(true);
    expect(L.cardsLeft(s)).toBe(2);
    s.stock = [c(2)];
    expect(L.isStuck(s)).toBe(false); // can still flip
  });

  it("undo reverses plays and flips, all the way back to the deal", () => {
    const start = L.deal(9);
    let cur = start;
    for (let i = 0; i < 12; i++) {
      const m = i % 4 === 3 ? L.FLIP : [0, 1, 2, 3, 4, 5, 6].find((r) => L.canPlay(cur, r));
      cur = L.apply(cur, m === undefined ? L.FLIP : m) as State;
    }
    expect(cur.moves).toHaveLength(12);
    while (cur.moves.length) cur = L.undo(cur) as State;
    expect(cur).toEqual(start);
    expect(L.undo(cur)).toBeNull();
  });
});

describe("RidgeWalkLogic saved state", () => {
  it("survives a JSON round trip mid-game and restores exactly", () => {
    const start = L.deal(10);
    let cur = start;
    for (let i = 0; i < 20; i++) {
      const m = [0, 1, 2, 3, 4, 5, 6].find((r) => L.canPlay(cur, r));
      const next = L.apply(cur, m === undefined ? L.FLIP : m);
      if (!next) break;
      cur = next;
    }
    expect(cur.moves.length).toBeGreaterThan(0);
    const back = L.restore(JSON.parse(JSON.stringify(L.snapshot(cur))));
    expect(back).toEqual(cur);
  });

  it("rejects malformed or tampered state", () => {
    const s = L.deal(9);
    expect(L.restore(null)).toBeNull();
    expect(L.restore({})).toBeNull();
    expect(L.restore({ ...s, seed: "x" })).toBeNull();
    expect(L.restore({ ...s, moves: [99] })).toBeNull();
    expect(L.restore({ ...s, moves: [0, 0, 0] })).toBeNull(); // not replayable
    const tampered = JSON.parse(JSON.stringify(s)) as State;
    tampered.ridges[0]?.reverse();
    expect(L.restore(tampered)).toBeNull();
  });
});
