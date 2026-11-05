// Arcade kit (ADR-019): a fixed-step game loop on requestAnimationFrame that stops completely while
// paused or hidden, a resume countdown (a static "Ready" prompt in reduced-motion mode), and a
// helper that wires both to LullshiftBridge. Game state stays the game's own plain JSON.

import { finiteIn, isRecord } from "../core/json.ts";

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
  /** Step length in milliseconds (default 1000 / 60, 1 to 1000; anything else is the default). */
  stepMs?: number;
  /**
   * At most this many steps per frame; extra time is dropped, not caught up (default 5, 1 to 60;
   * anything else is the default).
   */
  maxStepsPerFrame?: number;
  frames?: FrameScheduler;
  /** null: never pause for visibility. */
  visibility?: VisibilitySource | null;
  /** The page was hidden and the loop paused itself. It stays paused until `resume()`. */
  onHidden?(): void;
  /** The page is visible again (the loop is still paused). */
  onVisible?(): void;
  /**
   * `update` or `render` threw. The loop has already paused itself, so `resume()` works again.
   * Without this handler the error is rethrown (after pausing).
   */
  onError?(error: unknown): void;
}

export interface LoopState {
  /** Milliseconds of game time played. */
  elapsed: number;
  steps: number;
}

function defaultFrames(): FrameScheduler {
  return {
    request: (cb) => requestAnimationFrame(cb),
    cancel: (id) => cancelAnimationFrame(id),
  };
}

function defaultVisibility(): VisibilitySource | null {
  return typeof document === "undefined" ? null : document;
}

export class Loop {
  /** Milliseconds of game time played; only advances in `update` steps. */
  elapsed = 0;
  steps = 0;
  readonly stepMs: number;
  readonly maxStepsPerFrame: number;
  private options: LoopOptions;
  private frames: FrameScheduler;
  private visibility: VisibilitySource | null;
  private frameId: number | null = null;
  private last: number | null = null;
  private acc = 0;
  private isPaused = true;
  private listening = false;
  private onVisibility = (): void => {
    if (!this.visibility) return;
    if (this.visibility.hidden) {
      this.pause();
      this.options.onHidden?.();
    } else {
      this.options.onVisible?.();
    }
  };

  constructor(options: LoopOptions) {
    this.options = options;
    this.stepMs = finiteIn(options.stepMs, 1, 1000, 1000 / 60);
    this.maxStepsPerFrame = Math.floor(finiteIn(options.maxStepsPerFrame, 1, 60, 5));
    this.frames = options.frames ?? defaultFrames();
    this.visibility = options.visibility === undefined ? defaultVisibility() : options.visibility;
    // Listen from the start, so a page hidden before the first resume (during a countdown) is seen.
    this.listen();
  }

  /** True while paused (also before the first `resume`). Time does not advance then. */
  get paused(): boolean {
    return this.isPaused;
  }

  /** Starts or continues the loop. Does nothing while the page is hidden. */
  resume(): void {
    this.listen();
    if (this.visibility?.hidden) return;
    if (!this.isPaused) return;
    this.isPaused = false;
    this.last = null;
    this.schedule();
  }

  /** Stops frames now. Time does not advance until `resume`. */
  pause(): void {
    this.isPaused = true;
    this.last = null;
    if (this.frameId !== null) {
      this.frames.cancel(this.frameId);
      this.frameId = null;
    }
  }

  /** Pauses and stops listening for visibility. `resume` starts again. */
  stop(): void {
    this.pause();
    if (this.listening && this.visibility) {
      this.visibility.removeEventListener("visibilitychange", this.onVisibility);
    }
    this.listening = false;
  }

  /** Played time and steps, as plain JSON. */
  snapshot(): LoopState {
    return { elapsed: this.elapsed, steps: this.steps };
  }

  /** Loads played time. Returns false and changes nothing for bad input. */
  restore(state: unknown): boolean {
    if (!isRecord(state)) return false;
    const { elapsed, steps } = state;
    if (typeof elapsed !== "number" || !Number.isFinite(elapsed) || elapsed < 0) return false;
    if (!Number.isInteger(steps) || (steps as number) < 0) return false;
    this.elapsed = elapsed;
    this.steps = steps as number;
    this.acc = 0;
    return true;
  }

  private listen(): void {
    if (!this.listening && this.visibility) {
      this.visibility.addEventListener("visibilitychange", this.onVisibility);
      this.listening = true;
    }
  }

  private schedule(): void {
    this.frameId = this.frames.request((t) => this.frame(t));
  }

  private frame(time: number): void {
    this.frameId = null;
    if (this.isPaused) return;
    if (this.visibility?.hidden) {
      this.onVisibility();
      return;
    }
    if (this.last === null || time < this.last) this.last = time;
    this.acc += time - this.last;
    this.last = time;
    try {
      let n = 0;
      while (this.acc >= this.stepMs && n < this.maxStepsPerFrame) {
        this.options.update(this.stepMs / 1000);
        this.elapsed += this.stepMs;
        this.steps += 1;
        this.acc -= this.stepMs;
        n += 1;
        if (this.isPaused) break; // the game paused itself in update
      }
      if (this.acc >= this.stepMs) this.acc %= this.stepMs;
      this.options.render?.(this.acc / this.stepMs);
    } catch (error) {
      // Never leave the loop half alive: pause, so resume() schedules frames again.
      this.acc = 0;
      this.pause();
      if (!this.options.onError) throw error;
      this.options.onError(error);
      return;
    }
    if (!this.isPaused) this.schedule();
  }
}
