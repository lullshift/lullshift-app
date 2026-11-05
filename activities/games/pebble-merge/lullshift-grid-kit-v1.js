// Lullshift grid kit v1 (ADR-019), from @lullshift/activity-kits 1.0.0.
// Licensed under the Apache License, Version 2.0. Copyright the Lullshift authors.
//
// GENERATED, DO NOT EDIT. Built by packages/activity-kits/scripts/build-kits.ts from
// packages/activity-kits/{src/core/random.ts, src/core/json.ts, src/core/history.ts, src/core/keys.ts, src/grid/grid.ts, src/grid/dom.ts}.
//
// Copy this file next to index.html (`lullshift create --kit` does it) and load it as a
// classic script after the bridge and before your own code:
//   <script src="lullshift-bridge-v1.js"></script>
//   <script src="lullshift-grid-kit-v1.js"></script>
//   <script src="main.js"></script>
// It adds window.LullshiftKits.grid. Kits never save anything; save through the bridge.

(function () {
  "use strict";
  // src/core/random.ts
  // Seeded random numbers (ADR-019). The generator is mulberry32 (Tommy Ettinger, public domain):
  // one 32-bit integer of state, fast, and good enough for shuffles and puzzle generation. It is not
  // for anything secret. The state is a plain number, so a game can save it and carry on exactly
  // where it left off.

  /** Creates a generator from a seed (any number; it is reduced to 32 bits). */
  function createRng(seed) {
    let a = toUint32(seed);
    const next = () => {
      a = (a + 0x6d2b79f5) >>> 0;
      let t = a;
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
    const int = (min, max) => {
      const lo = Math.ceil(Math.min(min, max));
      const hi = Math.floor(Math.max(min, max));
      return lo + Math.floor(next() * (hi - lo + 1));
    };
    return {
      next,
      int,
      pick(items) {
        if (items.length === 0) throw new RangeError("pick needs at least one item");
        return items[int(0, items.length - 1)];
      },
      shuffle(items) {
        const out = items.slice();
        for (let i = out.length - 1; i > 0; i--) {
          const j = int(0, i);
          const tmp = out[i];
          out[i] = out[j];
          out[j] = tmp;
        }
        return out;
      },
      state: () => a,
    };
  }

  /** Any number as an unsigned 32-bit integer; non-finite numbers become 0. */
  function toUint32(n) {
    return Number.isFinite(n) ? Math.trunc(n) >>> 0 : 0;
  }

  /** FNV-1a, 32-bit, over UTF-16 code units: a stable seed from a string. */
  function hashString(text) {
    let h = 0x811c9dc5;
    for (let i = 0; i < text.length; i++) {
      h ^= text.charCodeAt(i);
      h = Math.imul(h, 0x01000193) >>> 0;
    }
    return h >>> 0;
  }

  /** "YYYY-MM-DD" for the player's local calendar day. */
  function localDateKey(date) {
    const pad = (n) => String(n).padStart(2, "0");
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
  }

  /**
   * The seed of the day: everyone playing on the same local date gets the same puzzle. Pass a Date
   * (its local date counts) or a "YYYY-MM-DD" string. Optional `salt` gives each game its own
   * puzzle for the same day.
   */
  function dailySeed(date = new Date(), salt = "") {
    let key;
    if (typeof date === "string") {
      if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) throw new RangeError("dailySeed wants YYYY-MM-DD");
      key = date;
    } else {
      if (Number.isNaN(date.getTime())) throw new RangeError("dailySeed got an invalid date");
      key = localDateKey(date);
    }
    return hashString(salt ? `${salt}:${key}` : key);
  }

  /** A fresh, unpredictable seed for a "new game" (not for anything secret). */
  function randomSeed() {
    return Math.floor(Math.random() * 4294967296) >>> 0;
  }

  // src/core/json.ts
  // Plain JSON helpers. Saved state comes back from the host as untrusted data, so kits check its
  // shape before using it and copy it so later edits never alias the saved value.

  /** True for JSON values (finite numbers, strings, booleans, null, plain arrays and objects). */
  function isJson(value, depth = 0) {
    if (depth > 32) return false;
    if (value === null) return true;
    switch (typeof value) {
      case "string":
      case "boolean":
        return true;
      case "number":
        return Number.isFinite(value);
      case "object":
        if (Array.isArray(value)) return value.every((v) => isJson(v, depth + 1));
        if (Object.getPrototypeOf(value) !== Object.prototype) return false;
        return Object.keys(value).every((k) => k !== "__proto__" && isJson(value[k], depth + 1));
      default:
        return false;
    }
  }

  /** A deep copy of a JSON value. */
  function cloneJson(value) {
    return value === undefined ? value : JSON.parse(JSON.stringify(value));
  }

  /** True when `value` is an object that is not an array or null. */
  function isRecord(value) {
    return typeof value === "object" && value !== null && !Array.isArray(value);
  }

  /** True for an integer in [min, max]. */
  function isIntIn(value, min, max) {
    return Number.isInteger(value) && value >= min && value <= max;
  }

  /** `value` clamped to [min, max] when it is a finite number, otherwise `fallback`. */
  function finiteIn(value, min, max, fallback) {
    if (typeof value !== "number" || !Number.isFinite(value)) return fallback;
    return Math.max(min, Math.min(max, value));
  }

  // src/core/history.ts
  // Undo and redo by snapshots (ADR-019). Before a change, record the state you are leaving; undo
  // hands it back and keeps the current state for redo. Entries are plain JSON so the whole history
  // can go into saved state. The oldest entries drop off past `limit`.

  /** Most total weight (cells, cards) of history entries that `restore` will check and keep. */
  const HISTORY_RESTORE_BUDGET = 100_000;

  class History {
    limit;
    past = [];
    future = [];

    /** `limit`: how many undo steps to keep (1 to 10000, default 100). */
    constructor(limit = 100) {
      this.limit = Math.max(1, Math.min(10000, Math.floor(limit) || 100));
    }

    /** Remember `before`, the state just before a change. Clears redo. */
    record(before) {
      this.past.push(cloneJson(before));
      if (this.past.length > this.limit) this.past.splice(0, this.past.length - this.limit);
      this.future = [];
    }

    /** The state to go back to, or undefined. `current` is kept for redo. */
    undo(current) {
      const prev = this.past.pop();
      if (prev === undefined) return undefined;
      this.future.push(cloneJson(current));
      return cloneJson(prev);
    }

    /** The state to go forward to, or undefined. `current` is kept for undo. */
    redo(current) {
      const next = this.future.pop();
      if (next === undefined) return undefined;
      this.past.push(cloneJson(current));
      return cloneJson(next);
    }

    canUndo() {
      return this.past.length > 0;
    }

    canRedo() {
      return this.future.length > 0;
    }

    /** Number of undo steps available. */
    size() {
      return this.past.length;
    }

    clear() {
      this.past = [];
      this.future = [];
    }

    /** Plain JSON for saved state. */
    snapshot() {
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
    restore(state, isEntry, weigh) {
      if (!isRecord(state) || !Array.isArray(state.undo) || !Array.isArray(state.redo))
        return false;
      let budget = HISTORY_RESTORE_BUDGET;
      const newest = (list) => {
        const kept = [];
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
      this.past = cloneJson(undo);
      this.future = cloneJson(redo);
      return true;
    }
  }

  // src/core/keys.ts
  // Keys to actions (ADR-019). Named keys use `event.key`, so they work on every layout. Undo and
  // redo use the letter the player typed (`key`), which is where the shortcut is printed on their
  // keyboard. Optional WASD movement uses `event.code`, the physical position, so it is ZQSD on
  // AZERTY. Alt combinations and other Ctrl/Meta shortcuts are left to the browser.

  const NAMED = {
    ArrowUp: "up",
    ArrowDown: "down",
    ArrowLeft: "left",
    ArrowRight: "right",
    Up: "up",
    Down: "down",
    Left: "left",
    Right: "right",
    Home: "home",
    End: "end",
    PageUp: "pageUp",
    PageDown: "pageDown",
    Enter: "activate",
    " ": "activate",
    Spacebar: "activate",
    Escape: "cancel",
    Esc: "cancel",
  };

  const WASD = {
    KeyW: "up",
    KeyA: "left",
    KeyS: "down",
    KeyD: "right",
  };

  /**
   * The action for a key press, or null when the kit has no use for it (let it through then).
   * Ctrl/Cmd+Z is undo; Ctrl/Cmd+Y and Ctrl/Cmd+Shift+Z are redo; Ctrl/Cmd+Home and +End are
   * "first" and "last".
   */
  function keyAction(event, options = {}) {
    if (event.altKey) return null;
    const mod = !!(event.ctrlKey || event.metaKey);
    if (mod) {
      const k = event.key.toLowerCase();
      if (k === "z") return event.shiftKey ? "redo" : "undo";
      if (k === "y" && !event.shiftKey) return "redo";
      if (event.key === "Home") return "first";
      if (event.key === "End") return "last";
      return null;
    }
    const named = own(NAMED, event.key);
    if (named) return named;
    if (options.wasd && event.code && !event.shiftKey) return own(WASD, event.code);
    return null;
  }

  function own(map, key) {
    return Object.prototype.hasOwnProperty.call(map, key) ? map[key] : null;
  }

  // src/grid/grid.ts
  // Grid kit model (ADR-019): a rectangular board of plain JSON cells, a cursor, a selection,
  // keyboard navigation, undo by snapshots, seeded generators, and snapshot/restore of plain JSON.

  /** Largest width or height a grid accepts (also when restoring). */
  const GRID_MAX_SIDE = 256;

  class Grid {
    width;
    height;
    wrap;
    cursor = { x: 0, y: 0 };
    data;
    chosen = new Set();
    history;

    /** `fill` is a value for every cell, or a function of the cell position. */
    constructor(width, height, fill, options = {}) {
      if (!isIntIn(width, 1, GRID_MAX_SIDE) || !isIntIn(height, 1, GRID_MAX_SIDE)) {
        throw new RangeError(`grid sides are whole numbers from 1 to ${GRID_MAX_SIDE}`);
      }
      this.width = width;
      this.height = height;
      this.wrap = !!options.wrap;
      this.history = new History(options.historyLimit ?? 100);
      this.data = [];
      for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
          const v = typeof fill === "function" ? fill(x, y) : fill;
          this.data.push(cloneJson(v));
        }
      }
    }

    /** Number of cells. */
    get size() {
      return this.width * this.height;
    }

    /** The cells in reading order (row by row). Read-only: change cells with `set` or `edit`. */
    get cells() {
      return this.data;
    }

    inBounds(x, y) {
      return isIntIn(x, 0, this.width - 1) && isIntIn(y, 0, this.height - 1);
    }

    index(x, y) {
      return y * this.width + x;
    }

    point(index) {
      return { x: index % this.width, y: Math.floor(index / this.width) };
    }

    get(x, y) {
      return this.inBounds(x, y) ? this.data[this.index(x, y)] : undefined;
    }

    /** Sets one cell. Returns false when (x, y) is off the grid. Not recorded for undo by itself. */
    set(x, y, value) {
      if (!this.inBounds(x, y)) return false;
      this.data[this.index(x, y)] = value;
      return true;
    }

    /** Positions next to (x, y) on the grid; diagonals too when asked. Ignores `wrap`. */
    neighbours(x, y, diagonal = false) {
      const out = [];
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
    edit(change) {
      const before = cloneJson(this.data);
      change(this);
      if (JSON.stringify(before) === JSON.stringify(this.data)) return false;
      this.history.record(before);
      return true;
    }

    undo() {
      const prev = this.history.undo(this.data);
      if (!prev) return false;
      this.data = prev;
      return true;
    }

    redo() {
      const next = this.history.redo(this.data);
      if (!next) return false;
      this.data = next;
      return true;
    }

    canUndo() {
      return this.history.canUndo();
    }

    canRedo() {
      return this.history.canRedo();
    }

    clearHistory() {
      this.history.clear();
    }

    /** Moves the cursor to (x, y). Returns false when that is off the grid. */
    setCursor(x, y) {
      if (!this.inBounds(x, y)) return false;
      this.cursor = { x, y };
      return true;
    }

    /** Moves the cursor by (dx, dy), wrapping when `wrap` is on. Returns true if it moved. */
    moveCursor(dx, dy) {
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
    navigate(action) {
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

    jump(x, y) {
      if (x === this.cursor.x && y === this.cursor.y) return false;
      return this.setCursor(x, y);
    }

    isSelected(x, y) {
      return this.inBounds(x, y) && this.chosen.has(this.index(x, y));
    }

    /** Adds (x, y) to the selection, or removes it when `on` is false. */
    select(x, y, on = true) {
      if (!this.inBounds(x, y)) return;
      if (on) this.chosen.add(this.index(x, y));
      else this.chosen.delete(this.index(x, y));
    }

    /** Flips (x, y) in or out of the selection; returns whether it is now selected. */
    toggleSelect(x, y) {
      this.select(x, y, !this.isSelected(x, y));
      return this.isSelected(x, y);
    }

    clearSelection() {
      this.chosen.clear();
    }

    /** Selected positions in reading order. */
    selection() {
      return [...this.chosen].sort((a, b) => a - b).map((i) => this.point(i));
    }

    /** Shuffles all cells with a seeded generator. Recorded for undo. */
    shuffle(rng) {
      this.edit(() => {
        this.data = rng.shuffle(this.data);
      });
    }

    /** Everything as plain JSON, safe to hand to `LullshiftBridge.autosave`. */
    snapshot() {
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
     * history or selection is treated as empty; a long history is trimmed (see History.restore).
     */
    restore(state, isCell) {
      if (!isRecord(state) || state.width !== this.width || state.height !== this.height)
        return false;
      const okCell = (v) => v !== undefined && isJson(v) && (!isCell || isCell(v));
      const okCells = (v) =>
        Array.isArray(v) && v.length === this.size && v.every((c) => okCell(c));
      if (!okCells(state.cells)) return false;
      const cursor = state.cursor;
      if (!isRecord(cursor) || !this.inBounds(cursor.x, cursor.y)) return false;
      const selected = state.selected === undefined ? [] : state.selected;
      if (
        !Array.isArray(selected) ||
        selected.length > this.size ||
        !selected.every((i) => isIntIn(i, 0, this.size - 1))
      ) {
        return false;
      }
      const history = new History(this.history.limit);
      // Weight = cell count, so a hostile history can't make restore check millions of cells.
      const weigh = (v) => (Array.isArray(v) ? v.length : 0);
      if (state.history !== undefined && !history.restore(state.history, okCells, weigh)) {
        return false;
      }
      this.data = cloneJson(state.cells);
      this.cursor = { x: cursor.x, y: cursor.y };
      this.chosen = new Set(selected);
      this.history = history;
      return true;
    }
  }

  /** A grid filled by a seeded generator: `cell(rng, x, y)` is called in reading order. */
  function generateGrid(width, height, rng, cell, options = {}) {
    return new Grid(width, height, (x, y) => cell(rng, x, y), options);
  }

  /**
   * `count` distinct cell positions chosen with a seeded generator, for scattering mines, gems or
   * lanterns. `where` limits the candidates. Returns fewer when not enough cells qualify.
   */
  function scatter(grid, rng, count, where) {
    const candidates = [];
    for (let i = 0; i < grid.size; i++) {
      const p = grid.point(i);
      if (!where || where(grid.cells[i], p.x, p.y)) candidates.push(i);
    }
    return rng
      .shuffle(candidates)
      .slice(0, Math.max(0, Math.floor(count)))
      .sort((a, b) => a - b)
      .map((i) => grid.point(i));
  }

  // src/grid/dom.ts
  // Grid kit DOM binding (ADR-019): one button per cell inside an ARIA grid, a single tab stop that
  // follows the cursor (roving tabindex), a text label per cell, and keyboard navigation. Text is
  // set with textContent only; the binding never parses HTML.

  const NAVIGATION = new Set([
    "up",
    "down",
    "left",
    "right",
    "home",
    "end",
    "first",
    "last",
    "pageUp",
    "pageDown",
  ]);

  /** Renders `grid` into `container` (an empty element) and keeps it in step on `render()`. */
  function bindGrid(container, grid, options) {
    const doc = container.ownerDocument;
    const buttons = [];
    const gridcells = [];
    const cleanups = [];

    container.textContent = "";
    container.setAttribute("role", "grid");
    container.setAttribute("aria-label", options.name);
    container.setAttribute("aria-rowcount", String(grid.height));
    container.setAttribute("aria-colcount", String(grid.width));
    if (options.selectable) container.setAttribute("aria-multiselectable", "true");

    for (let y = 0; y < grid.height; y++) {
      const row = doc.createElement("div");
      row.setAttribute("role", "row");
      row.className = "kit-row";
      for (let x = 0; x < grid.width; x++) {
        const cell = doc.createElement("div");
        cell.setAttribute("role", "gridcell");
        cell.className = "kit-gridcell";
        const button = doc.createElement("button");
        button.type = "button";
        const onClick = () => {
          grid.setCursor(x, y);
          render();
          options.onActivate?.(x, y);
        };
        button.addEventListener("click", onClick);
        cleanups.push(() => button.removeEventListener("click", onClick));
        cell.appendChild(button);
        row.appendChild(cell);
        buttons.push(button);
        gridcells.push(cell);
      }
      container.appendChild(row);
    }

    /** Redraws from the model. If focus was in the grid, it follows the cursor. */
    function render() {
      const hadFocus = container.contains(doc.activeElement);
      const cursor = grid.index(grid.cursor.x, grid.cursor.y);
      for (let i = 0; i < buttons.length; i++) {
        const button = buttons[i];
        const { x, y } = grid.point(i);
        const value = grid.cells[i];
        button.textContent = options.text ? options.text(value, x, y) : "";
        button.setAttribute("aria-label", options.label(value, x, y));
        const extra = options.className ? options.className(value, x, y) : "";
        button.className = extra ? `kit-cell ${extra}` : "kit-cell";
        if (options.pressed) {
          button.setAttribute("aria-pressed", options.pressed(value, x, y) ? "true" : "false");
        }
        button.tabIndex = i === cursor ? 0 : -1;
        if (options.selectable) {
          const selected = grid.isSelected(x, y) ? "true" : "false";
          gridcells[i].setAttribute("aria-selected", selected);
        }
      }
      // A game's onAction, undo or redo may move the cursor; keep DOM focus on it.
      if (hadFocus && doc.activeElement !== buttons[cursor]) focus();
    }

    function focus() {
      buttons[grid.index(grid.cursor.x, grid.cursor.y)]?.focus();
    }

    const onKey = (event) => {
      const action = keyAction(event, options.keys);
      if (!action || action === "activate") return; // the focused button turns Enter/Space into a click
      if (NAVIGATION.has(action)) {
        event.preventDefault();
        if (grid.navigate(action)) {
          render();
          focus();
          options.onMove?.(grid.cursor.x, grid.cursor.y);
        }
        return;
      }
      if (options.onAction?.(action, event)) {
        event.preventDefault();
        render();
        return;
      }
      if (action === "undo" || action === "redo") {
        event.preventDefault();
        if (action === "undo" ? grid.undo() : grid.redo()) {
          render();
          options.onChange?.();
        }
      }
    };
    container.addEventListener("keydown", onKey);
    cleanups.push(() => container.removeEventListener("keydown", onKey));

    render();
    return {
      render,
      focus,
      destroy() {
        for (const c of cleanups) c();
        container.textContent = "";
      },
    };
  }

  var root = typeof window !== "undefined" ? window : globalThis;
  var kits = (root.LullshiftKits = root.LullshiftKits || {});
  kits.grid = Object.freeze({
    createRng,
    toUint32,
    hashString,
    localDateKey,
    dailySeed,
    randomSeed,
    isJson,
    cloneJson,
    isRecord,
    isIntIn,
    finiteIn,
    HISTORY_RESTORE_BUDGET,
    History,
    keyAction,
    GRID_MAX_SIDE,
    Grid,
    generateGrid,
    scatter,
    bindGrid,
  });
})();
