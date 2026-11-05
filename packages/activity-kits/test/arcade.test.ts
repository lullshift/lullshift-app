import { describe, expect, it } from "vitest";
import {
  Loop,
  PAUSED_PROMPT,
  READY_PROMPT,
  ResumeGate,
  runArcade,
  type ArcadeInit,
  type BridgeLike,
  type FrameScheduler,
  type Timers,
  type VisibilitySource,
} from "../src/arcade/index.ts";

/** A fake requestAnimationFrame: `run(t)` delivers one frame at time t. */
function fakeFrames() {
  let next = 1;
  const queued = new Map<number, (t: number) => void>();
  const frames: FrameScheduler = {
    request(cb) {
      queued.set(next, cb);
      return next++;
    },
    cancel(id) {
      queued.delete(id);
    },
  };
  return {
    frames,
    pending: () => queued.size,
    run(t: number) {
      const cbs = [...queued.values()];
      queued.clear();
      for (const cb of cbs) cb(t);
    },
  };
}

function fakeVisibility() {
  const listeners = new Set<() => void>();
  const v = {
    hidden: false,
    addEventListener: (_t: string, fn: () => void) => listeners.add(fn),
    removeEventListener: (_t: string, fn: () => void) => listeners.delete(fn),
    set(hidden: boolean) {
      v.hidden = hidden;
      for (const fn of listeners) fn();
    },
    listeners,
  };
  return v as typeof v & VisibilitySource;
}

/** Fake timers: `advance(ms)` fires what is due. */
function fakeTimers() {
  let now = 0;
  let next = 1;
  const due = new Map<number, { at: number; cb: () => void }>();
  const timers: Timers = {
    set(cb, ms) {
      due.set(next, { at: now + ms, cb });
      return next++;
    },
    clear(id) {
      due.delete(id);
    },
  };
  return {
    timers,
    advance(ms: number) {
      const end = now + ms;
      for (;;) {
        const first = [...due.entries()].sort((a, b) => a[1].at - b[1].at)[0];
        if (!first || first[1].at > end) break;
        due.delete(first[0]);
        now = first[1].at;
        first[1].cb();
      }
      now = end;
    },
    pending: () => due.size,
  };
}

describe("fixed-step loop", () => {
  function setup() {
    const f = fakeFrames();
    const vis = fakeVisibility();
    const dts: number[] = [];
    const alphas: number[] = [];
    const loop = new Loop({
      update: (dt) => dts.push(dt),
      render: (a) => alphas.push(a),
      stepMs: 10,
      frames: f.frames,
      visibility: vis,
    });
    return { f, vis, dts, alphas, loop };
  }

  it("runs fixed steps from frame time", () => {
    const { f, dts, alphas, loop } = setup();
    expect(loop.paused).toBe(true);
    loop.resume();
    f.run(1000); // first frame only sets the clock
    expect(dts).toEqual([]);
    f.run(1025);
    expect(dts).toEqual([0.01, 0.01]);
    expect(alphas.at(-1)).toBeCloseTo(0.5);
    f.run(1030);
    expect(loop.steps).toBe(3);
    expect(loop.elapsed).toBe(30);
  });

  it("caps catch-up after a long frame", () => {
    const { f, loop } = setup();
    loop.resume();
    f.run(0);
    f.run(10_000);
    expect(loop.steps).toBe(5);
  });

  it("does not run or advance time while paused", () => {
    const { f, loop } = setup();
    loop.resume();
    f.run(0);
    f.run(20);
    loop.pause();
    expect(f.pending()).toBe(0);
    f.run(5000);
    expect(loop.elapsed).toBe(20);
    loop.resume();
    f.run(60_000); // resets the clock: no jump for the paused time
    f.run(60_010);
    expect(loop.elapsed).toBe(30);
  });

  it("pauses itself when the page is hidden and stays paused", () => {
    const f = fakeFrames();
    const vis = fakeVisibility();
    let hidden = 0;
    let visible = 0;
    const loop = new Loop({
      update: () => {},
      stepMs: 10,
      frames: f.frames,
      visibility: vis,
      onHidden: () => hidden++,
      onVisible: () => visible++,
    });
    loop.resume();
    f.run(0);
    f.run(10);
    vis.set(true);
    expect(loop.paused).toBe(true);
    expect(hidden).toBe(1);
    f.run(10_000);
    expect(loop.elapsed).toBe(10);
    loop.resume(); // ignored while hidden
    expect(loop.paused).toBe(true);
    vis.set(false);
    expect(visible).toBe(1);
    expect(loop.paused).toBe(true);
    loop.stop();
    expect(vis.listeners.size).toBe(0);
  });

  it("notices a hidden page at frame time too", () => {
    const { f, vis, loop } = setup();
    loop.resume();
    f.run(0);
    vis.hidden = true; // no event delivered
    f.run(50);
    expect(loop.elapsed).toBe(0);
    expect(loop.paused).toBe(true);
  });

  it("snapshots and restores played time", () => {
    const { f, loop } = setup();
    loop.resume();
    f.run(0);
    f.run(30);
    const saved = JSON.parse(JSON.stringify(loop.snapshot())) as unknown;
    const { loop: other } = setup();
    expect(other.restore(saved)).toBe(true);
    expect(other.snapshot()).toEqual({ elapsed: 30, steps: 3 });
    for (const bad of [
      null,
      { elapsed: -1, steps: 0 },
      { elapsed: 1, steps: 1.5 },
      { elapsed: "1", steps: 1 },
    ]) {
      expect(other.restore(bad)).toBe(false);
    }
    expect(other.snapshot()).toEqual({ elapsed: 30, steps: 3 });
  });
});

describe("resume gate", () => {
  it("counts down 3, 2, 1, then starts", () => {
    const t = fakeTimers();
    const shown: string[] = [];
    let done = 0;
    const gate = new ResumeGate({
      show: (s) => shown.push(s),
      done: () => done++,
      timers: t.timers,
    });
    gate.start();
    expect(gate.active).toBe(true);
    expect(gate.confirm()).toBe(true); // swallowed, does not skip
    t.advance(2999);
    expect(done).toBe(0);
    t.advance(1);
    expect(shown).toEqual(["3", "2", "1", ""]);
    expect(done).toBe(1);
    expect(gate.active).toBe(false);
    expect(gate.confirm()).toBe(false);
  });

  it("shows a static Ready prompt in reduced-motion mode", () => {
    const t = fakeTimers();
    const shown: string[] = [];
    let done = 0;
    const gate = new ResumeGate({
      show: (s) => shown.push(s),
      done: () => done++,
      timers: t.timers,
      reducedMotion: true,
    });
    gate.start();
    t.advance(60_000);
    expect(shown).toEqual([READY_PROMPT]);
    expect(t.pending()).toBe(0);
    expect(done).toBe(0);
    expect(gate.confirm()).toBe(true);
    expect(done).toBe(1);
    expect(shown.at(-1)).toBe("");
  });

  it("cancels without starting", () => {
    const t = fakeTimers();
    let done = 0;
    const gate = new ResumeGate({ show: () => {}, done: () => done++, timers: t.timers });
    gate.start();
    t.advance(1500);
    gate.cancel();
    t.advance(10_000);
    expect(done).toBe(0);
    expect(t.pending()).toBe(0);
  });
});

describe("runArcade with the bridge", () => {
  function setup(reducedMotion: boolean) {
    const f = fakeFrames();
    const vis = fakeVisibility();
    const t = fakeTimers();
    let handlers: Parameters<BridgeLike["start"]>[0] = {};
    const saved: unknown[] = [];
    const bridge: BridgeLike = {
      start: (h) => (handlers = h),
      autosave: (s) => saved.push(s),
    };
    const game = { x: 0 };
    const prompts: string[] = [];
    let restored: unknown = null;
    const session = runArcade({
      bridge,
      onInit: (info: ArcadeInit) => {
        restored = info.restore;
        if (info.restore) game.x = (info.restore as { x: number }).x;
      },
      update: (dt) => (game.x += dt),
      save: () => ({ x: game.x }),
      prompt: (s) => prompts.push(s),
      autosaveSeconds: 0.05,
      stepMs: 10,
      frames: f.frames,
      visibility: vis,
      timers: t.timers,
    });
    const init = (restore: unknown = null) =>
      handlers.onInit?.({ restore, reducedMotion, theme: "light", content: null });
    return {
      f,
      vis,
      t,
      handlers: () => handlers,
      saved,
      game,
      prompts,
      session,
      init,
      restored: () => restored,
    };
  }

  it("counts down after init, then plays and autosaves", () => {
    const s = setup(false);
    s.init({ x: 5 });
    expect(s.restored()).toEqual({ x: 5 });
    expect(s.session.paused).toBe(true);
    s.t.advance(3000);
    expect(s.prompts).toEqual(["3", "2", "1", ""]);
    expect(s.session.paused).toBe(false);
    s.f.run(0);
    s.f.run(100);
    expect(s.game.x).toBeCloseTo(5.05);
    expect(s.saved.length).toBeGreaterThan(0);
  });

  it("waits for Enter or Space in reduced-motion mode", () => {
    const s = setup(true);
    s.init();
    expect(s.session.reducedMotion).toBe(true);
    s.t.advance(10_000);
    expect(s.session.paused).toBe(true);
    expect(s.prompts).toEqual([READY_PROMPT]);
    expect(s.session.confirm()).toBe(true);
    expect(s.session.paused).toBe(false);
  });

  it("stops at once on host pause, answers saveState, and resumes through the gate", () => {
    const s = setup(true);
    s.init();
    s.session.confirm();
    s.f.run(0);
    s.f.run(20);
    s.handlers().onPause?.();
    expect(s.session.paused).toBe(true);
    s.f.run(5000);
    expect(s.game.x).toBeCloseTo(0.02);
    expect(s.handlers().saveState?.()).toEqual({ x: s.game.x });
    expect(s.session.confirm()).toBe(false); // the host holds the pause
    s.handlers().onResume?.();
    expect(s.prompts.at(-1)).toBe(READY_PROMPT);
    s.session.confirm();
    expect(s.session.paused).toBe(false);
  });

  it("pauses on a hidden page and on player request, saving each time", () => {
    const s = setup(true);
    s.init();
    s.session.confirm();
    s.f.run(0);
    s.f.run(10);
    const before = s.saved.length;
    s.vis.set(true);
    expect(s.session.paused).toBe(true);
    expect(s.saved.length).toBe(before + 1);
    expect(s.prompts.at(-1)).toBe(PAUSED_PROMPT);
    s.vis.set(false);
    expect(s.session.paused).toBe(true);
    expect(s.session.confirm()).toBe(true); // Ready prompt
    expect(s.session.confirm()).toBe(true); // play
    expect(s.session.paused).toBe(false);
    s.session.pause();
    expect(s.session.paused).toBe(true);
    expect(s.prompts.at(-1)).toBe(PAUSED_PROMPT);
  });
});
