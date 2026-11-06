// Card kit model (ADR-019): decks (standard or custom), seeded shuffles, named piles, moves
// checked by a rules hook, undo, keyboard focus between piles and cards, a pick-up-and-place
// selection, and snapshot/restore of plain JSON.

import { History, type HistoryState } from "../core/history.ts";
import { cloneJson, isIntIn, isRecord } from "../core/json.ts";
import type { KeyAction } from "../core/keys.ts";
import type { Rng } from "../core/random.ts";

export interface Card {
  /** Unique on the table. */
  id: string;
  suit: string;
  rank: number;
  faceUp: boolean;
  /** Optional display name for custom sets ("Owl"); the label uses it instead of rank and suit. */
  name?: string;
}

export interface CardSpec {
  suit: string;
  rank: number;
  id?: string;
  name?: string;
}

export const STANDARD_SUITS: readonly string[] = ["spades", "hearts", "diamonds", "clubs"];
export const STANDARD_RANKS: readonly number[] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];

/** Limits that `restore` enforces on saved state. */
export const CARD_LIMITS = { cards: 1000, piles: 100, text: 64 } as const;

export interface DeckOptions {
  suits?: readonly string[];
  ranks?: readonly number[];
  /** How many copies of each card (default 1). */
  copies?: number;
}

/** Every suit and rank combination, face down, in order. Defaults to a 52-card deck. */
export function buildDeck(options: DeckOptions = {}): Card[] {
  const suits = options.suits ?? STANDARD_SUITS;
  const ranks = options.ranks ?? STANDARD_RANKS;
  const copies = Math.max(1, Math.floor(options.copies ?? 1));
  const specs: CardSpec[] = [];
  for (let c = 1; c <= copies; c++) {
    for (const suit of suits) {
      for (const rank of ranks) {
        specs.push({ suit, rank, id: copies > 1 ? `${suit}-${rank}-${c}` : `${suit}-${rank}` });
      }
    }
  }
  return customDeck(specs);
}

/** A deck from your own list of cards, face down. Ids default to suit-rank-n; repeats throw. */
export function customDeck(specs: readonly CardSpec[]): Card[] {
  const seen = new Set<string>();
  return specs.map((s, n) => {
    const card: Card = {
      id: s.id ?? `${s.suit}-${s.rank}-${n + 1}`,
      suit: s.suit,
      rank: s.rank,
      faceUp: false,
    };
    if (s.name !== undefined) card.name = s.name;
    if (!isCard(card)) throw new RangeError(`card ${n + 1} isn't valid`);
    if (seen.has(card.id)) throw new RangeError(`card id ${card.id} is used twice`);
    seen.add(card.id);
    return card;
  });
}

/** A shuffled copy of `cards` (seeded). */
export function shuffleDeck(cards: readonly Card[], rng: Rng): Card[] {
  return rng.shuffle(cards).map((c) => ({ ...c }));
}

/** "Ace", "2" ... "10", "Jack", "Queen", "King". */
export function rankName(rank: number): string {
  switch (rank) {
    case 1:
      return "Ace";
    case 11:
      return "Jack";
    case 12:
      return "Queen";
    case 13:
      return "King";
    default:
      return String(rank);
  }
}

/** "red" for hearts and diamonds, "black" for spades and clubs, otherwise null. */
export function suitColor(suit: string): "red" | "black" | null {
  if (suit === "hearts" || suit === "diamonds") return "red";
  if (suit === "spades" || suit === "clubs") return "black";
  return null;
}

/** What a screen reader hears: "Queen of hearts", the custom name, or "Face-down card". */
export function cardLabel(card: Card): string {
  if (!card.faceUp) return "Face-down card";
  return card.name ?? `${rankName(card.rank)} of ${card.suit}`;
}

/** True for a well-formed card. */
export function isCard(value: unknown): value is Card {
  if (!isRecord(value)) return false;
  const text = (v: unknown) =>
    typeof v === "string" && v.length > 0 && v.length <= CARD_LIMITS.text;
  for (const k of Object.keys(value)) {
    if (!["id", "suit", "rank", "faceUp", "name"].includes(k)) return false;
  }
  return (
    text(value.id) &&
    text(value.suit) &&
    isIntIn(value.rank, -1000, 1000) &&
    typeof value.faceUp === "boolean" &&
    (value.name === undefined || text(value.name))
  );
}

export interface PileSpec {
  id: string;
  /** Spoken name, e.g. "Foundation 1". */
  name: string;
  cards?: Card[];
}

export interface Pile {
  readonly id: string;
  readonly name: string;
  cards: Card[];
}

/** A proposed move of the top `cards` from one pile to another. */
export interface Move {
  from: string;
  to: string;
  cards: Card[];
}

/** Focus and selection point at a pile (by position) and a card in it; card -1 is an empty pile. */
export interface CardSpot {
  pile: number;
  card: number;
}

export interface TableOptions {
  /** Your game's rules. Called after the basic checks (cards exist, face up, different piles). */
  canMove?(move: Move, table: Table): boolean;
  /** After a move, turn the new top card of the source pile face up (solitaire style). */
  revealAfterMove?: boolean;
  /** Undo steps kept (default 100). */
  historyLimit?: number;
  /**
   * Lay the piles out as a grid with this many columns (a whole number from 1 to 100), the way a
   * memory-pairs board does: up and down move between rows, left and right stay inside the row,
   * Home and End go to the row's ends, PageUp and PageDown to the column's top and bottom. Without
   * it, piles are one line: left and right move between piles and up and down between cards. The
   * game still lays the piles out in its own CSS.
   */
  columns?: number;
}

export interface TableState {
  piles: { id: string; cards: Card[] }[];
  focus: CardSpot;
  selection: CardSpot | null;
  history: HistoryState<Card[][]>;
}

export type ActivateResult = "selected" | "moved" | "rejected" | "cleared" | "none";

export class Table {
  piles: Pile[];
  focus: CardSpot = { pile: 0, card: -1 };
  selection: CardSpot | null = null;
  private options: TableOptions;
  private history: History<Card[][]>;

  constructor(piles: readonly PileSpec[], options: TableOptions = {}) {
    if (piles.length < 1 || piles.length > CARD_LIMITS.piles) {
      throw new RangeError(`a table has 1 to ${CARD_LIMITS.piles} piles`);
    }
    const ids = new Set(piles.map((p) => p.id));
    if (ids.size !== piles.length) throw new RangeError("pile ids must be unique");
    this.piles = piles.map((p) => ({ id: p.id, name: p.name, cards: cloneJson(p.cards ?? []) }));
    this.options = options;
    this.history = new History<Card[][]>(options.historyLimit ?? 100);
    this.focus = { pile: 0, card: this.topIndex(0) };
  }

  pile(id: string): Pile | undefined {
    return this.piles.find((p) => p.id === id);
  }

  /** The top card of a pile, if any. */
  top(id: string): Card | undefined {
    const p = this.pile(id);
    return p ? p.cards[p.cards.length - 1] : undefined;
  }

  /** Whether moving the top `count` cards from `from` to `to` is allowed. */
  canMove(from: string, to: string, count = 1): boolean {
    const move = this.proposal(from, to, count);
    return !!move && (!this.options.canMove || this.options.canMove(move, this));
  }

  /** Moves the top `count` cards if allowed. Recorded for undo. Returns false when refused. */
  move(from: string, to: string, count = 1): boolean {
    if (!this.canMove(from, to, count)) return false;
    this.edit((t) => {
      const src = t.pile(from) as Pile;
      const dst = t.pile(to) as Pile;
      dst.cards.push(...src.cards.splice(src.cards.length - count, count));
      const newTop = src.cards[src.cards.length - 1];
      if (t.options.revealAfterMove && newTop) newTop.faceUp = true;
    });
    this.clampFocus();
    return true;
  }

  /** Deals the top `count` cards from one pile to another, ignoring rules. Recorded for undo. */
  deal(from: string, to: string, count = 1, faceUp = true): number {
    const src = this.pile(from);
    const dst = this.pile(to);
    if (!src || !dst || src === dst) return 0;
    const n = Math.max(0, Math.min(Math.floor(count), src.cards.length));
    this.edit(() => {
      for (let i = 0; i < n; i++) {
        const card = src.cards.pop() as Card;
        card.faceUp = faceUp;
        dst.cards.push(card);
      }
    });
    this.clampFocus();
    return n;
  }

  /** Runs any change to the piles, recorded for undo when something changed. */
  edit(change: (table: this) => void): boolean {
    const before = this.cardsOf();
    change(this);
    if (JSON.stringify(before) === JSON.stringify(this.cardsOf())) return false;
    this.history.record(before);
    this.selection = null;
    return true;
  }

  undo(): boolean {
    const prev = this.history.undo(this.cardsOf());
    if (!prev) return false;
    this.load(prev);
    return true;
  }

  redo(): boolean {
    const next = this.history.redo(this.cardsOf());
    if (!next) return false;
    this.load(next);
    return true;
  }

  canUndo(): boolean {
    return this.history.canUndo();
  }

  canRedo(): boolean {
    return this.history.canRedo();
  }

  clearHistory(): void {
    this.history.clear();
  }

  /**
   * Keyboard focus: left and right move between piles (onto the top card), up and down between
   * cards in a pile, Home and End to the first and last pile, PageUp and PageDown to the bottom
   * and top card of the pile. Returns true if focus moved.
   */
  navigate(action: KeyAction): boolean {
    const { pile, card } = this.focus;
    const len = (this.piles[pile] as Pile).cards.length;
    const cols = this.options.columns;
    if (isIntIn(cols, 1, CARD_LIMITS.piles)) return this.navigateGrid(action, pile, cols);
    switch (action) {
      case "left":
        return this.focusPile(pile - 1);
      case "right":
        return this.focusPile(pile + 1);
      case "home":
      case "first":
        return this.focusPile(0);
      case "end":
      case "last":
        return this.focusPile(this.piles.length - 1);
      case "up":
        return this.focusCard(card - 1);
      case "down":
        return this.focusCard(card + 1);
      case "pageUp":
        return this.focusCard(0);
      case "pageDown":
        return this.focusCard(len - 1);
      default:
        return false;
    }
  }

  /**
   * Pick up and place, for Enter and Space: with nothing selected, picks up the focused face-up
   * card and the cards on top of it; with a selection, tries to move it onto the focused pile.
   * Activating the selected pile again puts the cards back down.
   */
  activate(): ActivateResult {
    const { pile, card } = this.focus;
    const focused = this.piles[pile] as Pile;
    if (!this.selection) {
      const c = focused.cards[card];
      if (!c || !c.faceUp) return "none";
      this.selection = { pile, card };
      return "selected";
    }
    const sel = this.selection;
    if (sel.pile === pile) {
      this.selection = null;
      return "cleared";
    }
    const src = this.piles[sel.pile] as Pile;
    if (!this.move(src.id, focused.id, src.cards.length - sel.card)) return "rejected";
    this.selection = null;
    this.focus = { pile, card: this.topIndex(pile) };
    return "moved";
  }

  /** Plain JSON for saved state. */
  snapshot(): TableState {
    return {
      piles: this.piles.map((p) => ({ id: p.id, cards: cloneJson(p.cards) })),
      focus: { ...this.focus },
      selection: this.selection ? { ...this.selection } : null,
      history: this.history.snapshot(),
    };
  }

  /**
   * Loads a snapshot. Piles must be the ones this table was made with, in the same order; cards
   * must be well formed and their ids unique. Returns false and changes nothing otherwise.
   */
  restore(state: unknown): boolean {
    if (!isRecord(state) || !Array.isArray(state.piles)) return false;
    const piles = state.piles;
    if (piles.length !== this.piles.length) return false;
    const cards: Card[][] = [];
    for (let i = 0; i < piles.length; i++) {
      const p = piles[i];
      if (!isRecord(p) || p.id !== (this.piles[i] as Pile).id) return false;
      cards.push(p.cards as Card[]);
    }
    if (!this.validCards(cards)) return false;
    const history = new History<Card[][]>(this.history.limit);
    // Weight = 4 per card (a card is a small object, several JSON values), bounding the work on
    // hostile saved state; 100 undo steps of a 52-card game fit. Malformed entries weigh 0 here
    // and are then rejected by validCards.
    const weigh = (e: unknown) => {
      if (!Array.isArray(e) || e.length !== this.piles.length) return 0;
      let n = 0;
      for (const pile of e) if (Array.isArray(pile)) n += Math.min(pile.length, CARD_LIMITS.cards);
      return n * 4;
    };
    const validEntry = (e: unknown) => this.validCards(e);
    if (state.history !== undefined && !history.restore(state.history, validEntry, weigh)) {
      return false;
    }
    const spot = (v: unknown, c: Card[][]) =>
      isRecord(v) &&
      isIntIn(v.pile, 0, c.length - 1) &&
      isIntIn(v.card, -1, (c[v.pile as number] as Card[]).length - 1);
    if (state.focus !== undefined && !spot(state.focus, cards)) return false;
    if (state.selection != null && !spot(state.selection, cards)) return false;
    this.load(cloneJson(cards));
    this.history = history;
    if (state.focus !== undefined) {
      const f = state.focus as unknown as CardSpot;
      this.focus = { pile: f.pile, card: f.card };
    }
    this.clampFocus();
    const s = state.selection as CardSpot | null | undefined;
    this.selection =
      s && (this.piles[s.pile] as Pile).cards[s.card] ? { pile: s.pile, card: s.card } : null;
    return true;
  }

  private proposal(from: string, to: string, count: number): Move | null {
    const src = this.pile(from);
    const dst = this.pile(to);
    if (!src || !dst || src === dst || !isIntIn(count, 1, src.cards.length)) return null;
    const cards = src.cards.slice(src.cards.length - count);
    if (!cards.every((c) => c.faceUp)) return null;
    return { from, to, cards: cloneJson(cards) };
  }

  private cardsOf(): Card[][] {
    return this.piles.map((p) => cloneJson(p.cards));
  }

  private load(cards: Card[][]): void {
    this.piles = this.piles.map((p, i) => ({ id: p.id, name: p.name, cards: cards[i] ?? [] }));
    this.selection = null;
    this.clampFocus();
  }

  private validCards(value: unknown): boolean {
    if (!Array.isArray(value) || value.length !== this.piles.length) return false;
    const ids = new Set<string>();
    let total = 0;
    for (const pile of value) {
      if (!Array.isArray(pile)) return false;
      total += pile.length;
      if (total > CARD_LIMITS.cards) return false;
      for (const c of pile) {
        if (!isCard(c) || ids.has(c.id)) return false;
        ids.add(c.id);
      }
    }
    return true;
  }

  private topIndex(pile: number): number {
    return ((this.piles[pile] as Pile).cards.length || 0) - 1;
  }

  private navigateGrid(action: KeyAction, pile: number, cols: number): boolean {
    const last = this.piles.length - 1;
    const rowStart = pile - (pile % cols);
    switch (action) {
      case "left":
        return pile % cols === 0 ? false : this.focusPile(pile - 1);
      case "right":
        return pile % cols === cols - 1 || pile === last ? false : this.focusPile(pile + 1);
      case "up":
        return this.focusPile(pile - cols);
      case "down":
        return this.focusPile(pile + cols);
      case "home":
        return this.focusPile(rowStart);
      case "end":
        return this.focusPile(Math.min(last, rowStart + cols - 1));
      case "first":
        return this.focusPile(0);
      case "last":
        return this.focusPile(last);
      case "pageUp":
        return this.focusPile(pile % cols);
      case "pageDown": {
        let bottom = pile % cols;
        while (bottom + cols <= last) bottom += cols;
        return this.focusPile(bottom);
      }
      default:
        return false;
    }
  }

  private focusPile(pile: number): boolean {
    if (pile < 0 || pile >= this.piles.length || pile === this.focus.pile) return false;
    this.focus = { pile, card: this.topIndex(pile) };
    return true;
  }

  private focusCard(card: number): boolean {
    const len = (this.piles[this.focus.pile] as Pile).cards.length;
    if (len === 0 || card < 0 || card >= len || card === this.focus.card) return false;
    this.focus = { pile: this.focus.pile, card };
    return true;
  }

  private clampFocus(): void {
    const pile = Math.max(0, Math.min(this.piles.length - 1, this.focus.pile));
    const len = (this.piles[pile] as Pile).cards.length;
    const card = len === 0 ? -1 : Math.max(0, Math.min(len - 1, this.focus.card));
    this.focus = { pile, card: this.focus.card < 0 && len > 0 ? len - 1 : card };
  }
}
