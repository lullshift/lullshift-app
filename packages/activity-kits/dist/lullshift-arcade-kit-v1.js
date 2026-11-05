// Lullshift arcade kit v1 (ADR-019), from @lullshift/activity-kits 1.0.0.
// Licensed under the Apache License, Version 2.0. Copyright the Lullshift authors.
//
// GENERATED, DO NOT EDIT. Built by packages/activity-kits/scripts/build-kits.ts from
// packages/activity-kits/{src/core/random.ts, src/core/json.ts, src/core/history.ts, src/core/keys.ts, src/arcade/loop.ts, src/arcade/session.ts}.
//
// Copy this file next to index.html (`lullshift create --kit` does it) and load it as a
// classic script after the bridge and before your own code:
//   <script src="lullshift-bridge-v1.js"></script>
//   <script src="lullshift-arcade-kit-v1.js"></script>
//   <script src="main.js"></script>
// It adds window.LullshiftKits.arcade. Kits never save anything; save through the bridge.

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

  // src/arcade/loop.ts
  // Arcade kit (ADR-019): a fixed-step game loop on requestAnimationFrame that stops completely while
  // paused or hidden, a resume countdown (a static "Ready" prompt in reduced-motion mode), and a
  // helper that wires both to LullshiftBridge. Game state stays the game's own plain JSON.

  function defaultFrames() {
    return {
      request: (cb) => requestAnimationFrame(cb),
      cancel: (id) => cancelAnimationFrame(id),
    };
  }

  function defaultVisibility() {
    return typeof document === "undefined" ? null : document;
  }

  class Loop {
    /** Milliseconds of game time played; only advances in `update` steps. */
    elapsed = 0;
    steps = 0;
    stepMs;
    options;
    frames;
    visibility;
    frameId = null;
    last = null;
    acc = 0;
    isPaused = true;
    listening = false;
    onVisibility = () => {
      if (!this.visibility) return;
      if (this.visibility.hidden) {
        this.pause();
        this.options.onHidden?.();
      } else {
        this.options.onVisible?.();
      }
    };

    constructor(options) {
      this.options = options;
      const step = options.stepMs ?? 1000 / 60;
      this.stepMs = Number.isFinite(step) && step >= 1 ? step : 1000 / 60;
      this.frames = options.frames ?? defaultFrames();
      this.visibility = options.visibility === undefined ? defaultVisibility() : options.visibility;
    }

    /** True while paused (also before the first `resume`). Time does not advance then. */
    get paused() {
      return this.isPaused;
    }

    /** Starts or continues the loop. Does nothing while the page is hidden. */
    resume() {
      if (!this.listening && this.visibility) {
        this.visibility.addEventListener("visibilitychange", this.onVisibility);
        this.listening = true;
      }
      if (this.visibility?.hidden) return;
      if (!this.isPaused) return;
      this.isPaused = false;
      this.last = null;
      this.schedule();
    }

    /** Stops frames now. Time does not advance until `resume`. */
    pause() {
      this.isPaused = true;
      this.last = null;
      if (this.frameId !== null) {
        this.frames.cancel(this.frameId);
        this.frameId = null;
      }
    }

    /** Pauses and stops listening for visibility. `resume` starts again. */
    stop() {
      this.pause();
      if (this.listening && this.visibility) {
        this.visibility.removeEventListener("visibilitychange", this.onVisibility);
      }
      this.listening = false;
    }

    /** Played time and steps, as plain JSON. */
    snapshot() {
      return { elapsed: this.elapsed, steps: this.steps };
    }

    /** Loads played time. Returns false and changes nothing for bad input. */
    restore(state) {
      if (!isRecord(state)) return false;
      const { elapsed, steps } = state;
      if (typeof elapsed !== "number" || !Number.isFinite(elapsed) || elapsed < 0) return false;
      if (!Number.isInteger(steps) || steps < 0) return false;
      this.elapsed = elapsed;
      this.steps = steps;
      this.acc = 0;
      return true;
    }

    schedule() {
      this.frameId = this.frames.request((t) => this.frame(t));
    }

    frame(time) {
      this.frameId = null;
      if (this.isPaused) return;
      if (this.visibility?.hidden) {
        this.onVisibility();
        return;
      }
      if (this.last === null || time < this.last) this.last = time;
      this.acc += time - this.last;
      this.last = time;
      const max = Math.max(1, Math.floor(this.options.maxStepsPerFrame ?? 5));
      let n = 0;
      while (this.acc >= this.stepMs && n < max) {
        this.options.update(this.stepMs / 1000);
        this.elapsed += this.stepMs;
        this.steps += 1;
        this.acc -= this.stepMs;
        n += 1;
        if (this.isPaused) break; // the game paused itself in update
      }
      if (this.acc >= this.stepMs) this.acc %= this.stepMs;
      this.options.render?.(this.acc / this.stepMs);
      if (!this.isPaused) this.schedule();
    }
  }

  // src/arcade/session.ts
  // Arcade kit: the resume countdown and the LullshiftBridge hook-up (ADR-019). The host owns
  // interruption: when it pauses, the loop stops at once; play only starts again after a visible
  // countdown, or, in reduced-motion mode, after the player presses Enter or Space.

  /** Shown instead of a countdown when reduced motion is on. */
  const READY_PROMPT = "Ready. Press Enter or Space to play.";
  /** Shown while paused. */
  const PAUSED_PROMPT = "Paused. Press Enter or Space to resume.";

  function defaultTimers() {
    return {
      set: (cb, ms) => setTimeout(cb, ms),
      clear: (id) => clearTimeout(id),
    };
  }

  /** The step between "paused" and "playing": 3, 2, 1, or a static Ready prompt. */
  class ResumeGate {
    reducedMotion;
    options;
    timers;
    timer = null;
    waiting = false;

    constructor(options) {
      this.options = options;
      this.reducedMotion = !!options.reducedMotion;
      this.timers = options.timers ?? defaultTimers();
    }

    /** True while counting down or waiting for the player to confirm. */
    get active() {
      return this.waiting;
    }

    /** Begins the countdown, or shows the Ready prompt in reduced-motion mode. Restarts if active. */
    start() {
      this.cancel();
      this.waiting = true;
      if (this.reducedMotion) {
        this.options.show(READY_PROMPT);
        return;
      }
      const seconds = Math.max(0, Math.min(10, Math.floor(this.options.seconds ?? 3)));
      this.tick(seconds);
    }

    /**
     * For Enter and Space: in reduced-motion mode it starts play. During a countdown it does nothing.
     * Returns true when the gate is active, so the key should not reach the game.
     */
    confirm() {
      if (!this.waiting) return false;
      if (this.reducedMotion) this.finish();
      return true;
    }

    /** Stops without starting play and hides the text. */
    cancel() {
      if (this.timer !== null) this.timers.clear(this.timer);
      this.timer = null;
      if (this.waiting) this.options.show("");
      this.waiting = false;
    }

    tick(n) {
      if (n <= 0) {
        this.finish();
        return;
      }
      this.options.show(String(n));
      this.timer = this.timers.set(() => {
        this.timer = null;
        this.tick(n - 1);
      }, 1000);
    }

    finish() {
      this.timer = null;
      this.waiting = false;
      this.options.show("");
      this.options.done();
    }
  }

  /**
   * Starts the bridge and runs the game: init, countdown, play; pause on host pause, hidden page or
   * player request; autosave during play and on every pause.
   */
  function runArcade(options) {
    let reducedMotion = false;
    let ready = false;
    let hostPaused = false;
    let sinceSave = 0;
    // Whether play is on, as the game sees it. The loop may already have paused itself (hidden page).
    let playing = false;
    const autosaveMs = Math.max(0, (options.autosaveSeconds ?? 2) * 1000);

    const saveNow = () => {
      const state = options.save();
      if (state !== undefined && state !== null) options.bridge.autosave(state);
    };

    const loop = new Loop({
      update(dt) {
        options.update(dt);
        sinceSave += dt * 1000;
        if (autosaveMs > 0 && sinceSave >= autosaveMs) {
          sinceSave = 0;
          saveNow();
        }
      },
      render: options.render,
      stepMs: options.stepMs,
      maxStepsPerFrame: options.maxStepsPerFrame,
      frames: options.frames,
      visibility: options.visibility,
      onHidden: () => stopped(),
      onVisible: () => {
        if (ready && !hostPaused) options.prompt(PAUSED_PROMPT);
      },
    });

    const gate = new ResumeGate({
      show: options.prompt,
      seconds: options.countdownSeconds,
      timers: options.timers,
      done: () => {
        loop.resume();
        if (!loop.paused) {
          playing = true;
          options.onPausedChange?.(false);
        }
      },
    });

    function stopped(showPrompt = true) {
      const wasPlaying = playing;
      playing = false;
      loop.pause();
      gate.cancel();
      if (showPrompt && ready) options.prompt(PAUSED_PROMPT);
      if (wasPlaying) {
        saveNow();
        options.onPausedChange?.(true);
      }
    }

    const session = {
      loop,
      gate,
      get reducedMotion() {
        return reducedMotion;
      },
      get paused() {
        return loop.paused;
      },
      pause: () => stopped(),
      resume() {
        if (!ready || hostPaused || !loop.paused) return;
        gate.start();
      },
      confirm() {
        if (gate.active) return gate.confirm();
        if (ready && !hostPaused && loop.paused) {
          session.resume();
          return true;
        }
        return false;
      },
      saveNow,
    };

    options.bridge.start({
      onInit(info) {
        reducedMotion = !!info.reducedMotion;
        gate.reducedMotion = reducedMotion;
        options.onInit(info);
        options.render?.(0);
        ready = true;
        session.resume();
      },
      saveState: () => options.save(),
      onPause() {
        hostPaused = true;
        stopped(false);
      },
      onResume() {
        hostPaused = false;
        session.resume();
      },
    });
    return session;
  }

  var kits = (window.LullshiftKits = window.LullshiftKits || {});
  kits.arcade = Object.freeze({
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
    Loop,
    READY_PROMPT,
    PAUSED_PROMPT,
    ResumeGate,
    runArcade,
  });
})();
