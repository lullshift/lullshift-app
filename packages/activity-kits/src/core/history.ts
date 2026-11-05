// Undo and redo by snapshots (ADR-019). Before a change, record the state you are leaving; undo
// hands it back and keeps the current state for redo. Entries are plain JSON so the whole history
// can go into saved state. The oldest entries drop off past `limit`.

import { cloneJson, isRecord } from "./json.ts";

export interface HistoryState<T> {
  undo: T[];
  redo: T[];
}

/** Most total weight (cells, cards) of history entries that `restore` will check and keep. */
export const HISTORY_RESTORE_BUDGET = 100_000;

export class History<T> {
  readonly limit: number;
  private past: T[] = [];
  private future: T[] = [];

  /** `limit`: how many undo steps to keep (1 to 10000, default 100). */
  constructor(limit = 100) {
    this.limit = Math.max(1, Math.min(10000, Math.floor(limit) || 100));
  }

  /** Remember `before`, the state just before a change. Clears redo. */
  record(before: T): void {
    this.past.push(cloneJson(before));
    if (this.past.length > this.limit) this.past.splice(0, this.past.length - this.limit);
    this.future = [];
  }

  /** The state to go back to, or undefined. `current` is kept for redo. */
  undo(current: T): T | undefined {
    const prev = this.past.pop();
    if (prev === undefined) return undefined;
    this.future.push(cloneJson(current));
    return cloneJson(prev);
  }

  /** The state to go forward to, or undefined. `current` is kept for undo. */
  redo(current: T): T | undefined {
    const next = this.future.pop();
    if (next === undefined) return undefined;
    this.past.push(cloneJson(current));
    return cloneJson(next);
  }

  canUndo(): boolean {
    return this.past.length > 0;
  }

  canRedo(): boolean {
    return this.future.length > 0;
  }

  /** Number of undo steps available. */
  size(): number {
    return this.past.length;
  }

  clear(): void {
    this.past = [];
    this.future = [];
  }

  /** Plain JSON for saved state. */
  snapshot(): HistoryState<T> {
    return { undo: cloneJson(this.past), redo: cloneJson(this.future) };
  }

  /**
   * Loads a snapshot. Saved state is untrusted, so the work is bounded before any entry is
   * checked: only the newest `limit` entries of each list are kept, and, when `weigh` is given
   * (a cheap size estimate such as a cell or card count), only the newest entries whose total
   * weight fits in HISTORY_RESTORE_BUDGET. Dropped entries are the oldest undo steps and the
   * furthest redo steps. Every kept entry must pass `isEntry`; returns false, changing nothing,
   * when the input is malformed.
   */
  restore(
    state: unknown,
    isEntry: (entry: unknown) => boolean,
    weigh?: (entry: unknown) => number,
  ): boolean {
    if (!isRecord(state) || !Array.isArray(state.undo) || !Array.isArray(state.redo)) return false;
    let budget = HISTORY_RESTORE_BUDGET;
    const newest = (list: unknown[]): unknown[] => {
      const kept: unknown[] = [];
      for (let i = list.length - 1; i >= 0 && kept.length < this.limit; i--) {
        const e = list[i];
        if (weigh) {
          const w = weigh(e);
          if (!(w >= 0) || w > budget) break;
          budget -= w;
        }
        kept.push(e);
      }
      return kept.reverse();
    };
    // Undo first: the steps right behind the current state matter most.
    const undo = newest(state.undo);
    const redo = newest(state.redo);
    for (const e of undo) if (e === undefined || !isEntry(e)) return false;
    for (const e of redo) if (e === undefined || !isEntry(e)) return false;
    this.past = cloneJson(undo as T[]);
    this.future = cloneJson(redo as T[]);
    return true;
  }
}
