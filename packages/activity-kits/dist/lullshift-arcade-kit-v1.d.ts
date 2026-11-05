// Lullshift arcade kit v1 (ADR-019), from @lullshift/activity-kits 1.0.0.
// Licensed under the Apache License, Version 2.0. Copyright the Lullshift authors.
//
// GENERATED, DO NOT EDIT. Built by packages/activity-kits/scripts/build-kits.ts from
// packages/activity-kits/{src/core/random.ts, src/core/json.ts, src/core/history.ts, src/core/keys.ts, src/arcade/loop.ts, src/arcade/session.ts}.
//
// Types for window.LullshiftKits.arcade. Reference this file from a TypeScript activity:
//   /// <reference path="./lullshift-arcade-kit-v1.d.ts" />

declare namespace LullshiftKits {
  namespace arcade {
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

    /** Frame scheduling; the default uses requestAnimationFrame. Tests pass their own. */
    export interface FrameScheduler {
      request(callback: (time: number) => void): number;
      cancel(id: number): void;
    }
    /** Where the loop learns the page is hidden; the default is `document`. */
    export interface VisibilitySource {
      readonly hidden: boolean;
      addEventListener(type: "visibilitychange", listener: () => void): void;
      removeEventListener(type: "visibilitychange", listener: () => void): void;
    }
    export interface LoopOptions {
      /** Advances the game by one fixed step of `dt` seconds. */
      update(dt: number): void;
      /** Draws; `alpha` in [0, 1) is how far the clock is into the next step, for smoothing. */
      render?(alpha: number): void;
      /** Step length in milliseconds (default 1000 / 60). */
      stepMs?: number;
      /** At most this many steps per frame; extra time is dropped, not caught up (default 5). */
      maxStepsPerFrame?: number;
      frames?: FrameScheduler;
      /** null: never pause for visibility. */
      visibility?: VisibilitySource | null;
      /** The page was hidden and the loop paused itself. It stays paused until `resume()`. */
      onHidden?(): void;
      /** The page is visible again (the loop is still paused). */
      onVisible?(): void;
    }
    export interface LoopState {
      /** Milliseconds of game time played. */
      elapsed: number;
      steps: number;
    }
    export class Loop {
      /** Milliseconds of game time played; only advances in `update` steps. */
      elapsed: number;
      steps: number;
      readonly stepMs: number;
      private options;
      private frames;
      private visibility;
      private frameId;
      private last;
      private acc;
      private isPaused;
      private listening;
      private onVisibility;
      constructor(options: LoopOptions);
      /** True while paused (also before the first `resume`). Time does not advance then. */
      get paused(): boolean;
      /** Starts or continues the loop. Does nothing while the page is hidden. */
      resume(): void;
      /** Stops frames now. Time does not advance until `resume`. */
      pause(): void;
      /** Pauses and stops listening for visibility. `resume` starts again. */
      stop(): void;
      /** Played time and steps, as plain JSON. */
      snapshot(): LoopState;
      /** Loads played time. Returns false and changes nothing for bad input. */
      restore(state: unknown): boolean;
      private schedule;
      private frame;
    }

    /** Shown instead of a countdown when reduced motion is on. */
    export const READY_PROMPT = "Ready. Press Enter or Space to play.";
    /** Shown while paused. */
    export const PAUSED_PROMPT = "Paused. Press Enter or Space to resume.";
    /** Timer functions; the default uses setTimeout. Tests pass their own. */
    export interface Timers {
      set(callback: () => void, ms: number): number;
      clear(id: number): void;
    }
    export interface ResumeGateOptions {
      /** Show this text to the player (in an aria-live region); "" means hide it. */
      show(text: string): void;
      /** The countdown finished or the player confirmed: start play. */
      done(): void;
      reducedMotion?: boolean;
      /** Countdown length in seconds (default 3, 0 to 10). */
      seconds?: number;
      timers?: Timers;
    }
    /** The step between "paused" and "playing": 3, 2, 1, or a static Ready prompt. */
    export class ResumeGate {
      reducedMotion: boolean;
      private options;
      private timers;
      private timer;
      private waiting;
      constructor(options: ResumeGateOptions);
      /** True while counting down or waiting for the player to confirm. */
      get active(): boolean;
      /** Begins the countdown, or shows the Ready prompt in reduced-motion mode. Restarts if active. */
      start(): void;
      /**
       * For Enter and Space: in reduced-motion mode it starts play. During a countdown it does nothing.
       * Returns true when the gate is active, so the key should not reach the game.
       */
      confirm(): boolean;
      /** Stops without starting play and hides the text. */
      cancel(): void;
      private tick;
      private finish;
    }
    /** The parts of LullshiftBridge the session uses. */
    export interface BridgeLike {
      start(handlers: {
        onInit?(info: ArcadeInit): void;
        saveState?(): unknown;
        onPause?(): void;
        onResume?(): void;
      }): void;
      autosave(state: unknown): void;
    }
    /** What LullshiftBridge passes to onInit. */
    export interface ArcadeInit {
      restore: unknown;
      reducedMotion: boolean;
      theme: "light" | "dark";
      content: unknown;
    }
    export interface ArcadeOptions {
      /** Usually `window.LullshiftBridge`. */
      bridge: BridgeLike;
      /** Rebuild from `info.restore` (validate it first) and honour `info.reducedMotion`. */
      onInit(info: ArcadeInit): void;
      update: LoopOptions["update"];
      render?: LoopOptions["render"];
      /** The whole game state as plain JSON (any value except null). */
      save(): unknown;
      /** Show this text to the player (aria-live); "" hides it. */
      prompt(text: string): void;
      /** Play started (false) or stopped (true): update buttons and status. */
      onPausedChange?(paused: boolean): void;
      /** Autosave after this many seconds of play (default 2; 0 turns it off). */
      autosaveSeconds?: number;
      countdownSeconds?: number;
      stepMs?: number;
      maxStepsPerFrame?: number;
      frames?: FrameScheduler;
      visibility?: VisibilitySource | null;
      timers?: Timers;
    }
    export interface ArcadeSession {
      readonly loop: Loop;
      readonly gate: ResumeGate;
      /** From the host's init. Games must skip shakes, flashes and smooth motion when true. */
      readonly reducedMotion: boolean;
      /** True until play starts and whenever the game is paused. */
      readonly paused: boolean;
      /** Player pause (e.g. P or Escape): stops at once, saves, shows the paused prompt. */
      pause(): void;
      /** Starts the countdown (or Ready prompt) and then play. */
      resume(): void;
      /** Enter or Space: confirms the Ready prompt or resumes a pause. True if it used the key. */
      confirm(): boolean;
      /** Sends the current state to the host now. */
      saveNow(): void;
    }
    /**
     * Starts the bridge and runs the game: init, countdown, play; pause on host pause, hidden page or
     * player request; autosave during play and on every pause.
     */
    export function runArcade(options: ArcadeOptions): ArcadeSession;
  }
}

interface Window {
  readonly LullshiftKits: typeof LullshiftKits;
}
