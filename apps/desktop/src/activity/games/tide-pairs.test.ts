// Rule tests for Tide Pairs (S06-T04/T05).
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// Load the kit and the logic as classic scripts, the way the activity page does.
const root = fileURLToPath(new URL("../../../../../", import.meta.url));
for (const file of [
  "packages/activity-kits/dist/lullshift-card-kit-v1.js",
  "activities/games/tide-pairs/logic.js",
]) {
  new Function("window", readFileSync(join(root, file), "utf8"))(globalThis);
}

interface State {
  seed: number;
  mode: "daily" | "random";
  pairs: number;
  order: number[];
  matched: boolean[];
  open: number[];
  turns: number;
}
interface Logic {
  SYMBOLS: string[];
  deal(seed: number, pairs?: number, mode?: string): State;
  dailySeed(date: Date | string): number;
  symbolAt(s: State, i: number): string;
  isUp(s: State, i: number): boolean;
  pairsFound(s: State): number;
  isWon(s: State): boolean;
  needsSettle(s: State): boolean;
  flip(s: State, i: number): { state: State; result: string };
  settle(s: State): State;
  restore(raw: unknown): State | null;
  snapshot(s: State): State;
}
const L = (globalThis as unknown as { TidePairsLogic: Logic }).TidePairsLogic;

// Slots of the two cards that share a symbol index.
const slotList = (s: State, sym: number) => s.order.flatMap((v, i) => (v === sym ? [i] : []));
const slotsOf = (s: State, sym: number): [number, number] => {
  const [a, b] = slotList(s, sym);
  if (a === undefined || b === undefined) throw new Error("symbol " + sym + " is not a pair");
  return [a, b];
};
// Two slots whose cards do not match.
const mismatch = (s: State): [number, number] => [slotsOf(s, 0)[0], slotsOf(s, 1)[0]];

describe("TidePairsLogic deal", () => {
  it("is deterministic per seed and differs between seeds", () => {
    expect(L.deal(7, 8)).toEqual(L.deal(7, 8));
    expect(L.deal(1, 8).order).not.toEqual(L.deal(2, 8).order);
  });

  it("deals each symbol exactly twice, for 8 and 12 pairs", () => {
    for (const pairs of [8, 12]) {
      for (const seed of [1, 5, 123]) {
        const s = L.deal(seed, pairs);
        expect(s.order).toHaveLength(pairs * 2);
        for (let sym = 0; sym < pairs; sym++) expect(slotList(s, sym)).toHaveLength(2);
        expect(s.matched.every((m) => !m)).toBe(true);
        expect(s.open).toEqual([]);
        expect(s.turns).toBe(0);
      }
    }
  });

  it("falls back to 8 pairs for an unknown size, and uses named symbols", () => {
    expect(L.deal(1, 5).pairs).toBe(8);
    expect(L.symbolAt(L.deal(1, 8), 0)).toMatch(/^[a-z]+$/);
    expect(new Set(L.SYMBOLS).size).toBe(L.SYMBOLS.length);
  });

  it("daily seed is stable for a date and differs between dates", () => {
    expect(L.dailySeed("2026-11-05")).toBe(L.dailySeed(new Date(2026, 10, 5)));
    expect(L.dailySeed("2026-11-05")).not.toBe(L.dailySeed("2026-11-06"));
  });

  it("every deal can be solved: turning each pair clears the board in one turn per pair", () => {
    for (const pairs of [8, 12]) {
      let s = L.deal(9, pairs);
      for (let sym = 0; sym < pairs; sym++) {
        const [a, b] = slotsOf(s, sym);
        s = L.flip(s, a).state;
        const r = L.flip(s, b);
        expect(r.result).toBe("match");
        s = r.state;
      }
      expect(L.isWon(s)).toBe(true);
      expect(s.turns).toBe(pairs);
    }
  });
});

describe("TidePairsLogic turns", () => {
  it("first card then a matching second card: match, both stay up, one turn", () => {
    const s0 = L.deal(3, 8);
    const [a, b] = slotsOf(s0, 4);
    const f = L.flip(s0, a);
    expect(f.result).toBe("first");
    expect(f.state.open).toEqual([a]);
    expect(f.state.turns).toBe(0);
    const m = L.flip(f.state, b);
    expect(m.result).toBe("match");
    expect(m.state.matched[a] && m.state.matched[b]).toBe(true);
    expect(m.state.open).toEqual([]);
    expect(m.state.turns).toBe(1);
    expect(L.pairsFound(m.state)).toBe(1);
  });

  it("a mismatch keeps both up until settled, then turns both back", () => {
    const s0 = L.deal(3, 8);
    const [a, b] = mismatch(s0);
    const f = L.flip(s0, a).state;
    const r = L.flip(f, b);
    expect(r.result).toBe("mismatch");
    expect(r.state.turns).toBe(1);
    expect(L.needsSettle(r.state)).toBe(true);
    expect(L.isUp(r.state, a) && L.isUp(r.state, b)).toBe(true);
    // No third card while the pair shows.
    const c = slotsOf(s0, 2)[0];
    expect(L.flip(r.state, c).result).toBe("ignored");
    const settled = L.settle(r.state);
    expect(settled.open).toEqual([]);
    expect(L.isUp(settled, a) || L.isUp(settled, b)).toBe(false);
    expect(settled.turns).toBe(1);
    expect(L.needsSettle(settled)).toBe(false);
  });

  it("settle leaves a single face-up card and matched cards alone", () => {
    const s0 = L.deal(3, 8);
    const [a, b] = slotsOf(s0, 0);
    let s = L.flip(L.flip(s0, a).state, b).state;
    const c = slotsOf(s0, 1)[0];
    s = L.flip(s, c).state;
    expect(L.settle(s)).toBe(s);
    expect(s.matched[a]).toBe(true);
  });

  it("ignores a card that is already up, matched, or out of range", () => {
    const s0 = L.deal(3, 8);
    const [a, b] = slotsOf(s0, 0);
    const f = L.flip(s0, a).state;
    expect(L.flip(f, a).result).toBe("ignored");
    const m = L.flip(f, b).state;
    expect(L.flip(m, a).result).toBe("ignored");
    expect(L.flip(m, -1).result).toBe("ignored");
    expect(L.flip(m, 16).result).toBe("ignored");
  });

  it("does not change its input", () => {
    const s0 = L.deal(3, 8);
    const copy = JSON.stringify(s0);
    L.flip(s0, 0);
    expect(JSON.stringify(s0)).toBe(copy);
  });
});

describe("TidePairsLogic saved state", () => {
  it("survives a JSON round trip mid-game, including an open card", () => {
    const s0 = L.deal(11, 12, "daily");
    const [a, b] = slotsOf(s0, 3);
    let s = L.flip(L.flip(s0, a).state, b).state;
    s = L.flip(s, slotsOf(s0, 5)[0]).state;
    expect(L.restore(JSON.parse(JSON.stringify(L.snapshot(s))))).toEqual(s);
  });

  it("a restored unmatched pair can be turned back (what resume does)", () => {
    const s0 = L.deal(11, 8);
    const [a, b] = mismatch(s0);
    const s = L.flip(L.flip(s0, a).state, b).state;
    const back = L.restore(JSON.parse(JSON.stringify(s))) as State;
    expect(L.needsSettle(back)).toBe(true);
    expect(L.settle(back).open).toEqual([]);
  });

  it("rejects malformed or inconsistent state", () => {
    const s = L.deal(2, 8);
    const bad = (patch: Record<string, unknown>) => L.restore({ ...s, ...patch });
    expect(L.restore(null)).toBeNull();
    expect(L.restore({})).toBeNull();
    expect(bad({ pairs: 7 })).toBeNull();
    expect(bad({ seed: -1 })).toBeNull();
    expect(bad({ order: s.order.slice(1) })).toBeNull();
    expect(bad({ order: s.order.map(() => 0) })).toBeNull();
    expect(bad({ matched: s.matched.slice(1) })).toBeNull();
    const lone = s.matched.slice();
    lone[0] = true; // a matched card whose twin is not matched
    expect(bad({ matched: lone })).toBeNull();
    expect(bad({ open: [0, 1, 2] })).toBeNull();
    expect(bad({ open: [3, 3] })).toBeNull();
    expect(bad({ open: [99] })).toBeNull();
    expect(bad({ turns: -2 })).toBeNull();
    expect(bad({ turns: 1.5 })).toBeNull();
  });
});
