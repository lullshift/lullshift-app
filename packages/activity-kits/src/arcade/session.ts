// Arcade kit: the resume countdown and the LullshiftBridge hook-up (ADR-019). The host owns
// interruption: when it pauses, the loop stops at once; play only starts again after a visible
// countdown, or, in reduced-motion mode, after the player presses Enter or Space.

import { Loop, type FrameScheduler, type LoopOptions, type VisibilitySource } from "./loop.ts";

/** Shown instead of a countdown when reduced motion is on. */
export const READY_PROMPT = "Ready. Press Enter or Space to play.";
/** Shown while paused. */
export const PAUSED_PROMPT = "Paused. Press Enter or Space to resume.";

/** Timer functions; the default uses setTimeout. Tests pass their own. */
export interface Timers {
  set(callback: () => void, ms: number): number;
  clear(id: number): void;
}

function defaultTimers(): Timers {
  return {
    set: (cb, ms) => setTimeout(cb, ms) as unknown as number,
    clear: (id) => clearTimeout(id),
  };
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
  private options: ResumeGateOptions;
  private timers: Timers;
  private timer: number | null = null;
  private waiting = false;

  constructor(options: ResumeGateOptions) {
    this.options = options;
    this.reducedMotion = !!options.reducedMotion;
    this.timers = options.timers ?? defaultTimers();
  }

  /** True while counting down or waiting for the player to confirm. */
  get active(): boolean {
    return this.waiting;
  }

  /** Begins the countdown, or shows the Ready prompt in reduced-motion mode. Restarts if active. */
  start(): void {
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
  confirm(): boolean {
    if (!this.waiting) return false;
    if (this.reducedMotion) this.finish();
    return true;
  }

  /** Stops without starting play and hides the text. */
  cancel(): void {
    if (this.timer !== null) this.timers.clear(this.timer);
    this.timer = null;
    if (this.waiting) this.options.show("");
    this.waiting = false;
  }

  private tick(n: number): void {
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

  private finish(): void {
    this.timer = null;
    this.waiting = false;
    this.options.show("");
    this.options.done();
  }
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
export function runArcade(options: ArcadeOptions): ArcadeSession {
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

  function stopped(showPrompt = true): void {
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

  const session: ArcadeSession = {
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
