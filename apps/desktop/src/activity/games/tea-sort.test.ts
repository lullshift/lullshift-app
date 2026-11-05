// Tea Sort rules: generation, solvability, pouring, undo, win detection and state round-trips.
/// <reference types="node" />
import { describe, expect, it } from "vitest";
import "../../../../../activities/games/tea-sort/logic.js";

type Cups = number[][];
interface Logic {
  COLOURS: number;
  CAP: number;
  CUPS: number;
  HISTORY_LIMIT: number;
  generate(seed: number): { cups: Cups; solution: number[] };
  solvedCups(): Cups;
  topRun(cup: number[]): number;
  pourAmount(cups: Cups, from: number, to: number): number;
  pour(cups: Cups, from: number, to: number): { cups: Cups; amount: number } | null;
  encode(from: number, to: number, amount: number): number;
  decode(code: number): { from: number; to: number; amount: number };
  isMoveCode(v: unknown): boolean;
  undoMove(cups: Cups, code: number): Cups | null;
  pushMove(history: number[], code: number): number[];
  isSolved(cups: Cups): boolean;
  sortedCount(cups: Cups): number;
  validCups(v: unknown): boolean;
}
const L = (globalThis as unknown as { TeaLogic: Logic }).TeaLogic;

describe("TeaLogic", () => {
  it("has 8 cups: 6 teas of 4 layers and 2 empty", () => {
    expect(L.CUPS).toBe(8);
    const s = L.solvedCups();
    expect(s.filter((c) => c.length === 0)).toHaveLength(2);
    expect(L.isSolved(s)).toBe(true);
  });

  it("generates deterministically per seed", () => {
    expect(L.generate(20260101)).toEqual(L.generate(20260101));
    expect(L.generate(1).cups).not.toEqual(L.generate(2).cups);
  });

  it("generates valid, unsolved, solvable puzzles (replaying the solution solves them)", () => {
    for (let seed = 1; seed <= 300; seed++) {
      const { cups, solution } = L.generate(seed);
      expect(L.validCups(cups)).toBe(true);
      expect(L.isSolved(cups)).toBe(false);
      expect(solution.length).toBeGreaterThan(5);
      let state = cups;
      for (const code of solution) {
        const m = L.decode(code);
        const r = L.pour(state, m.from, m.to);
        expect(r).not.toBeNull();
        expect(r!.amount).toBe(m.amount);
        state = r!.cups;
      }
      expect(L.isSolved(state)).toBe(true);
    }
  });

  it("only moves the top run, and only as much as fits", () => {
    const cups: Cups = [[1, 2, 2, 2], [2], [], [2, 2, 2], [1, 1], [], [], []];
    expect(L.topRun(cups[0]!)).toBe(3);
    expect(L.pourAmount(cups, 0, 1)).toBe(3);
    expect(L.pourAmount(cups, 0, 2)).toBe(3);
    expect(L.pourAmount(cups, 0, 3)).toBe(1); // only one space left
    const r = L.pour(cups, 0, 3)!;
    expect(r.amount).toBe(1);
    expect(r.cups[0]).toEqual([1, 2, 2]);
    expect(r.cups[3]).toEqual([2, 2, 2, 2]);
  });

  it("refuses illegal pours and never mutates its input", () => {
    const cups: Cups = [[1, 2], [1, 1, 1, 1], [3], [], [], [], [], []];
    const copy = JSON.parse(JSON.stringify(cups));
    expect(L.pour(cups, 0, 0)).toBeNull(); // same cup
    expect(L.pour(cups, 3, 4)).toBeNull(); // nothing to pour
    expect(L.pour(cups, 0, 1)).toBeNull(); // full
    expect(L.pour(cups, 0, 2)).toBeNull(); // different colour on top
    expect(L.pour(cups, 0, 3)).not.toBeNull();
    expect(cups).toEqual(copy);
  });

  it("detects a win only when each cup is empty or one full colour", () => {
    expect(L.isSolved([[1, 1, 1, 1], [], [2, 2, 2, 2]])).toBe(true);
    expect(L.isSolved([[1, 1, 1], [1], [2, 2, 2, 2]])).toBe(false);
    expect(L.isSolved([[1, 1, 1, 2], [2], []])).toBe(false);
    expect(L.sortedCount(L.solvedCups())).toBe(6);
  });

  it("undoes any number of pours back to the start", () => {
    const { cups, solution } = L.generate(77);
    let state = cups;
    let history: number[] = [];
    for (const code of solution) {
      const m = L.decode(code);
      const r = L.pour(state, m.from, m.to)!;
      history = L.pushMove(history, L.encode(m.from, m.to, r.amount));
      state = r.cups;
    }
    expect(L.isSolved(state)).toBe(true);
    while (history.length) {
      const back = L.undoMove(state, history[history.length - 1]!);
      expect(back).not.toBeNull();
      state = back!;
      history = history.slice(0, -1);
    }
    expect(state).toEqual(cups);
  });

  it("rejects an undo that does not fit the cups", () => {
    expect(L.undoMove(L.solvedCups(), L.encode(0, 6, 2))).toBeNull();
  });

  it("encodes moves and checks move codes", () => {
    expect(L.decode(L.encode(3, 7, 2))).toEqual({ from: 3, to: 7, amount: 2 });
    expect(L.isMoveCode(L.encode(3, 7, 2))).toBe(true);
    for (const bad of [
      -1,
      1.5,
      "x",
      null,
      L.encode(3, 3, 1),
      L.encode(9, 1, 1),
      L.encode(1, 2, 0),
    ]) {
      expect(L.isMoveCode(bad)).toBe(false);
    }
  });

  it("keeps a long history within its limit", () => {
    let h: number[] = [];
    for (let i = 0; i < L.HISTORY_LIMIT + 50; i++) h = L.pushMove(h, L.encode(0, 6, 1));
    expect(h).toHaveLength(L.HISTORY_LIMIT);
  });

  it("validates saved cups", () => {
    const { cups } = L.generate(5);
    expect(L.validCups(JSON.parse(JSON.stringify(cups)))).toBe(true);
    expect(L.validCups([])).toBe(false);
    expect(L.validCups(cups.slice(1))).toBe(false);
    const wrong = JSON.parse(JSON.stringify(cups));
    wrong[0].push(1);
    expect(L.validCups(wrong)).toBe(false);
    const bad = JSON.parse(JSON.stringify(cups));
    bad[0][0] = 99;
    expect(L.validCups(bad)).toBe(false);
  });

  it("survives a JSON round-trip of a mid-game snapshot", () => {
    const { cups, solution } = L.generate(9);
    const m = L.decode(solution[0]!);
    const r = L.pour(cups, m.from, m.to)!;
    const snap = {
      v: 1,
      seed: 9,
      start: cups,
      cups: r.cups,
      history: [L.encode(m.from, m.to, r.amount)],
    };
    const back = JSON.parse(JSON.stringify(snap));
    expect(back).toEqual(snap);
    expect(L.validCups(back.cups)).toBe(true);
    expect(back.history.every(L.isMoveCode)).toBe(true);
    expect(L.undoMove(back.cups, back.history[0])).toEqual(cups);
  });
});
