// Harvest Pairs rules: deals, pairing, draw pile, undo, solver and saved-state checks.
/// <reference types="node" />
import { describe, expect, it } from "vitest";
import "../../../../../packages/activity-kits/dist/lullshift-card-kit-v1.js";
import "../../../../../activities/games/harvest-pairs/logic.js";

type Move = number[];
interface State {
  seed: number;
  attempt: number;
  verified: boolean;
  pyr: number[];
  stock: number[];
  waste: number[];
  log: Move[];
}
interface Logic {
  SLOTS: number;
  WASTE: number;
  KIDS: number[][];
  rank(id: number): number;
  suit(id: number): number;
  cardName(id: number): string;
  dateSeed(d: Date): number;
  isUncovered(pyr: number[], s: number): boolean;
  cardAt(st: State, entry: number): number;
  cleared(st: State): boolean;
  left(st: State): number;
  whyNot(st: State, a: number, b: number): string | null;
  remove(st: State, a: number, b: number): Move | null;
  draw(st: State): Move | null;
  turnOver(st: State): Move | null;
  undo(st: State): boolean;
  copy(st: State): State;
  search(st: State, budget: number): { result: string; moves: Move[] };
  replay(st: State, moves: Move[]): State | null;
  plainDeal(seed: number, attempt: number): State;
  newGame(seed: number, budget?: number): State;
  outlook(st: State, budget?: number): { result: string; moves: Move[] };
  validate(raw: unknown): State | null;
}
const L = (globalThis as unknown as { HarvestLogic: Logic }).HarvestLogic;

// A small hand-built state: pyr has 28 slots; -1 is removed.
function make(pyr: Record<number, number>, stock: number[] = [], waste: number[] = []): State {
  const p = Array.from({ length: 28 }, () => -1);
  for (const [k, v] of Object.entries(pyr)) p[Number(k)] = v;
  return { seed: 1, attempt: 0, verified: false, pyr: p, stock, waste, log: [] };
}
// Card ids: rank = id % 13 + 1, suit = floor(id / 13).
const card = (rank: number, suit = 0) => suit * 13 + rank - 1;

describe("Harvest Pairs deals", () => {
  it("deals every card exactly once, 28 on the field and 24 in the draw pile", () => {
    const d = L.plainDeal(12345, 0);
    expect(d.pyr).toHaveLength(28);
    expect(d.stock).toHaveLength(24);
    expect(d.waste).toEqual([]);
    expect([...d.pyr, ...d.stock].sort((a, b) => a - b)).toEqual(
      Array.from({ length: 52 }, (_, i) => i),
    );
  });

  it("is deterministic per seed and differs between seeds", () => {
    expect(L.newGame(777)).toEqual(L.newGame(777));
    expect(L.newGame(1).pyr).not.toEqual(L.newGame(2).pyr);
  });

  it("today's seed is stable for a date, salted, and changes with the date", () => {
    const a = L.dateSeed(new Date(2026, 10, 6, 9));
    expect(L.dateSeed(new Date(2026, 10, 6, 23))).toBe(a);
    expect(L.dateSeed(new Date(2026, 10, 7))).not.toBe(a);
  });

  it("every verified deal is cleared by replaying its solver moves through the real rules", () => {
    let verified = 0;
    for (let seed = 1; seed <= 12; seed++) {
      const g = L.newGame(seed * 104729);
      if (!g.verified) continue;
      verified += 1;
      const found = L.search(g, 60000);
      expect(found.result).toBe("solved");
      const end = L.replay(g, found.moves);
      expect(end).not.toBeNull();
      expect(L.cleared(end as State)).toBe(true);
    }
    expect(verified).toBeGreaterThanOrEqual(11);
  }, 60000);
});

describe("Harvest Pairs pairing rules", () => {
  it("only uncovered cards are free", () => {
    const st = L.plainDeal(5, 0);
    for (let s = 0; s < 21; s++) expect(L.isUncovered(st.pyr, s)).toBe(false);
    for (let s = 21; s < 28; s++) expect(L.isUncovered(st.pyr, s)).toBe(true);
    // Removing one child is not enough; both children must be gone.
    const p = st.pyr.slice();
    const [k0, k1] = L.KIDS[15] as [number, number];
    p[k0] = -1;
    expect(L.isUncovered(p, 15)).toBe(false);
    p[k1] = -1;
    expect(L.isUncovered(p, 15)).toBe(true);
  });

  it("removes two free cards that add up to 13", () => {
    const st = make({ 21: card(5), 22: card(8, 1), 23: card(6) });
    expect(L.whyNot(st, 21, 22)).toBeNull();
    expect(L.remove(st, 21, 22)).not.toBeNull();
    expect(st.pyr[21]).toBe(-1);
    expect(st.pyr[22]).toBe(-1);
    expect(st.pyr[23]).toBe(card(6));
  });

  it("refuses other sums, with a plain reason, and changes nothing", () => {
    const st = make({ 21: card(5), 22: card(7), 23: card(6) });
    expect(L.whyNot(st, 21, 22)).toBe("Those add up to 12, not 13.");
    expect(L.remove(st, 21, 22)).toBeNull();
    expect(L.left(st)).toBe(3);
    expect(L.whyNot(st, 21, 21)).toBe("Pick a different card.");
  });

  it("refuses covered cards", () => {
    const st = make({ 0: card(6), 1: card(7), 2: card(2), 3: card(11) });
    // Slot 0 is covered by slots 1 and 2.
    expect(L.whyNot(st, 0, 1)).toBe("That card is not free.");
    expect(L.remove(st, 0, 1)).toBeNull();
  });

  it("a King goes alone, and nothing else does", () => {
    const st = make({ 21: card(13, 2), 22: card(12) });
    expect(L.whyNot(st, 21, -1)).toBeNull();
    expect(L.whyNot(st, 22, -1)).toBe("Only a King can go alone.");
    expect(L.remove(st, 21, -1)).not.toBeNull();
    expect(st.pyr[21]).toBe(-1);
  });

  it("covered cards become free after the cards over them are removed", () => {
    // Slot 15 (row 5, col 0) sits over slots 21 and 22.
    const st = make({ 15: card(13), 21: card(13), 22: card(13) });
    expect(L.whyNot(st, 15, -1)).toBe("That card is not free.");
    L.remove(st, 21, -1);
    expect(L.whyNot(st, 15, -1)).toBe("That card is not free.");
    L.remove(st, 22, -1);
    expect(L.whyNot(st, 15, -1)).toBeNull();
  });

  it("detects a cleared field", () => {
    const st = make({ 21: card(13) });
    expect(L.cleared(st)).toBe(false);
    L.remove(st, 21, -1);
    expect(L.cleared(st)).toBe(true);
  });

  it("the top of the waste pile pairs with a free card, or goes alone as a King", () => {
    const st = make({ 21: card(4) }, [], [card(1), card(9, 3)]);
    expect(L.cardAt(st, L.WASTE)).toBe(card(9, 3));
    expect(L.whyNot(st, 21, L.WASTE)).toBeNull();
    L.remove(st, 21, L.WASTE);
    expect(st.waste).toEqual([card(1)]);
    const k = make({}, [], [card(13)]);
    expect(L.remove(k, L.WASTE, -1)).not.toBeNull();
    expect(k.waste).toEqual([]);
    expect(L.whyNot(make({}, [], []), L.WASTE, -1)).toBe("That card is not free.");
  });
});

describe("Harvest Pairs draw pile", () => {
  it("draws onto the waste pile and turns it over in the same order, as often as you like", () => {
    const st = make({ 21: card(2) }, [30, 20, 10], []); // 10 is drawn first
    expect(L.draw(st)).toEqual([0]);
    expect(L.draw(st)).toEqual([0]);
    expect(L.draw(st)).toEqual([0]);
    expect(st.waste).toEqual([10, 20, 30]);
    expect(L.draw(st)).toBeNull();
    expect(L.turnOver(st)).toEqual([1]);
    expect(st.stock).toEqual([30, 20, 10]);
    expect(st.waste).toEqual([]);
    expect(L.turnOver(st)).toBeNull();
    for (let round = 0; round < 3; round++) {
      L.draw(st);
      L.draw(st);
      L.draw(st);
      L.turnOver(st);
    }
    expect(st.stock).toEqual([30, 20, 10]);
  });

  it("turning over is only offered when the draw pile is empty", () => {
    const st = make({}, [1, 2], [3]);
    expect(L.turnOver(st)).toBeNull();
  });
});

describe("Harvest Pairs undo", () => {
  it("is unlimited and restores the exact state, move by move", () => {
    const start = L.newGame(424242);
    const st = L.copy(start);
    const snapshots: State[] = [L.copy(st)];
    const found = L.search(st, 60000);
    expect(found.result).toBe("solved");
    for (const m of found.moves) {
      if (m[0] === 0) L.draw(st);
      else if (m[0] === 1) L.turnOver(st);
      else L.remove(st, m[1]!, m[2]!);
      snapshots.push(L.copy(st));
    }
    expect(L.cleared(st)).toBe(true);
    for (let i = snapshots.length - 2; i >= 0; i--) {
      expect(L.undo(st)).toBe(true);
      expect(st).toEqual(snapshots[i]);
    }
    expect(L.undo(st)).toBe(false);
    expect(st.pyr).toEqual(start.pyr);
    expect(st.stock).toEqual(start.stock);
  });

  it("keeps a long history (well past 100 moves)", () => {
    const st = make(
      { 21: card(2) },
      Array.from({ length: 24 }, (_, i) => 24 - i),
      [],
    );
    for (let round = 0; round < 20; round++) {
      for (let i = 0; i < 24; i++) L.draw(st);
      L.turnOver(st);
    }
    expect(st.log.length).toBe(20 * 25);
    for (let i = 0; i < 20 * 25; i++) expect(L.undo(st)).toBe(true);
    expect(st.waste).toEqual([]);
    expect(st.stock).toEqual(Array.from({ length: 24 }, (_, i) => 24 - i));
  });

  it("refuses to undo a log that does not fit the state", () => {
    const st = make({}, [], []);
    st.log = [[0]];
    expect(L.undo(st)).toBe(false);
    st.log = [[2, 5, -1, card(13), -1]];
    st.pyr[5] = 3;
    expect(L.undo(st)).toBe(false);
  });
});

describe("Harvest Pairs solver", () => {
  it("recognises a deal that cannot be cleared", () => {
    // One free 5 and nothing it can pair with, empty piles.
    const st = make({ 21: card(5) });
    expect(L.outlook(st).result).toBe("dead");
    expect(L.search(st, 1000).moves).toEqual([]);
  });

  it("finds a path that needs the draw pile and a turn over", () => {
    // The 8 for the field 5 is under the 2 in the waste pile: it only comes back after a turn over.
    const st = make({ 21: card(5) }, [], [card(8), card(2)]);
    const r = L.search(st, 1000);
    expect(r.result).toBe("solved");
    expect(r.moves.some((m) => m[0] === 1)).toBe(true);
    const end = L.replay(st, r.moves);
    expect(end && L.cleared(end)).toBe(true);
  });

  it("reports unknown, never a false answer, when the budget runs out", () => {
    const g = L.plainDeal(31, 0);
    const r = L.search(g, 3);
    expect(["unknown", "solved"]).toContain(r.result);
    if (r.result === "unknown") expect(r.moves).toEqual([]);
  });

  it("a solved search on a mid-game state with cards in the waste pile replays cleanly", () => {
    const g = L.newGame(9001);
    const st = L.copy(g);
    L.draw(st);
    L.draw(st);
    const r = L.outlook(st, 60000);
    if (r.result === "solved") {
      const end = L.replay(st, r.moves);
      expect(end && L.cleared(end)).toBe(true);
    } else {
      expect(["dead", "unknown"]).toContain(r.result);
    }
  });
});

describe("Harvest Pairs saved state", () => {
  it("survives a JSON round trip mid-game", () => {
    const st = L.newGame(2468);
    const r = L.search(st, 60000);
    const mid = L.replay(st, r.moves.slice(0, 12)) as State;
    const back = L.validate(JSON.parse(JSON.stringify(mid)));
    expect(back).toEqual(mid);
    // And undo still works on the restored copy.
    const before = L.copy(back as State);
    expect(L.undo(back as State)).toBe(true);
    expect(back).not.toEqual(before);
  });

  it("rejects malformed or hostile saves", () => {
    const g = L.plainDeal(3, 0);
    const bad = (mut: (s: Record<string, unknown>) => void) => {
      const s = JSON.parse(JSON.stringify(g)) as Record<string, unknown>;
      mut(s);
      return L.validate(s);
    };
    expect(L.validate(null)).toBeNull();
    expect(L.validate("x")).toBeNull();
    expect(bad((s) => (s.pyr = []))).toBeNull();
    expect(bad((s) => ((s.pyr as number[])[0] = 99))).toBeNull();
    expect(bad((s) => ((s.pyr as number[])[0] = (s.pyr as number[])[1]!))).toBeNull(); // duplicate
    expect(bad((s) => (s.stock = [...(s.stock as number[]), 0]))).toBeNull();
    expect(bad((s) => (s.log = [[7]]))).toBeNull();
    expect(bad((s) => (s.log = [[2, 99, -1, 1, -1]]))).toBeNull();
    expect(bad((s) => (s.log = Array.from({ length: 20001 }, () => [0])))).toBeNull();
    expect(bad((s) => (s.seed = "1"))).toBeNull();
    expect(bad(() => undefined)).not.toBeNull();
  });
});

describe("Harvest Pairs card text", () => {
  it("reads value and suit with original suit names", () => {
    expect(L.cardName(card(7, 1))).toBe("7 of Acorns");
    expect(L.cardName(card(1, 0))).toBe("Ace of Leaves");
    expect(L.cardName(card(13, 2))).toBe("King of Berries");
    expect(L.cardName(card(12, 3))).toBe("Queen of Shells");
    expect(L.rank(card(11, 2))).toBe(11);
    expect(L.suit(card(11, 2))).toBe(2);
  });
});
