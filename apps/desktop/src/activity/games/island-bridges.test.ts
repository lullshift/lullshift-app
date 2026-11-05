// Rule tests for Island Bridges' pure logic.
/// <reference types="node" />
import { describe, expect, it } from "vitest";
import "../../../../../activities/games/island-bridges/logic.js";

type Bridges = number[][];
type Dir = "up" | "right" | "down" | "left";
interface Puzzle {
  size: number;
  seed: number;
  islands: { x: number; y: number; n: number }[];
  solution: Bridges;
}
interface Result {
  ok: boolean;
  bridges?: Bridges;
  j?: number;
  from?: number;
  to?: number;
  reason?: string;
}
interface Move {
  a: number;
  b: number;
  from: number;
  to: number;
}
interface Logic {
  SIZES: number[];
  HISTORY_LIMIT: number;
  dateSeed(d: Date, size: number): number;
  isSize(n: number): boolean;
  generate(seed: number, size: number): Puzzle;
  islandAt(p: Puzzle, x: number, y: number): number;
  neighbourIsland(p: Puzzle, i: number, dir: Dir): number;
  countBetween(b: Bridges, a: number, c: number): number;
  degree(b: Bridges, i: number): number;
  addBridge(p: Puzzle, b: Bridges, i: number, dir: Dir): Result;
  removeBridge(p: Puzzle, b: Bridges, i: number, dir: Dir): Result;
  groupCount(p: Puzzle, b: Bridges): number;
  doneCount(p: Puzzle, b: Bridges): number;
  validBridges(p: Puzzle, b: unknown): boolean;
  isSolved(p: Puzzle, b: Bridges): boolean;
  cellBridges(p: Puzzle, b: Bridges): ({ o: string; c: number; a: number; b: number } | null)[];
  pushMove(h: Move[], a: number, b: number, from: number, to: number): Move[];
  undo(b: Bridges, h: Move[]): { bridges: Bridges; history: Move[] };
}
const L = (globalThis as unknown as { IslandLogic: Logic }).IslandLogic;

// Hand-made 5 by 5 sea:   A . B . .      A=(0,0) n3   B=(2,0) n2
//                         . . . . .      C=(0,2) n3   D=(2,2) n4   E=(4,2) n1
//                         C . D . E      F=(2,4) n2
//                         . . . . .
//                         . . F . .
const hand = (): Puzzle => ({
  size: 5,
  seed: 0,
  islands: [
    { x: 0, y: 0, n: 3 },
    { x: 2, y: 0, n: 2 },
    { x: 0, y: 2, n: 3 },
    { x: 2, y: 2, n: 4 },
    { x: 4, y: 2, n: 1 },
    { x: 2, y: 4, n: 2 },
  ],
  solution: [],
});
// indexes: A0 B1 C2 D3 E4 F5

describe("IslandLogic", () => {
  it("offers 7 and 9", () => {
    expect(L.SIZES).toEqual([7, 9]);
    expect(L.isSize(8)).toBe(false);
    expect(() => L.generate(1, 8)).toThrow();
  });

  it("generates deterministically per seed and size", () => {
    for (const size of L.SIZES) {
      expect(L.generate(20260101, size)).toEqual(L.generate(20260101, size));
      expect(L.generate(1, size)).not.toEqual(L.generate(2, size));
    }
  });

  it("dateSeed is stable per local date and differs by size and day", () => {
    const d = new Date(2026, 10, 5);
    expect(L.dateSeed(d, 7)).toBe(L.dateSeed(new Date(2026, 10, 5, 22, 5), 7));
    expect(L.dateSeed(d, 7)).not.toBe(L.dateSeed(d, 9));
    expect(L.dateSeed(d, 7)).not.toBe(L.dateSeed(new Date(2026, 10, 6), 7));
  });

  it("generated puzzles are solvable, connected and well formed", () => {
    for (const size of L.SIZES) {
      for (let seed = 1; seed <= 150; seed++) {
        const p = L.generate(seed, size);
        expect(p.islands.length).toBeGreaterThanOrEqual(5);
        const cells = new Set(p.islands.map((i) => i.y * size + i.x));
        expect(cells.size).toBe(p.islands.length);
        for (const i of p.islands) {
          expect(i.x).toBeGreaterThanOrEqual(0);
          expect(i.x).toBeLessThan(size);
          expect(i.n).toBeGreaterThanOrEqual(1);
          expect(i.n).toBeLessThanOrEqual(8);
        }
        // Sorted top to bottom, left to right.
        const order = p.islands.map((i) => i.y * size + i.x);
        expect(order).toEqual([...order].sort((a, b) => a - b));
        expect(L.groupCount(p, p.solution)).toBe(1);
        expect(L.validBridges(p, p.solution)).toBe(true);
        expect(L.isSolved(p, p.solution)).toBe(true);
        expect(L.doneCount(p, p.solution)).toBe(p.islands.length);
      }
    }
  });

  it("neighbourIsland finds the first island in a direction", () => {
    const p = hand();
    expect(L.neighbourIsland(p, 0, "right")).toBe(1);
    expect(L.neighbourIsland(p, 0, "down")).toBe(2);
    expect(L.neighbourIsland(p, 0, "left")).toBe(-1);
    expect(L.neighbourIsland(p, 0, "up")).toBe(-1);
    expect(L.neighbourIsland(p, 3, "up")).toBe(1);
    expect(L.neighbourIsland(p, 3, "down")).toBe(5);
    expect(L.neighbourIsland(p, 3, "left")).toBe(2);
    expect(L.neighbourIsland(p, 3, "right")).toBe(4);
    expect(L.neighbourIsland(p, 5, "up")).toBe(3);
  });

  it("adds one bridge at a time, up to two per pair", () => {
    const p = hand();
    let r = L.addBridge(p, [], 0, "right");
    expect(r).toMatchObject({ ok: true, j: 1, from: 0, to: 1 });
    r = L.addBridge(p, r.bridges!, 0, "right");
    expect(r).toMatchObject({ ok: true, from: 1, to: 2 });
    expect(L.countBetween(r.bridges!, 1, 0)).toBe(2);
    // B has number 2, so it is now full as well as the pair maxed at two.
    expect(L.addBridge(p, r.bridges!, 0, "right")).toMatchObject({
      ok: false,
      reason: "two already",
    });
    expect(L.addBridge(p, r.bridges!, 0, "left")).toMatchObject({ ok: false, reason: "no island" });
  });

  it("refuses to give an island more than its number", () => {
    const p = hand();
    let b: Bridges = [];
    b = L.addBridge(p, b, 3, "right").bridges!; // D-E; E has 1 now
    expect(L.addBridge(p, b, 3, "right")).toMatchObject({ ok: false, reason: "full" });
    expect(L.degree(b, 4)).toBe(1);
  });

  it("refuses crossing bridges, in both directions", () => {
    const p = hand();
    // Horizontal C-D crosses nothing. A vertical A-C then B-D then C-D... build a real crossing:
    // use a plus-shaped sea.
    const plus: Puzzle = {
      size: 5,
      seed: 0,
      islands: [
        { x: 2, y: 0, n: 1 },
        { x: 0, y: 2, n: 1 },
        { x: 4, y: 2, n: 1 },
        { x: 2, y: 4, n: 1 },
      ],
      solution: [],
    };
    const h = L.addBridge(plus, [], 1, "right"); // west island toward east island
    expect(h.ok).toBe(true);
    expect(L.addBridge(plus, h.bridges!, 0, "down")).toMatchObject({
      ok: false,
      reason: "crossed",
    });
    const v = L.addBridge(plus, [], 0, "down");
    expect(v.ok).toBe(true);
    expect(L.addBridge(plus, v.bridges!, 1, "right")).toMatchObject({
      ok: false,
      reason: "crossed",
    });
    // Removing the vertical bridge frees the crossing.
    const rm = L.removeBridge(plus, v.bridges!, 0, "down");
    expect(rm.ok).toBe(true);
    expect(L.addBridge(plus, rm.bridges!, 1, "right").ok).toBe(true);
    // Bridges that merely share an end-point island do not cross.
    expect(L.addBridge(p, L.addBridge(p, [], 0, "right").bridges!, 0, "down").ok).toBe(true);
  });

  it("islands side by side cannot be bridged", () => {
    const p: Puzzle = {
      size: 5,
      seed: 0,
      islands: [
        { x: 0, y: 0, n: 1 },
        { x: 1, y: 0, n: 1 },
      ],
      solution: [],
    };
    expect(L.addBridge(p, [], 0, "right")).toMatchObject({ ok: false, reason: "too close" });
    expect(L.validBridges(p, [[0, 1, 1]])).toBe(false);
  });

  it("removes bridges one at a time", () => {
    const p = hand();
    let b = L.addBridge(p, [], 2, "right").bridges!;
    b = L.addBridge(p, b, 2, "right").bridges!;
    expect(L.countBetween(b, 2, 3)).toBe(2);
    let r = L.removeBridge(p, b, 3, "left");
    expect(r).toMatchObject({ ok: true, from: 2, to: 1 });
    r = L.removeBridge(p, r.bridges!, 2, "right");
    expect(r.bridges).toEqual([]);
    expect(L.removeBridge(p, [], 2, "right")).toMatchObject({ ok: false, reason: "none" });
    expect(L.removeBridge(p, [], 2, "left")).toMatchObject({ ok: false, reason: "no island" });
  });

  it("wins only when all numbers match and everything is one group", () => {
    const p = hand();
    // A3: B(1)... build: A-B 1? Numbers: A3 B2 C3 D4 E1 F2. Total = 15 is odd, so adjust.
    p.islands[1]!.n = 1; // B1 -> total 14 -> 7 bridges
    p.islands[5]!.n = 2;
    // A-B 1, A-C 2, C-D 1, D-E 1, D-F 2 -> A=3, B=1, C=3, D=4, E=1, F=2.
    const win: Bridges = [
      [0, 1, 1],
      [0, 2, 2],
      [2, 3, 1],
      [3, 4, 1],
      [3, 5, 2],
    ];
    expect(L.isSolved(p, win)).toBe(true);
    expect(L.doneCount(p, win)).toBe(6);
    expect(L.groupCount(p, win)).toBe(1);
    // Missing a bridge.
    expect(L.isSolved(p, win.slice(0, 4))).toBe(false);
    expect(L.isSolved(p, [])).toBe(false);
  });

  it("accepts any valid solution, not just the generated one", () => {
    // A square of four islands, each needing 2. Two valid answers exist: all single bridges around
    // the square, or double bridges on opposite sides which leave two groups (not a win).
    const sq: Puzzle = {
      size: 5,
      seed: 0,
      islands: [
        { x: 0, y: 0, n: 2 },
        { x: 3, y: 0, n: 2 },
        { x: 0, y: 3, n: 2 },
        { x: 3, y: 3, n: 2 },
      ],
      solution: [],
    };
    const ring: Bridges = [
      [0, 1, 1],
      [0, 2, 1],
      [1, 3, 1],
      [2, 3, 1],
    ];
    expect(L.isSolved(sq, ring)).toBe(true);
    const pairs: Bridges = [
      [0, 1, 2],
      [2, 3, 2],
    ];
    expect(L.isSolved(sq, pairs)).toBe(false); // right numbers, two groups
    expect(L.groupCount(sq, pairs)).toBe(2);
    // A different generated puzzle, solved by an alternative layout with the same numbers.
    const p = L.generate(31, 7);
    const alt = p.solution.map((x) => x.slice());
    expect(L.isSolved(p, alt)).toBe(true);
    alt.reverse();
    expect(L.isSolved(p, alt)).toBe(true); // order of the list does not matter
  });

  it("rejects crossing layouts, even with correct numbers", () => {
    const plus: Puzzle = {
      size: 5,
      seed: 0,
      islands: [
        { x: 2, y: 0, n: 1 },
        { x: 0, y: 2, n: 1 },
        { x: 4, y: 2, n: 1 },
        { x: 2, y: 4, n: 1 },
      ],
      solution: [],
    };
    // Islands sorted by position: 0 (2,0) 1 (0,2) 2 (4,2) 3 (2,4).
    expect(
      L.isSolved(plus, [
        [0, 3, 1],
        [1, 2, 1],
      ]),
    ).toBe(false); // numbers right, bridges cross
  });

  it("validBridges refuses hostile shapes", () => {
    const p = hand();
    expect(L.validBridges(p, null)).toBe(false);
    expect(L.validBridges(p, [[0, 1]])).toBe(false);
    expect(L.validBridges(p, [[0, 1, 3]])).toBe(false);
    expect(L.validBridges(p, [[1, 0, 1]])).toBe(false);
    expect(L.validBridges(p, [[0, 99, 1]])).toBe(false);
    expect(L.validBridges(p, [[0, "1", 1]])).toBe(false);
    expect(L.validBridges(p, [[0, 4, 1]])).toBe(false); // not in line
    expect(L.validBridges(p, [[0, 3, 1]])).toBe(false); // diagonal
    expect(L.validBridges(p, [[0, 5, 1]])).toBe(false); // not facing
    expect(
      L.validBridges(p, [
        [0, 1, 1],
        [0, 1, 2],
      ]),
    ).toBe(false); // duplicate pair
    expect(
      L.validBridges(p, [
        [0, 1, 1],
        [0, 2, 2],
      ]),
    ).toBe(true);
  });

  it("cellBridges lays bridges over the water between islands", () => {
    const p = hand();
    const cells = L.cellBridges(p, [
      [0, 1, 2],
      [0, 2, 1],
    ]);
    expect(cells[1]).toMatchObject({ o: "h", c: 2 });
    expect(cells[5]).toMatchObject({ o: "v", c: 1 });
    expect(cells[0]).toBeNull();
    expect(cells[2]).toBeNull();
    expect(cells[3]).toBeNull();
  });

  it("undo reverses moves in order and stops at the start", () => {
    const p = hand();
    let b: Bridges = [];
    let h: Move[] = [];
    const play = (i: number, dir: Dir, remove = false) => {
      const r = remove ? L.removeBridge(p, b, i, dir) : L.addBridge(p, b, i, dir);
      expect(r.ok).toBe(true);
      h = L.pushMove(h, i, r.j!, r.from!, r.to!);
      b = r.bridges!;
    };
    play(0, "right");
    play(0, "right");
    play(0, "down");
    play(0, "right", true);
    expect(L.countBetween(b, 0, 1)).toBe(1);
    let r = L.undo(b, h);
    expect(L.countBetween(r.bridges, 0, 1)).toBe(2);
    r = L.undo(r.bridges, r.history);
    expect(L.countBetween(r.bridges, 0, 2)).toBe(0);
    r = L.undo(r.bridges, r.history);
    r = L.undo(r.bridges, r.history);
    expect(r.bridges).toEqual([]);
    expect(L.undo(r.bridges, r.history)).toEqual({ bridges: [], history: [] });
  });

  it("history is capped", () => {
    let h: Move[] = [];
    for (let k = 0; k < L.HISTORY_LIMIT + 30; k++) h = L.pushMove(h, 0, 1, 0, 1);
    expect(h).toHaveLength(L.HISTORY_LIMIT);
  });

  it("a play state survives a JSON round trip", () => {
    const p = L.generate(17, 9);
    const half = p.solution.slice(0, Math.max(1, p.solution.length >> 1));
    const h = L.pushMove([], half[0]![0]!, half[0]![1]!, 0, half[0]![2]!);
    const saved = JSON.parse(
      JSON.stringify({ v: 1, seed: p.seed, size: 9, bridges: half, history: h, selected: 2 }),
    );
    expect(saved.bridges).toEqual(half);
    expect(saved.history).toEqual(h);
    const again = L.generate(saved.seed, saved.size);
    expect(again).toEqual(p);
    expect(L.validBridges(again, saved.bridges)).toBe(true);
  });
});
