// Lullshift grid kit v1 (ADR-019), from @lullshift/activity-kits 1.0.0.
// Licensed under the Apache License, Version 2.0. Copyright the Lullshift authors.
//
// GENERATED, DO NOT EDIT. Built by packages/activity-kits/scripts/build-kits.ts from
// packages/activity-kits/{src/core/random.ts, src/core/json.ts, src/core/history.ts, src/core/keys.ts, src/grid/grid.ts, src/grid/dom.ts}.
//
// Types for window.LullshiftKits.grid. Reference this file from a TypeScript activity:
//   /// <reference path="./lullshift-grid-kit-v1.d.ts" />

declare namespace LullshiftKits {
  namespace grid {
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

    export interface Point {
      x: number;
      y: number;
    }
    /** Everything a Grid holds, as plain JSON. Save it whole; `restore` checks it. */
    export interface GridState<C> {
      width: number;
      height: number;
      cells: C[];
      cursor: Point;
      /** Selected cell indexes, ascending. */
      selected: number[];
      history: HistoryState<C[]>;
    }
    export interface GridOptions {
      /** Moving off an edge comes back on the opposite side of the same row or column. */
      wrap?: boolean;
      /** Undo steps kept (default 100). */
      historyLimit?: number;
    }
    /** Largest width or height a grid accepts (also when restoring). */
    export const GRID_MAX_SIDE = 256;
    export class Grid<C> {
      readonly width: number;
      readonly height: number;
      wrap: boolean;
      cursor: Point;
      private data;
      private chosen;
      private history;
      /** `fill` is a value for every cell, or a function of the cell position. */
      constructor(
        width: number,
        height: number,
        fill: C | ((x: number, y: number) => C),
        options?: GridOptions,
      );
      /** Number of cells. */
      get size(): number;
      /** The cells in reading order (row by row). Read-only: change cells with `set` or `edit`. */
      get cells(): readonly C[];
      inBounds(x: number, y: number): boolean;
      index(x: number, y: number): number;
      point(index: number): Point;
      get(x: number, y: number): C | undefined;
      /** Sets one cell. Returns false when (x, y) is off the grid. Not recorded for undo by itself. */
      set(x: number, y: number, value: C): boolean;
      /** Positions next to (x, y) on the grid; diagonals too when asked. Ignores `wrap`. */
      neighbours(x: number, y: number, diagonal?: boolean): Point[];
      /**
       * Runs `change`, recording the cells from before it so `undo` can bring them back. Nothing is
       * recorded when the cells end up the same.
       */
      edit(change: (grid: this) => void): boolean;
      undo(): boolean;
      redo(): boolean;
      canUndo(): boolean;
      canRedo(): boolean;
      clearHistory(): void;
      /** Moves the cursor to (x, y). Returns false when that is off the grid. */
      setCursor(x: number, y: number): boolean;
      /** Moves the cursor by (dx, dy), wrapping when `wrap` is on. Returns true if it moved. */
      moveCursor(dx: number, dy: number): boolean;
      /**
       * Applies a navigation action from `keyAction`: arrows move one cell, Home and End go to the
       * ends of the row, first and last (Ctrl+Home, Ctrl+End) to the corners, PageUp and PageDown to
       * the top and bottom of the column. Returns true if the cursor moved; other actions return false.
       */
      navigate(action: KeyAction): boolean;
      private jump;
      isSelected(x: number, y: number): boolean;
      /** Adds (x, y) to the selection, or removes it when `on` is false. */
      select(x: number, y: number, on?: boolean): void;
      /** Flips (x, y) in or out of the selection; returns whether it is now selected. */
      toggleSelect(x: number, y: number): boolean;
      clearSelection(): void;
      /** Selected positions in reading order. */
      selection(): Point[];
      /** Shuffles all cells with a seeded generator. Recorded for undo. */
      shuffle(rng: Rng): void;
      /** Everything as plain JSON, safe to hand to `LullshiftBridge.autosave`. */
      snapshot(): GridState<C>;
      /**
       * Loads a snapshot. The size must match this grid, and every cell must be JSON and pass
       * `isCell` when given. Returns false and changes nothing when anything is off. A missing
       * history or selection is treated as empty.
       */
      restore(state: unknown, isCell?: (value: unknown) => boolean): boolean;
    }
    /** A grid filled by a seeded generator: `cell(rng, x, y)` is called in reading order. */
    export function generateGrid<C>(
      width: number,
      height: number,
      rng: Rng,
      cell: (rng: Rng, x: number, y: number) => C,
      options?: GridOptions,
    ): Grid<C>;
    /**
     * `count` distinct cell positions chosen with a seeded generator, for scattering mines, gems or
     * lanterns. `where` limits the candidates. Returns fewer when not enough cells qualify.
     */
    export function scatter<C>(
      grid: Grid<C>,
      rng: Rng,
      count: number,
      where?: (cell: C, x: number, y: number) => boolean,
    ): Point[];

    export interface GridBindingOptions<C> {
      /** Accessible name of the whole grid, e.g. "Lantern pond, 5 by 5". */
      name: string;
      /** Accessible label of one cell, e.g. "Row 2, column 3, lit". Required. */
      label(cell: C, x: number, y: number): string;
      /** Visible text of one cell (default none). Decorative text is fine; the label is what is read. */
      text?(cell: C, x: number, y: number): string;
      /** Extra class names for one cell's button, for styling by state. */
      className?(cell: C, x: number, y: number): string;
      /** When given, the button gets aria-pressed with this value. */
      pressed?(cell: C, x: number, y: number): boolean;
      /** Enter, Space or a click on a cell. The cursor is already on it. */
      onActivate?(x: number, y: number): void;
      /** The cursor moved by keyboard. */
      onMove?(x: number, y: number): void;
      /**
       * Any other action from `keyAction` (undo, redo, cancel). Return true if you handled it.
       * Otherwise undo and redo call `grid.undo()` and `grid.redo()`, then `onChange`.
       */
      onAction?(action: KeyAction, event: KeyboardEvent): boolean | void;
      /** The binding changed the cells (keyboard undo or redo): save and update your status. */
      onChange?(): void;
      /** The game uses the selection: cells then carry aria-selected. */
      selectable?: boolean;
      keys?: KeyOptions;
    }
    export interface GridBinding {
      /** Updates labels, text, classes and the tab stop from the model. Call after every change. */
      render(): void;
      /** Moves keyboard focus to the cursor cell. */
      focus(): void;
      /** Removes the cells and listeners. */
      destroy(): void;
    }
    /** Renders `grid` into `container` (an empty element) and keeps it in step on `render()`. */
    export function bindGrid<C>(
      container: HTMLElement,
      grid: Grid<C>,
      options: GridBindingOptions<C>,
    ): GridBinding;
  }
}

interface Window {
  readonly LullshiftKits: typeof LullshiftKits;
}
