// Clock Patience rules: deals, turns, ending, results and saved state.
/// <reference types="node" />
import { describe, expect, it } from "vitest";
import "../../../../../packages/activity-kits/dist/lullshift-card-kit-v1.js";
import "../../../../../activities/games/clock-patience/logic.js";

interface Last {
  card: number;
  from: number;
  to: number;
}
interface State {
  seed: number;
  down: number[][];
  up: number[][];
  cur: number;
  turned: number;
  last: Last | null;
  over: boolean;
}
interface Logic {
  PILES: number;
  CENTRE: number;
  CARDS: number;
  rank(id: number): number;
  suit(id: number): number;
  rankText(id: number): string;
  cardName(id: number): string;
  pileName(p: number): string;
  dateSeed(d: Date): number;
  deal(seed: number): State;
  step(st: State): { card: number; to: number } | null;
  play(seed: number, turned: number): State;
  finish(seed: number): State;
  faceDown(st: State): number;
  pilesLeft(st: State): number;
  won(st: State): boolean;
  resultText(st: State): string;
  validate(raw: unknown): { seed: number; turned: number; mode: string } | null;
}
const L = (globalThis as unknown as { ClockLogic: Logic }).ClockLogic;

describe("Clock Patience deal", () => {
  it("lays out thirteen piles of four face-down cards, each card once, starting at the centre", () => {
    const d = L.deal(99);
    expect(d.down).toHaveLength(13);
    expect(d.down.every((p) => p.length === 4)).toBe(true);
    expect(d.up.every((p) => p.length === 0)).toBe(true);
    expect(d.down.flat().sort((a, b) => a - b)).toEqual(Array.from({ length: 52 }, (_, i) => i));
    expect(d.cur).toBe(L.CENTRE);
    expect(d.turned).toBe(0);
    expect(d.over).toBe(false);
  });

  it("is deterministic per seed and differs between seeds", () => {
    expect(L.deal(5)).toEqual(L.deal(5));
    expect(L.deal(5).down).not.toEqual(L.deal(6).down);
    expect(L.finish(5)).toEqual(L.finish(5));
  });

  it("today's seed is stable for a date, salted, and changes with the date", () => {
    const a = L.dateSeed(new Date(2026, 10, 6, 8));
    expect(L.dateSeed(new Date(2026, 10, 6, 22))).toBe(a);
    expect(L.dateSeed(new Date(2026, 10, 7))).not.toBe(a);
  });
});

describe("Clock Patience turns", () => {
  it("turns the top card of the current pile and places it under the pile for its rank", () => {
    const st = L.deal(1234);
    const top = st.down[L.CENTRE]![3]!;
    const r = L.step(st);
    expect(r).toEqual({ card: top, to: L.rank(top) - 1 });
    expect(st.down[L.CENTRE]).toHaveLength(3);
    expect(st.up[L.rank(top) - 1]).toEqual([top]);
    expect(st.cur).toBe(L.rank(top) - 1);
    expect(st.turned).toBe(1);
    expect(st.last).toEqual({ card: top, from: L.CENTRE, to: L.rank(top) - 1 });
  });

  it("every card placed under a pile has that pile's rank", () => {
    for (let seed = 1; seed <= 50; seed++) {
      const st = L.finish(seed);
      st.up.forEach((pile, p) => pile.forEach((id) => expect(L.rank(id) - 1).toBe(p)));
    }
  });

  it("a King goes to the centre and play stays there", () => {
    let seed = 1;
    while (L.rank(L.deal(seed).down[L.CENTRE]![3]!) !== 13) seed += 1;
    const st = L.deal(seed);
    const king = st.down[L.CENTRE]![3]!;
    const r = L.step(st);
    expect(r?.to).toBe(L.CENTRE);
    expect(st.cur).toBe(L.CENTRE);
    expect(st.up[L.CENTRE]).toEqual([king]);
  });

  it("always ends exactly when the fourth King is turned", () => {
    for (let seed = 1; seed <= 300; seed++) {
      const st = L.finish(seed);
      expect(st.over).toBe(true);
      expect(st.up[L.CENTRE]).toHaveLength(4);
      expect(st.down[L.CENTRE]).toHaveLength(0);
      expect(st.last?.card && L.rank(st.last.card)).toBe(13);
      expect(st.turned).toBe(st.up.flat().length);
      expect(st.turned).toBeGreaterThanOrEqual(4);
      expect(st.turned).toBeLessThanOrEqual(52);
    }
  });

  it("never turns anything once the game is over", () => {
    const st = L.finish(3);
    const before = JSON.stringify(st);
    expect(L.step(st)).toBeNull();
    expect(JSON.stringify(st)).toBe(before);
  });

  it("never lands on an empty pile before the end", () => {
    for (let seed = 1; seed <= 300; seed++) {
      const st = L.deal(seed);
      while (!st.over) {
        expect(st.down[st.cur]!.length).toBeGreaterThan(0);
        L.step(st);
      }
    }
  });
});

describe("Clock Patience results", () => {
  it("counts piles left and cards face down, and reports them plainly", () => {
    let lost: State | null = null;
    for (let seed = 1; seed < 100 && !lost; seed++) {
      const st = L.finish(seed);
      if (!L.won(st)) lost = st;
    }
    const st = lost as State;
    const piles = st.down.filter((p) => p.length > 0).length;
    expect(L.pilesLeft(st)).toBe(piles);
    expect(L.faceDown(st)).toBe(52 - st.turned);
    const text = L.resultText(st);
    expect(text).toContain(piles + (piles === 1 ? " pile" : " piles") + " left this time");
    expect(text).toContain(String(52 - st.turned) + " cards still face down");
  });

  it("wins only when every card is face up, and says so", () => {
    let won: State | null = null;
    for (let seed = 1; seed < 5000 && !won; seed++) {
      const st = L.finish(seed);
      if (L.won(st)) won = st;
    }
    const st = won as State;
    expect(st).not.toBeNull();
    expect(st.turned).toBe(52);
    expect(L.pilesLeft(st)).toBe(0);
    expect(L.resultText(st)).toContain("Every pile is complete");
  });

  it("has no result before the game ends", () => {
    expect(L.resultText(L.deal(1))).toBe("");
    expect(L.won(L.deal(1))).toBe(false);
  });

  it("wins about one deal in thirteen", () => {
    let wins = 0;
    const N = 6000;
    for (let seed = 1; seed <= N; seed++) if (L.won(L.finish(seed))) wins += 1;
    expect(wins / N).toBeGreaterThan(0.05);
    expect(wins / N).toBeLessThan(0.11);
  });
});

describe("Clock Patience saved state and text", () => {
  it("replaying seed and turn count restores the exact state, through JSON", () => {
    for (const turned of [0, 1, 7, 30, 52]) {
      const live = L.deal(4242);
      for (let i = 0; i < turned && !live.over; i++) L.step(live);
      const saved = JSON.parse(
        JSON.stringify({ v: 1, seed: live.seed, mode: "random", turned: live.turned }),
      );
      const r = L.validate(saved);
      expect(r).not.toBeNull();
      expect(L.play((r as { seed: number }).seed, (r as { turned: number }).turned)).toEqual(live);
    }
  });

  it("rejects malformed or hostile saves", () => {
    expect(L.validate(null)).toBeNull();
    expect(L.validate("x")).toBeNull();
    expect(L.validate({ seed: "1", turned: 3 })).toBeNull();
    expect(L.validate({ seed: 1.5, turned: 3 })).toBeNull();
    expect(L.validate({ seed: -1, turned: 3 })).toBeNull();
    expect(L.validate({ seed: 1, turned: -1 })).toBeNull();
    expect(L.validate({ seed: 1, turned: 53 })).toBeNull();
    expect(L.validate({ seed: 1, turned: 1e9 })).toBeNull();
    expect(L.validate({ seed: 5, turned: 3, mode: "weird" })).toEqual({
      seed: 5,
      turned: 3,
      mode: "daily",
    });
  });

  it("names cards and piles with original suits and clock positions", () => {
    expect(L.cardName(6 + 13)).toBe("7 of Acorns");
    expect(L.cardName(12 + 26)).toBe("King of Berries");
    expect(L.cardName(0)).toBe("Ace of Leaves");
    expect(L.cardName(3 * 13 + 11)).toBe("Queen of Shells");
    expect(L.pileName(0)).toBe("Ace pile, 1 o'clock");
    expect(L.pileName(10)).toBe("Jack pile, 11 o'clock");
    expect(L.pileName(11)).toBe("Queen pile, 12 o'clock");
    expect(L.pileName(L.CENTRE)).toBe("King pile, centre");
    expect(L.rankText(12)).toBe("K");
    expect(L.rankText(0)).toBe("A");
  });
});
