// Grid kit model (ADR-019): a rectangular board of plain JSON cells, a cursor, a selection,
// keyboard navigation, undo by snapshots, seeded generators, and snapshot/restore of plain JSON.

import { History, type HistoryState } from "../core/history.ts";
import { cloneJson, isIntIn, isJson, isRecord } from "../core/json.ts";
import type { KeyAction } from "../core/keys.ts";
import type { Rng } from "../core/random.ts";

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
  cursor: Point = { x: 0, y: 0 };
  private data: C[];
  private chosen: Set<number> = new Set();
  private history: History<C[]>;

  /** `fill` is a value for every cell, or a function of the cell position. */
  constructor(
    width: number,
    height: number,
    fill: C | ((x: number, y: number) => C),
    options: GridOptions = {},
  ) {
    if (!isIntIn(width, 1, GRID_MAX_SIDE) || !isIntIn(height, 1, GRID_MAX_SIDE)) {
      throw new RangeError(`grid sides are whole numbers from 1 to ${GRID_MAX_SIDE}`);
    }
    this.width = width;
    this.height = height;
    this.wrap = !!options.wrap;
    this.history = new History<C[]>(options.historyLimit ?? 100);
    this.data = [];
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        const v = typeof fill === "function" ? (fill as (x: number, y: number) => C)(x, y) : fill;
        this.data.push(cloneJson(v));
      }
    }
  }

  /** Number of cells. */
  get size(): number {
    return this.width * this.height;
  }

  /** The cells in reading order (row by row). Read-only: change cells with `set` or `edit`. */
  get cells(): readonly C[] {
    return this.data;
  }

  inBounds(x: number, y: number): boolean {
    return isIntIn(x, 0, this.width - 1) && isIntIn(y, 0, this.height - 1);
  }

  index(x: number, y: number): number {
    return y * this.width + x;
  }

  point(index: number): Point {
    return { x: index % this.width, y: Math.floor(index / this.width) };
  }

  get(x: number, y: number): C | undefined {
    return this.inBounds(x, y) ? this.data[this.index(x, y)] : undefined;
  }

  /** Sets one cell. Returns false when (x, y) is off the grid. Not recorded for undo by itself. */
  set(x: number, y: number, value: C): boolean {
    if (!this.inBounds(x, y)) return false;
    this.data[this.index(x, y)] = value;
    return true;
  }

  /** Positions next to (x, y) on the grid; diagonals too when asked. Ignores `wrap`. */
  neighbours(x: number, y: number, diagonal = false): Point[] {
    const out: Point[] = [];
    for (let dy = -1; dy <= 1; dy++) {
      for (let dx = -1; dx <= 1; dx++) {
        if ((dx === 0 && dy === 0) || (!diagonal && dx !== 0 && dy !== 0)) continue;
        if (this.inBounds(x + dx, y + dy)) out.push({ x: x + dx, y: y + dy });
      }
    }
    return out;
  }

  /**
   * Runs `change`, recording the cells from before it so `undo` can bring them back. Nothing is
   * recorded when the cells end up the same.
   */
  edit(change: (grid: this) => void): boolean {
    const before = cloneJson(this.data);
    change(this);
    if (JSON.stringify(before) === JSON.stringify(this.data)) return false;
    this.history.record(before);
    return true;
  }

  undo(): boolean {
    const prev = this.history.undo(this.data);
    if (!prev) return false;
    this.data = prev;
    return true;
  }

  redo(): boolean {
    const next = this.history.redo(this.data);
    if (!next) return false;
    this.data = next;
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

  /** Moves the cursor to (x, y). Returns false when that is off the grid. */
  setCursor(x: number, y: number): boolean {
    if (!this.inBounds(x, y)) return false;
    this.cursor = { x, y };
    return true;
  }

  /** Moves the cursor by (dx, dy), wrapping when `wrap` is on. Returns true if it moved. */
  moveCursor(dx: number, dy: number): boolean {
    let x = this.cursor.x + dx;
    let y = this.cursor.y + dy;
    if (this.wrap) {
      x = ((x % this.width) + this.width) % this.width;
      y = ((y % this.height) + this.height) % this.height;
    } else {
      x = Math.max(0, Math.min(this.width - 1, x));
      y = Math.max(0, Math.min(this.height - 1, y));
    }
    if (x === this.cursor.x && y === this.cursor.y) return false;
    this.cursor = { x, y };
    return true;
  }

  /**
   * Applies a navigation action from `keyAction`: arrows move one cell, Home and End go to the
   * ends of the row, first and last (Ctrl+Home, Ctrl+End) to the corners, PageUp and PageDown to
   * the top and bottom of the column. Returns true if the cursor moved; other actions return false.
   */
  navigate(action: KeyAction): boolean {
    const { x, y } = this.cursor;
    switch (action) {
      case "up":
        return this.moveCursor(0, -1);
      case "down":
        return this.moveCursor(0, 1);
      case "left":
        return this.moveCursor(-1, 0);
      case "right":
        return this.moveCursor(1, 0);
      case "home":
        return this.jump(0, y);
      case "end":
        return this.jump(this.width - 1, y);
      case "first":
        return this.jump(0, 0);
      case "last":
        return this.jump(this.width - 1, this.height - 1);
      case "pageUp":
        return this.jump(x, 0);
      case "pageDown":
        return this.jump(x, this.height - 1);
      default:
        return false;
    }
  }

  private jump(x: number, y: number): boolean {
    if (x === this.cursor.x && y === this.cursor.y) return false;
    return this.setCursor(x, y);
  }

  isSelected(x: number, y: number): boolean {
    return this.inBounds(x, y) && this.chosen.has(this.index(x, y));
  }

  /** Adds (x, y) to the selection, or removes it when `on` is false. */
  select(x: number, y: number, on = true): void {
    if (!this.inBounds(x, y)) return;
    if (on) this.chosen.add(this.index(x, y));
    else this.chosen.delete(this.index(x, y));
  }

  /** Flips (x, y) in or out of the selection; returns whether it is now selected. */
  toggleSelect(x: number, y: number): boolean {
    this.select(x, y, !this.isSelected(x, y));
    return this.isSelected(x, y);
  }

  clearSelection(): void {
    this.chosen.clear();
  }

  /** Selected positions in reading order. */
  selection(): Point[] {
    return [...this.chosen].sort((a, b) => a - b).map((i) => this.point(i));
  }

  /** Shuffles all cells with a seeded generator. Recorded for undo. */
  shuffle(rng: Rng): void {
    this.edit(() => {
      this.data = rng.shuffle(this.data);
    });
  }

  /** Everything as plain JSON, safe to hand to `LullshiftBridge.autosave`. */
  snapshot(): GridState<C> {
    return {
      width: this.width,
      height: this.height,
      cells: cloneJson(this.data),
      cursor: { ...this.cursor },
      selected: [...this.chosen].sort((a, b) => a - b),
      history: this.history.snapshot(),
    };
  }

  /**
   * Loads a snapshot. The size must match this grid, and every cell must be JSON and pass
   * `isCell` when given. Returns false and changes nothing when anything is off. A missing
   * history or selection is treated as empty.
   */
  restore(state: unknown, isCell?: (value: unknown) => boolean): boolean {
    if (!isRecord(state) || state.width !== this.width || state.height !== this.height)
      return false;
    const okCell = (v: unknown) => v !== undefined && isJson(v) && (!isCell || isCell(v));
    const okCells = (v: unknown) =>
      Array.isArray(v) && v.length === this.size && v.every((c) => okCell(c));
    if (!okCells(state.cells)) return false;
    const cursor = state.cursor;
    if (!isRecord(cursor) || !this.inBounds(cursor.x as number, cursor.y as number)) return false;
    const selected = state.selected === undefined ? [] : state.selected;
    if (!Array.isArray(selected) || !selected.every((i) => isIntIn(i, 0, this.size - 1))) {
      return false;
    }
    const history = new History<C[]>(this.history.limit);
    if (state.history !== undefined && !history.restore(state.history, okCells)) return false;
    this.data = cloneJson(state.cells as C[]);
    this.cursor = { x: cursor.x as number, y: cursor.y as number };
    this.chosen = new Set(selected as number[]);
    this.history = history;
    return true;
  }
}

/** A grid filled by a seeded generator: `cell(rng, x, y)` is called in reading order. */
export function generateGrid<C>(
  width: number,
  height: number,
  rng: Rng,
  cell: (rng: Rng, x: number, y: number) => C,
  options: GridOptions = {},
): Grid<C> {
  return new Grid<C>(width, height, (x, y) => cell(rng, x, y), options);
}

/**
 * `count` distinct cell positions chosen with a seeded generator, for scattering mines, gems or
 * lanterns. `where` limits the candidates. Returns fewer when not enough cells qualify.
 */
export function scatter<C>(
  grid: Grid<C>,
  rng: Rng,
  count: number,
  where?: (cell: C, x: number, y: number) => boolean,
): Point[] {
  const candidates: number[] = [];
  for (let i = 0; i < grid.size; i++) {
    const p = grid.point(i);
    if (!where || where(grid.cells[i] as C, p.x, p.y)) candidates.push(i);
  }
  return rng
    .shuffle(candidates)
    .slice(0, Math.max(0, Math.floor(count)))
    .sort((a, b) => a - b)
    .map((i) => grid.point(i));
}
