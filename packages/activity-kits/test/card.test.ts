import { describe, expect, it } from "vitest";
import { createRng } from "../src/core/index.ts";
import {
  Table,
  buildDeck,
  cardLabel,
  customDeck,
  isCard,
  shuffleDeck,
  suitColor,
  type Card,
} from "../src/card/index.ts";

const up = (cards: Card[]) => cards.map((c) => ({ ...c, faceUp: true }));
const ids = (cards: Card[]) => cards.map((c) => c.id);

describe("decks", () => {
  it("builds a standard deck and custom sets", () => {
    const deck = buildDeck();
    expect(deck).toHaveLength(52);
    expect(new Set(ids(deck)).size).toBe(52);
    expect(deck[0]).toEqual({ id: "spades-1", suit: "spades", rank: 1, faceUp: false });
    expect(buildDeck({ suits: ["a", "b"], ranks: [1, 2, 3], copies: 2 })).toHaveLength(12);
    const owls = customDeck([
      { suit: "birds", rank: 1, name: "Owl" },
      { suit: "birds", rank: 2, name: "Wren" },
    ]);
    expect(cardLabel({ ...owls[0]!, faceUp: true })).toBe("Owl");
    expect(() =>
      customDeck([
        { suit: "x", rank: 1, id: "a" },
        { suit: "y", rank: 2, id: "a" },
      ]),
    ).toThrow(/twice/);
    expect(() => customDeck([{ suit: "", rank: 1 }])).toThrow(RangeError);
  });

  it("labels cards for screen readers", () => {
    expect(cardLabel({ id: "q", suit: "hearts", rank: 12, faceUp: true })).toBe("Queen of hearts");
    expect(cardLabel({ id: "a", suit: "spades", rank: 1, faceUp: true })).toBe("Ace of spades");
    expect(cardLabel({ id: "a", suit: "spades", rank: 1, faceUp: false })).toBe("Face-down card");
    expect(suitColor("diamonds")).toBe("red");
    expect(suitColor("stars")).toBeNull();
  });

  it("shuffles deterministically by seed", () => {
    const deck = buildDeck();
    const a = shuffleDeck(deck, createRng(99));
    expect(ids(shuffleDeck(deck, createRng(99)))).toEqual(ids(a));
    expect(ids(shuffleDeck(deck, createRng(100)))).not.toEqual(ids(a));
    expect(ids(deck)[0]).toBe("spades-1");
    expect([...ids(a)].sort()).toEqual([...ids(deck)].sort());
  });
});

function table(canMove?: (m: { cards: Card[]; to: string }, t: Table) => boolean) {
  const cards = up(customDeck([1, 2, 3, 4, 5].map((rank) => ({ suit: "s", rank }))));
  return new Table(
    [
      { id: "stock", name: "Stock", cards: cards.slice(0, 3) },
      { id: "a", name: "Pile A", cards: cards.slice(3) },
      { id: "b", name: "Pile B" },
    ],
    { canMove },
  );
}

describe("table moves", () => {
  it("moves with basic checks and the rules hook", () => {
    // Rule: only onto an empty pile or onto a higher rank.
    const t = table((m, tbl) => {
      const top = tbl.top(m.to);
      return !top || top.rank > m.cards[0]!.rank;
    });
    expect(t.move("a", "a")).toBe(false);
    expect(t.move("a", "zzz")).toBe(false);
    expect(t.move("a", "b", 3)).toBe(false);
    expect(t.move("a", "b")).toBe(true);
    expect(ids(t.pile("b")!.cards)).toEqual(["s-5-5"]);
    expect(t.move("stock", "b")).toBe(true);
    expect(t.move("a", "b")).toBe(false); // 4 onto 3 breaks the rule
    expect(t.canMove("stock", "b")).toBe(true);
  });

  it("refuses to move face-down cards", () => {
    const t = new Table([
      { id: "x", name: "X", cards: customDeck([{ suit: "s", rank: 1 }]) },
      { id: "y", name: "Y" },
    ]);
    expect(t.move("x", "y")).toBe(false);
    expect(t.deal("x", "y", 5, true)).toBe(1);
    expect(t.top("y")?.faceUp).toBe(true);
  });

  it("reveals the new top card when asked", () => {
    const cards = customDeck([1, 2].map((rank) => ({ suit: "s", rank })));
    cards[1]!.faceUp = true;
    const t = new Table(
      [
        { id: "x", name: "X", cards },
        { id: "y", name: "Y" },
      ],
      { revealAfterMove: true },
    );
    expect(t.move("x", "y")).toBe(true);
    expect(t.top("x")?.faceUp).toBe(true);
    expect(t.undo()).toBe(true);
    expect(t.pile("x")!.cards[0]!.faceUp).toBe(false);
  });

  it("undoes and redoes moves", () => {
    const t = table();
    const start = t.snapshot().piles;
    t.move("a", "b", 2);
    t.move("stock", "a");
    expect(t.undo() && t.undo()).toBe(true);
    expect(t.snapshot().piles).toEqual(start);
    expect(t.undo()).toBe(false);
    expect(t.redo()).toBe(true);
    expect(t.pile("b")!.cards).toHaveLength(2);
  });
});

describe("table keyboard focus and pick-up-and-place", () => {
  it("moves focus between piles and cards", () => {
    const t = table();
    expect(t.focus).toEqual({ pile: 0, card: 2 });
    expect(t.navigate("up")).toBe(true);
    expect(t.focus).toEqual({ pile: 0, card: 1 });
    expect(t.navigate("pageUp")).toBe(true);
    expect(t.focus.card).toBe(0);
    expect(t.navigate("up")).toBe(false);
    expect(t.navigate("right")).toBe(true);
    expect(t.focus).toEqual({ pile: 1, card: 1 });
    expect(t.navigate("end")).toBe(true);
    expect(t.focus).toEqual({ pile: 2, card: -1 });
    expect(t.navigate("down")).toBe(false);
    expect(t.navigate("right")).toBe(false);
    expect(t.navigate("home")).toBe(true);
    expect(t.focus.pile).toBe(0);
  });

  it("picks up and places with activate", () => {
    const t = table();
    t.focus = { pile: 1, card: 0 };
    expect(t.activate()).toBe("selected");
    expect(t.selection).toEqual({ pile: 1, card: 0 });
    t.focus = { pile: 1, card: 1 };
    expect(t.activate()).toBe("cleared");
    t.focus = { pile: 1, card: 0 };
    t.activate();
    t.focus = { pile: 2, card: -1 };
    expect(t.activate()).toBe("moved");
    expect(t.pile("b")!.cards).toHaveLength(2);
    expect(t.focus).toEqual({ pile: 2, card: 1 });
    expect(t.selection).toBeNull();
    t.focus = { pile: 1, card: -1 };
    expect(t.activate()).toBe("none");
  });

  it("reports a rejected placement and keeps the selection", () => {
    const t = table(() => false);
    t.focus = { pile: 0, card: 2 };
    t.activate();
    t.focus = { pile: 2, card: -1 };
    expect(t.activate()).toBe("rejected");
    expect(t.selection).not.toBeNull();
  });
});

describe("table snapshot and restore", () => {
  it("round-trips through JSON", () => {
    const t = table();
    t.move("a", "b");
    t.navigate("right");
    const saved = JSON.parse(JSON.stringify(t.snapshot())) as unknown;
    const u = table();
    expect(u.restore(saved)).toBe(true);
    expect(u.snapshot()).toEqual(t.snapshot());
    expect(u.undo()).toBe(true);
    expect(u.pile("b")!.cards).toHaveLength(0);
  });

  it("rejects malformed input and changes nothing", () => {
    const good = table().snapshot();
    const card0 = good.piles[0]!.cards[0]!;
    const withPile0 = (cards: unknown[]) => ({
      ...good,
      piles: [{ id: "stock", cards }, ...good.piles.slice(1)],
    });
    const bad: unknown[] = [
      null,
      [],
      { ...good, piles: good.piles.slice(1) },
      { ...good, piles: [...good.piles].reverse() },
      withPile0([{ ...card0, rank: "Q" }]),
      withPile0([{ ...card0, extra: 1 }]),
      withPile0([{ ...card0, id: "s-4-4" }]), // duplicate of a card in pile a
      withPile0([{ ...card0, id: "x".repeat(65) }]),
      withPile0(Array.from({ length: 1001 }, (_, i) => ({ ...card0, id: `c${i}` }))),
      { ...good, focus: { pile: 9, card: 0 } },
      { ...good, focus: { pile: 0, card: 7 } },
      { ...good, selection: { pile: 0, card: -2 } },
      { ...good, history: { undo: [[[]]], redo: [] } },
    ];
    const t = table();
    const before = t.snapshot();
    for (const b of bad) {
      expect(t.restore(b)).toBe(false);
      expect(t.snapshot()).toEqual(before);
    }
    expect(isCard({ ...card0, faceUp: "yes" })).toBe(false);
  });
});
