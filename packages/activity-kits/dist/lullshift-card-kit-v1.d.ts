// Lullshift card kit v1 (ADR-019), from @lullshift/activity-kits 1.0.0.
// Licensed under the Apache License, Version 2.0. Copyright the Lullshift authors.
//
// GENERATED, DO NOT EDIT. Built by packages/activity-kits/scripts/build-kits.ts from
// packages/activity-kits/{src/core/random.ts, src/core/json.ts, src/core/history.ts, src/core/keys.ts, src/card/cards.ts, src/card/dom.ts}.
//
// Types for window.LullshiftKits.card. Reference this file from a TypeScript activity:
//   /// <reference path="./lullshift-card-kit-v1.d.ts" />

declare namespace LullshiftKits {
  namespace card {
    /** A seeded random source. `state()` is a 32-bit integer; `createRng(rng.state())` resumes it. */
    export interface Rng {
      /** A float in [0, 1). */
      next(): number;
      /** An integer in [min, max], both inclusive. */
      int(min: number, max: number): number;
      /** One item of a non-empty array. */
      pick<T>(items: readonly T[]): T;
      /** A shuffled copy (Fisher-Yates); the input is left alone. */
      shuffle<T>(items: readonly T[]): T[];
      /** The current state: save it, then `createRng(state)` continues the same sequence. */
      state(): number;
    }
    /** Creates a generator from a seed (any number; it is reduced to 32 bits). */
    export function createRng(seed: number): Rng;
    /** Any number as an unsigned 32-bit integer; non-finite numbers become 0. */
    export function toUint32(n: number): number;
    /** FNV-1a, 32-bit, over UTF-16 code units: a stable seed from a string. */
    export function hashString(text: string): number;
    /** "YYYY-MM-DD" for the player's local calendar day. */
    export function localDateKey(date: Date): string;
    /**
     * The seed of the day: everyone playing on the same local date gets the same puzzle. Pass a Date
     * (its local date counts) or a "YYYY-MM-DD" string. Optional `salt` gives each game its own
     * puzzle for the same day.
     */
    export function dailySeed(date?: Date | string, salt?: string): number;
    /** A fresh, unpredictable seed for a "new game" (not for anything secret). */
    export function randomSeed(): number;

    /** True for JSON values (finite numbers, strings, booleans, null, plain arrays and objects). */
    export function isJson(value: unknown, depth?: number): boolean;
    /** A deep copy of a JSON value. */
    export function cloneJson<T>(value: T): T;
    /** True when `value` is an object that is not an array or null. */
    export function isRecord(value: unknown): value is Record<string, unknown>;
    /** True for an integer in [min, max]. */
    export function isIntIn(value: unknown, min: number, max: number): value is number;

    export interface HistoryState<T> {
      undo: T[];
      redo: T[];
    }
    export class History<T> {
      readonly limit: number;
      private past;
      private future;
      /** `limit`: how many undo steps to keep (1 to 10000, default 100). */
      constructor(limit?: number);
      /** Remember `before`, the state just before a change. Clears redo. */
      record(before: T): void;
      /** The state to go back to, or undefined. `current` is kept for redo. */
      undo(current: T): T | undefined;
      /** The state to go forward to, or undefined. `current` is kept for undo. */
      redo(current: T): T | undefined;
      canUndo(): boolean;
      canRedo(): boolean;
      /** Number of undo steps available. */
      size(): number;
      clear(): void;
      /** Plain JSON for saved state. */
      snapshot(): HistoryState<T>;
      /**
       * Loads a snapshot. Every entry must pass `isEntry`. Returns false, changing nothing, when the
       * input is malformed. Entries over the limit are dropped, oldest first.
       */
      restore(state: unknown, isEntry: (entry: unknown) => boolean): boolean;
    }

    export type KeyAction =
      | "up"
      | "down"
      | "left"
      | "right"
      | "home"
      | "end"
      | "first"
      | "last"
      | "pageUp"
      | "pageDown"
      | "activate"
      | "cancel"
      | "undo"
      | "redo";
    /** The parts of a KeyboardEvent the map reads. */
    export interface KeyInput {
      key: string;
      code?: string;
      ctrlKey?: boolean;
      metaKey?: boolean;
      shiftKey?: boolean;
      altKey?: boolean;
    }
    export interface KeyOptions {
      /** Move with the W, A, S and D positions as well as the arrows. Off by default. */
      wasd?: boolean;
    }
    /**
     * The action for a key press, or null when the kit has no use for it (let it through then).
     * Ctrl/Cmd+Z is undo; Ctrl/Cmd+Y and Ctrl/Cmd+Shift+Z are redo; Ctrl/Cmd+Home and +End are
     * "first" and "last".
     */
    export function keyAction(event: KeyInput, options?: KeyOptions): KeyAction | null;

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
    export const STANDARD_SUITS: readonly string[];
    export const STANDARD_RANKS: readonly number[];
    /** Limits that `restore` enforces on saved state. */
    export const CARD_LIMITS: {
      readonly cards: 1000;
      readonly piles: 100;
      readonly text: 64;
    };
    export interface DeckOptions {
      suits?: readonly string[];
      ranks?: readonly number[];
      /** How many copies of each card (default 1). */
      copies?: number;
    }
    /** Every suit and rank combination, face down, in order. Defaults to a 52-card deck. */
    export function buildDeck(options?: DeckOptions): Card[];
    /** A deck from your own list of cards, face down. Ids default to suit-rank-n; repeats throw. */
    export function customDeck(specs: readonly CardSpec[]): Card[];
    /** A shuffled copy of `cards` (seeded). */
    export function shuffleDeck(cards: readonly Card[], rng: Rng): Card[];
    /** "Ace", "2" ... "10", "Jack", "Queen", "King". */
    export function rankName(rank: number): string;
    /** "red" for hearts and diamonds, "black" for spades and clubs, otherwise null. */
    export function suitColor(suit: string): "red" | "black" | null;
    /** What a screen reader hears: "Queen of hearts", the custom name, or "Face-down card". */
    export function cardLabel(card: Card): string;
    /** True for a well-formed card. */
    export function isCard(value: unknown): value is Card;
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
    }
    export interface TableState {
      piles: {
        id: string;
        cards: Card[];
      }[];
      focus: CardSpot;
      selection: CardSpot | null;
      history: HistoryState<Card[][]>;
    }
    export type ActivateResult = "selected" | "moved" | "rejected" | "cleared" | "none";
    export class Table {
      piles: Pile[];
      focus: CardSpot;
      selection: CardSpot | null;
      private options;
      private history;
      constructor(piles: readonly PileSpec[], options?: TableOptions);
      pile(id: string): Pile | undefined;
      /** The top card of a pile, if any. */
      top(id: string): Card | undefined;
      /** Whether moving the top `count` cards from `from` to `to` is allowed. */
      canMove(from: string, to: string, count?: number): boolean;
      /** Moves the top `count` cards if allowed. Recorded for undo. Returns false when refused. */
      move(from: string, to: string, count?: number): boolean;
      /** Deals the top `count` cards from one pile to another, ignoring rules. Recorded for undo. */
      deal(from: string, to: string, count?: number, faceUp?: boolean): number;
      /** Runs any change to the piles, recorded for undo when something changed. */
      edit(change: (table: this) => void): boolean;
      undo(): boolean;
      redo(): boolean;
      canUndo(): boolean;
      canRedo(): boolean;
      clearHistory(): void;
      /**
       * Keyboard focus: left and right move between piles (onto the top card), up and down between
       * cards in a pile, Home and End to the first and last pile, PageUp and PageDown to the bottom
       * and top card of the pile. Returns true if focus moved.
       */
      navigate(action: KeyAction): boolean;
      /**
       * Pick up and place, for Enter and Space: with nothing selected, picks up the focused face-up
       * card and the cards on top of it; with a selection, tries to move it onto the focused pile.
       * Activating the selected pile again puts the cards back down.
       */
      activate(): ActivateResult;
      /** Plain JSON for saved state. */
      snapshot(): TableState;
      /**
       * Loads a snapshot. Piles must be the ones this table was made with, in the same order; cards
       * must be well formed and their ids unique. Returns false and changes nothing otherwise.
       */
      restore(state: unknown): boolean;
      private proposal;
      private cardsOf;
      private load;
      private validCards;
      private topIndex;
      private focusPile;
      private focusCard;
      private clampFocus;
    }

    export interface TableBindingOptions {
      /** Accessible name of the whole table, e.g. "Solitaire table". */
      name: string;
      /** Spoken label of a card (default `cardLabel`). "picked up" is added for the selection. */
      label?(card: Card, pile: Pile): string;
      /** Visible text of a card (default: the label for face-up cards, nothing for face-down). */
      text?(card: Card, pile: Pile): string;
      /** Extra class names for a card's button. */
      className?(card: Card, pile: Pile): string;
      /**
       * Enter, Space or a click on a card (index -1 for an empty pile); focus is already there.
       * Return true if you handled it; otherwise the table's pick-up-and-place `activate()` runs.
       */
      onActivate?(pileId: string, cardIndex: number): boolean | void;
      /** What the built-in `activate()` did, so the game can announce it. */
      onResult?(result: ActivateResult): void;
      /** Return true if you handled an action (undo, redo, cancel). */
      onAction?(action: KeyAction, event: KeyboardEvent): boolean | void;
      /** The binding changed the piles (a move, undo or redo): save and update your status. */
      onChange?(): void;
      keys?: KeyOptions;
    }
    export interface TableBinding {
      /** Rebuilds the piles from the model. Call after every change. Keeps focus inside. */
      render(): void;
      /** Moves keyboard focus to the table's focused card. */
      focus(): void;
      destroy(): void;
    }
    /** Renders `table` into `container` (an empty element). */
    export function bindTable(
      container: HTMLElement,
      table: Table,
      options: TableBindingOptions,
    ): TableBinding;
  }
}

interface Window {
  readonly LullshiftKits: typeof LullshiftKits;
}
