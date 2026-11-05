// Lullshift card kit v1 (ADR-019), from @lullshift/activity-kits 1.0.0.
// Licensed under the Apache License, Version 2.0. Copyright the Lullshift authors.
//
// GENERATED, DO NOT EDIT. Built by packages/activity-kits/scripts/build-kits.ts from
// packages/activity-kits/{src/core/random.ts, src/core/json.ts, src/core/history.ts, src/core/keys.ts, src/card/cards.ts, src/card/dom.ts}.
//
// Copy this file next to index.html (`lullshift create --kit` does it) and load it as a
// classic script after the bridge and before your own code:
//   <script src="lullshift-bridge-v1.js"></script>
//   <script src="lullshift-card-kit-v1.js"></script>
//   <script src="main.js"></script>
// It adds window.LullshiftKits.card. Kits never save anything; save through the bridge.

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

  // src/core/history.ts
  // Undo and redo by snapshots (ADR-019). Before a change, record the state you are leaving; undo
  // hands it back and keeps the current state for redo. Entries are plain JSON so the whole history
  // can go into saved state. The oldest entries drop off past `limit`.

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
     * Loads a snapshot. Every entry must pass `isEntry`. Returns false, changing nothing, when the
     * input is malformed. Entries over the limit are dropped, oldest first.
     */
    restore(state, isEntry) {
      if (!isRecord(state) || !Array.isArray(state.undo) || !Array.isArray(state.redo))
        return false;
      if (state.undo.length > 10000 || state.redo.length > 10000) return false;
      for (const e of state.undo) if (e === undefined || !isEntry(e)) return false;
      for (const e of state.redo) if (e === undefined || !isEntry(e)) return false;
      this.past = cloneJson(state.undo.slice(-this.limit));
      this.future = cloneJson(state.redo.slice(-this.limit));
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

  // src/card/cards.ts
  // Card kit model (ADR-019): decks (standard or custom), seeded shuffles, named piles, moves
  // checked by a rules hook, undo, keyboard focus between piles and cards, a pick-up-and-place
  // selection, and snapshot/restore of plain JSON.

  const STANDARD_SUITS = ["spades", "hearts", "diamonds", "clubs"];
  const STANDARD_RANKS = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];

  /** Limits that `restore` enforces on saved state. */
  const CARD_LIMITS = { cards: 1000, piles: 100, text: 64 };

  /** Every suit and rank combination, face down, in order. Defaults to a 52-card deck. */
  function buildDeck(options = {}) {
    const suits = options.suits ?? STANDARD_SUITS;
    const ranks = options.ranks ?? STANDARD_RANKS;
    const copies = Math.max(1, Math.floor(options.copies ?? 1));
    const specs = [];
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
  function customDeck(specs) {
    const seen = new Set();
    return specs.map((s, n) => {
      const card = {
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
  function shuffleDeck(cards, rng) {
    return rng.shuffle(cards).map((c) => ({ ...c }));
  }

  /** "Ace", "2" ... "10", "Jack", "Queen", "King". */
  function rankName(rank) {
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
  function suitColor(suit) {
    if (suit === "hearts" || suit === "diamonds") return "red";
    if (suit === "spades" || suit === "clubs") return "black";
    return null;
  }

  /** What a screen reader hears: "Queen of hearts", the custom name, or "Face-down card". */
  function cardLabel(card) {
    if (!card.faceUp) return "Face-down card";
    return card.name ?? `${rankName(card.rank)} of ${card.suit}`;
  }

  /** True for a well-formed card. */
  function isCard(value) {
    if (!isRecord(value)) return false;
    const text = (v) => typeof v === "string" && v.length > 0 && v.length <= CARD_LIMITS.text;
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

  class Table {
    piles;
    focus = { pile: 0, card: -1 };
    selection = null;
    options;
    history;

    constructor(piles, options = {}) {
      if (piles.length < 1 || piles.length > CARD_LIMITS.piles) {
        throw new RangeError(`a table has 1 to ${CARD_LIMITS.piles} piles`);
      }
      const ids = new Set(piles.map((p) => p.id));
      if (ids.size !== piles.length) throw new RangeError("pile ids must be unique");
      this.piles = piles.map((p) => ({ id: p.id, name: p.name, cards: cloneJson(p.cards ?? []) }));
      this.options = options;
      this.history = new History(options.historyLimit ?? 100);
      this.focus = { pile: 0, card: this.topIndex(0) };
    }

    pile(id) {
      return this.piles.find((p) => p.id === id);
    }

    /** The top card of a pile, if any. */
    top(id) {
      const p = this.pile(id);
      return p ? p.cards[p.cards.length - 1] : undefined;
    }

    /** Whether moving the top `count` cards from `from` to `to` is allowed. */
    canMove(from, to, count = 1) {
      const move = this.proposal(from, to, count);
      return !!move && (!this.options.canMove || this.options.canMove(move, this));
    }

    /** Moves the top `count` cards if allowed. Recorded for undo. Returns false when refused. */
    move(from, to, count = 1) {
      if (!this.canMove(from, to, count)) return false;
      this.edit((t) => {
        const src = t.pile(from);
        const dst = t.pile(to);
        dst.cards.push(...src.cards.splice(src.cards.length - count, count));
        const newTop = src.cards[src.cards.length - 1];
        if (t.options.revealAfterMove && newTop) newTop.faceUp = true;
      });
      this.clampFocus();
      return true;
    }

    /** Deals the top `count` cards from one pile to another, ignoring rules. Recorded for undo. */
    deal(from, to, count = 1, faceUp = true) {
      const src = this.pile(from);
      const dst = this.pile(to);
      if (!src || !dst || src === dst) return 0;
      const n = Math.max(0, Math.min(Math.floor(count), src.cards.length));
      this.edit(() => {
        for (let i = 0; i < n; i++) {
          const card = src.cards.pop();
          card.faceUp = faceUp;
          dst.cards.push(card);
        }
      });
      this.clampFocus();
      return n;
    }

    /** Runs any change to the piles, recorded for undo when something changed. */
    edit(change) {
      const before = this.cardsOf();
      change(this);
      if (JSON.stringify(before) === JSON.stringify(this.cardsOf())) return false;
      this.history.record(before);
      this.selection = null;
      return true;
    }

    undo() {
      const prev = this.history.undo(this.cardsOf());
      if (!prev) return false;
      this.load(prev);
      return true;
    }

    redo() {
      const next = this.history.redo(this.cardsOf());
      if (!next) return false;
      this.load(next);
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

    /**
     * Keyboard focus: left and right move between piles (onto the top card), up and down between
     * cards in a pile, Home and End to the first and last pile, PageUp and PageDown to the bottom
     * and top card of the pile. Returns true if focus moved.
     */
    navigate(action) {
      const { pile, card } = this.focus;
      const len = this.piles[pile].cards.length;
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
    activate() {
      const { pile, card } = this.focus;
      const focused = this.piles[pile];
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
      const src = this.piles[sel.pile];
      if (!this.move(src.id, focused.id, src.cards.length - sel.card)) return "rejected";
      this.selection = null;
      this.focus = { pile, card: this.topIndex(pile) };
      return "moved";
    }

    /** Plain JSON for saved state. */
    snapshot() {
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
    restore(state) {
      if (!isRecord(state) || !Array.isArray(state.piles)) return false;
      const piles = state.piles;
      if (piles.length !== this.piles.length) return false;
      const cards = [];
      for (let i = 0; i < piles.length; i++) {
        const p = piles[i];
        if (!isRecord(p) || p.id !== this.piles[i].id) return false;
        cards.push(p.cards);
      }
      if (!this.validCards(cards)) return false;
      const history = new History(this.history.limit);
      if (
        state.history !== undefined &&
        !history.restore(state.history, (e) => this.validCards(e))
      ) {
        return false;
      }
      const spot = (v, c) =>
        isRecord(v) &&
        isIntIn(v.pile, 0, c.length - 1) &&
        isIntIn(v.card, -1, c[v.pile].length - 1);
      if (state.focus !== undefined && !spot(state.focus, cards)) return false;
      if (state.selection != null && !spot(state.selection, cards)) return false;
      this.load(cloneJson(cards));
      this.history = history;
      if (state.focus !== undefined) {
        const f = state.focus;
        this.focus = { pile: f.pile, card: f.card };
      }
      this.clampFocus();
      const s = state.selection;
      this.selection =
        s && this.piles[s.pile].cards[s.card] ? { pile: s.pile, card: s.card } : null;
      return true;
    }

    proposal(from, to, count) {
      const src = this.pile(from);
      const dst = this.pile(to);
      if (!src || !dst || src === dst || !isIntIn(count, 1, src.cards.length)) return null;
      const cards = src.cards.slice(src.cards.length - count);
      if (!cards.every((c) => c.faceUp)) return null;
      return { from, to, cards: cloneJson(cards) };
    }

    cardsOf() {
      return this.piles.map((p) => cloneJson(p.cards));
    }

    load(cards) {
      this.piles = this.piles.map((p, i) => ({ id: p.id, name: p.name, cards: cards[i] ?? [] }));
      this.selection = null;
      this.clampFocus();
    }

    validCards(value) {
      if (!Array.isArray(value) || value.length !== this.piles.length) return false;
      const ids = new Set();
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

    topIndex(pile) {
      return (this.piles[pile].cards.length || 0) - 1;
    }

    focusPile(pile) {
      if (pile < 0 || pile >= this.piles.length || pile === this.focus.pile) return false;
      this.focus = { pile, card: this.topIndex(pile) };
      return true;
    }

    focusCard(card) {
      const len = this.piles[this.focus.pile].cards.length;
      if (len === 0 || card < 0 || card >= len || card === this.focus.card) return false;
      this.focus = { pile: this.focus.pile, card };
      return true;
    }

    clampFocus() {
      const pile = Math.max(0, Math.min(this.piles.length - 1, this.focus.pile));
      const len = this.piles[pile].cards.length;
      const card = len === 0 ? -1 : Math.max(0, Math.min(len - 1, this.focus.card));
      this.focus = { pile, card: this.focus.card < 0 && len > 0 ? len - 1 : card };
    }
  }

  // src/card/dom.ts
  // Card kit DOM binding (ADR-019): each pile is a labelled group of card buttons, with a single
  // tab stop that follows the table's focus (roving tabindex). An empty pile shows one "Empty"
  // button so it can still be chosen as a target. Text is set with textContent only.

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

  /** Renders `table` into `container` (an empty element). */
  function bindTable(container, table, options) {
    const doc = container.ownerDocument;
    let current = null;
    const label = (card, pile) => (options.label ? options.label(card, pile) : cardLabel(card));

    container.setAttribute("role", "group");
    container.setAttribute("aria-label", options.name);

    function activate(pile, card) {
      table.focus = { pile, card };
      const p = table.piles[pile];
      const before = JSON.stringify(table.piles.map((x) => x.cards));
      if (!options.onActivate?.(p.id, card)) {
        const result = table.activate();
        options.onResult?.(result);
      }
      render();
      focus();
      if (JSON.stringify(table.piles.map((x) => x.cards)) !== before) options.onChange?.();
    }

    function cardButton(text, aria, pile, card) {
      const b = doc.createElement("button");
      b.type = "button";
      b.textContent = text;
      b.setAttribute("aria-label", aria);
      const focused = table.focus.pile === pile && table.focus.card === card;
      b.tabIndex = focused ? 0 : -1;
      if (focused) current = b;
      b.addEventListener("click", () => activate(pile, card));
      return b;
    }

    function render() {
      const hadFocus = container.contains(doc.activeElement);
      current = null;
      container.textContent = "";
      const sel = table.selection;
      table.piles.forEach((pile, p) => {
        const group = doc.createElement("div");
        group.setAttribute("role", "group");
        const n = pile.cards.length;
        group.setAttribute("aria-label", `${pile.name}, ${n === 1 ? "1 card" : `${n} cards`}`);
        group.className = "kit-pile";
        group.setAttribute("data-pile", pile.id);
        if (n === 0) {
          const b = cardButton("", `Empty ${pile.name}`, p, -1);
          b.className = "kit-card kit-empty";
          group.appendChild(b);
        }
        pile.cards.forEach((card, c) => {
          const picked = !!sel && sel.pile === p && c >= sel.card;
          const aria = label(card, pile) + (picked ? ", picked up" : "");
          const text = options.text
            ? options.text(card, pile)
            : card.faceUp
              ? label(card, pile)
              : "";
          const b = cardButton(text, aria, p, c);
          const extra = options.className ? options.className(card, pile) : "";
          b.className = [
            "kit-card",
            card.faceUp ? "kit-face-up" : "kit-face-down",
            picked ? "kit-picked" : "",
            extra,
          ]
            .filter(Boolean)
            .join(" ");
          b.setAttribute("aria-pressed", picked ? "true" : "false");
          group.appendChild(b);
        });
        container.appendChild(group);
      });
      if (hadFocus) focus();
    }

    function focus() {
      current?.focus();
    }

    const onKey = (event) => {
      const action = keyAction(event, options.keys);
      if (!action || action === "activate") return; // the focused button turns Enter/Space into a click
      if (NAVIGATION.has(action)) {
        event.preventDefault();
        if (table.navigate(action)) {
          render();
          focus();
        }
        return;
      }
      if (options.onAction?.(action, event)) {
        event.preventDefault();
        render();
        return;
      }
      if (action === "cancel" && table.selection) {
        event.preventDefault();
        table.selection = null;
        render();
        focus();
        return;
      }
      if (action === "undo" || action === "redo") {
        event.preventDefault();
        if (action === "undo" ? table.undo() : table.redo()) {
          render();
          focus();
          options.onChange?.();
        }
      }
    };
    container.addEventListener("keydown", onKey);

    render();
    return {
      render,
      focus,
      destroy() {
        container.removeEventListener("keydown", onKey);
        container.textContent = "";
      },
    };
  }

  var kits = (window.LullshiftKits = window.LullshiftKits || {});
  kits.card = Object.freeze({
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
    History,
    keyAction,
    STANDARD_SUITS,
    STANDARD_RANKS,
    CARD_LIMITS,
    buildDeck,
    customDeck,
    shuffleDeck,
    rankName,
    suitColor,
    cardLabel,
    isCard,
    Table,
    bindTable,
  });
})();
