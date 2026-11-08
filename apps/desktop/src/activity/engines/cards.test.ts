// Rule, schema and fixture tests for the Calm Cards engine (activities/engines/calm-cards).
/// <reference types="node" />
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { hostDataCheck } from "./hostDataCheck";
import { describe, expect, it } from "vitest";

const root = fileURLToPath(new URL("../../../../../", import.meta.url));
new Function("window", readFileSync(join(root, "activities/engines/calm-cards/logic.js"), "utf8"))(
  globalThis,
);

interface Card {
  front: string;
  back?: string;
}
interface Deck {
  title: string;
  intro?: string;
  roundSize?: number;
  cards: Card[];
}
interface State {
  index: number;
  revealed: boolean;
  seed?: number;
  seen?: number[];
}
interface Logic {
  validDeck(c: unknown): boolean;
  roundSizeOf(deck: Deck): number;
  roundOf(deck: Deck, s: { seed?: number; seen?: number[] }): number[];
  roundCards(deck: Deck, s: { seed?: number; seen?: number[] }): Card[];
  remember(seen: number[], round: number[], n: number): number[];
  freshState(deck: Deck, seed: number): State;
  nextRound(deck: Deck, s: State, seed: number): State;
  restore(deck: Deck, raw: unknown): State | null;
}
const L = (globalThis as unknown as { CardsLogic: Logic }).CardsLogic;

const readJson = (p: string): unknown => JSON.parse(readFileSync(p, "utf8"));
const schema = readJson(join(root, "activities/engines/calm-cards/data.schema.json"));
const validate = hostDataCheck(schema as object);

const deck = (roundSize?: number, n = 12): Deck => ({
  title: "Deck",
  ...(roundSize === undefined ? {} : { roundSize }),
  cards: Array.from({ length: n }, (_, i) => ({
    front: `f${i}`,
    ...(i % 3 ? { back: `b${i}` } : {}),
  })),
});
const sorted = (a: number[]) => [...a].sort((x, y) => x - y);

describe("CardsLogic without roundSize", () => {
  it("keeps the whole deck in written order and the plain state", () => {
    const d = deck();
    expect(L.roundSizeOf(d)).toBe(0);
    expect(L.roundCards(d, {})).toEqual(d.cards);
    expect(L.freshState(d, 4)).toEqual({ index: 0, revealed: false });
    expect(L.nextRound(d, { index: 12, revealed: false }, 4)).toEqual({
      index: 0,
      revealed: false,
    });
  });

  it("restores old saves as the engine always did: clamped, never refused", () => {
    const d = deck();
    expect(L.restore(d, { index: 4, revealed: true })).toEqual({ index: 4, revealed: true });
    expect(L.restore(d, { index: 3, revealed: true })).toEqual({ index: 3, revealed: false }); // no back
    expect(L.restore(d, { index: 99, revealed: true })).toEqual({ index: 12, revealed: false });
    expect(L.restore(d, { index: -3 })).toEqual({ index: 0, revealed: false });
    expect(L.restore(d, { index: 2.7 })).toEqual({ index: 2, revealed: false });
    expect(L.restore(d, null)).toEqual({ index: 0, revealed: false });
    expect(L.restore(d, "x")).toEqual({ index: 0, revealed: false });
    expect(L.restore(d, { index: 1, seed: 3, seen: [1] })).toEqual({ index: 1, revealed: false });
  });
});

describe("CardsLogic rounds (roundSize, ADR-028 section 10)", () => {
  it("is deterministic for a seed, draws roundSize cards and keeps the written order", () => {
    const d = deck(6);
    expect(L.roundSizeOf(d)).toBe(6);
    const a = L.roundOf(d, { seed: 9, seen: [] });
    expect(a).toEqual(L.roundOf(d, { seed: 9, seen: [] }));
    expect(a).toHaveLength(6);
    expect(new Set(a).size).toBe(6);
    expect(a).toEqual(sorted(a));
    expect(L.freshState(d, 9)).toEqual({ index: 0, revealed: false, seed: 9, seen: [] });
    const rounds = [1, 2, 3, 4, 5].map((seed) => L.roundOf(d, { seed, seen: [] }).join());
    expect(new Set(rounds).size).toBeGreaterThan(1);
  });

  it("prefers unseen cards, then fills from the least recently seen", () => {
    const d = deck(4);
    for (const seed of [1, 2, 3, 77]) {
      expect(L.roundOf(d, { seed, seen: [0, 1, 2, 3, 4, 5, 6, 7] })).toEqual([8, 9, 10, 11]);
      expect(L.roundOf(d, { seed, seen: [6, 2, 0, 1, 3, 4, 5, 7, 8, 9] })).toEqual([2, 6, 10, 11]);
    }
  });

  it("moves through rounds with a bounded seen list", () => {
    const d = deck(5);
    let s = L.freshState(d, 1);
    const shown: number[] = [];
    for (let round = 0; round < 20; round++) {
      shown.push(...L.roundOf(d, s));
      s = L.nextRound(d, { ...s, index: 5 }, round + 50);
      expect(s.index).toBe(0);
      expect(s.seen!.length).toBeLessThanOrEqual(12);
      expect(new Set(s.seen).size).toBe(s.seen!.length);
    }
    expect(new Set(shown.slice(0, 10)).size).toBe(10);
    expect(new Set(shown.slice(0, 15)).size).toBe(12);
    expect(L.remember([7, 8], [8, 9], 12)).toEqual([7, 8, 9]);
  });

  it("resumes the same round at the same card", () => {
    const d = deck(5);
    const s0 = L.nextRound(d, L.freshState(d, 3), 8);
    const cards = L.roundCards(d, s0);
    const at = cards.findIndex((c) => c.back !== undefined);
    const s = { ...s0, index: at, revealed: true };
    const back = L.restore(d, JSON.parse(JSON.stringify(s)));
    expect(back).toEqual(s);
    expect(L.roundCards(d, back!)).toEqual(cards);
    // A place past the end of the round is clamped to its end.
    expect(L.restore(d, { ...s0, index: 40 })).toEqual({ ...s0, index: 5, revealed: false });
  });

  it("starts a new round for saves that don't fit a deck with roundSize", () => {
    const d = deck(5);
    const ok = L.freshState(d, 2);
    for (const bad of [
      { index: 1, revealed: false }, // an older save
      null,
      { ...ok, seed: -1 },
      { ...ok, seed: "2" },
      { ...ok, seen: [3, 3] },
      { ...ok, seen: [12] },
      { ...ok, seen: Array.from({ length: 13 }, (_, i) => i % 12) },
    ]) {
      expect(L.restore(d, bad), JSON.stringify(bad)).toBeNull();
    }
  });

  it("treats a roundSize at or above the card count as the whole deck", () => {
    for (const k of [12, 60]) {
      expect(L.roundSizeOf(deck(k))).toBe(0);
      expect(L.freshState(deck(k), 1)).toEqual({ index: 0, revealed: false });
      expect(L.validDeck(deck(k))).toBe(true);
    }
    expect(L.validDeck(deck(2))).toBe(false);
    expect(L.validDeck(deck(61))).toBe(false);
    expect(L.validDeck({ ...deck(), roundSize: 4.5 })).toBe(false);
  });
});

describe("CardsLogic validDeck", () => {
  it("rejects malformed decks", () => {
    expect(L.validDeck(deck())).toBe(true);
    expect(L.validDeck(null)).toBe(false);
    expect(L.validDeck({ title: "T", cards: [] })).toBe(false);
    expect(L.validDeck({ title: "T", cards: [{ front: "" }] })).toBe(false);
    expect(L.validDeck({ title: "T", cards: [{ front: "x", back: 3 }] })).toBe(false);
    expect(L.validDeck(deck(undefined, 61))).toBe(false);
  });
});

describe("lullshift.cards@1 data schema and fixtures", () => {
  const dir = join(root, "tests/fixtures/engines/lullshift.cards@1");
  const files = (kind: string): string[] =>
    existsSync(join(dir, kind))
      ? readdirSync(join(dir, kind)).filter((f) => f.endsWith(".json"))
      : [];

  it("has enough fixtures", () => {
    expect(files("valid").length).toBeGreaterThanOrEqual(3);
    expect(files("invalid").length).toBeGreaterThanOrEqual(4);
  });

  for (const f of files("valid")) {
    it(`valid/${f} validates and plays`, () => {
      const data = readJson(join(dir, "valid", f));
      expect(validate(data), JSON.stringify(validate.errors)).toBe(true);
      expect(L.validDeck(data)).toBe(true);
    });
  }
  for (const f of files("invalid")) {
    it(`invalid/${f} is rejected`, () => {
      expect(validate(readJson(join(dir, "invalid", f)))).toBe(false);
    });
  }

  it("uses a closed schema", () => {
    expect(JSON.stringify(schema)).not.toContain('"additionalProperties":true');
  });
});

describe("bundled card packs", () => {
  const base = join(root, "activities/content/calm-cards");
  const packs = existsSync(base) ? readdirSync(base) : [];

  for (const slug of packs) {
    it(`${slug} fits the schema, and sets roundSize when it has 12 or more cards`, () => {
      const manifest = readJson(join(base, slug, "manifest.json")) as { data: string };
      const data = readJson(join(base, slug, manifest.data)) as Deck;
      expect(validate(data), JSON.stringify(validate.errors)).toBe(true);
      expect(L.validDeck(data)).toBe(true);
      if (data.cards.length >= 12) {
        expect(data.roundSize).toBeDefined();
        expect(L.roundSizeOf(data)).toBe(data.roundSize);
      } else {
        expect(data.roundSize).toBeUndefined();
      }
    });
  }
});
