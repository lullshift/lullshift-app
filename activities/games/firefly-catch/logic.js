// Pure rules for Firefly Catch. No DOM. Attaches its API to globalThis.FireflyLogic.
// All state is plain JSON; stepping uses a fixed dt so a snapshot restores exactly.
(function (root) {
  "use strict";
  var W = 400;
  var H = 300;
  var JAR_W = 56;
  var JAR_H = 40;
  var JAR_TOP = H - 50; // y of the jar mouth
  var JAR_SPEED = 170; // px per second
  var ROUND_SECONDS = 90;
  var MAX_FIREFLIES = 40;

  // mulberry32 whose whole state is the number stored in state.rng.
  function nextRandom(state) {
    state.rng = (state.rng + 0x6d2b79f5) >>> 0;
    var t = state.rng;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  }
  function range(state, lo, hi) {
    return lo + (hi - lo) * nextRandom(state);
  }

  // reduced: straight lines, slower, no curves.
  function create(seed, reduced) {
    return {
      version: 1,
      seed: seed >>> 0,
      rng: seed >>> 0,
      reduced: !!reduced,
      time: 0,
      timeLeft: ROUND_SECONDS,
      jarX: W / 2,
      caught: 0,
      best: 0,
      over: false,
      spawnIn: 0.5,
      flies: [],
    };
  }

  function spawn(state) {
    var speed = state.reduced ? 0.7 : 1;
    var x0 = range(state, 24, W - 24);
    state.flies.push({
      x0: x0,
      x: x0,
      y: -8,
      t: 0,
      vy: range(state, 28, 45) * speed,
      vx: range(state, -8, 8) * speed,
      amp: state.reduced ? 0 : range(state, 10, 28),
      freq: range(state, 0.8, 1.6),
      phase: range(state, 0, Math.PI * 2),
      away: false,
    });
  }

  function overlapsJar(state, f) {
    return Math.abs(f.x - state.jarX) <= JAR_W / 2 + 3 && f.y >= JAR_TOP && f.y <= JAR_TOP + 22;
  }

  // Advance by dt seconds. dir is -1, 0 or 1 (jar direction). Returns fireflies caught this step.
  function step(state, dt, dir) {
    if (state.over) return 0;
    state.time += dt;
    state.timeLeft = Math.max(0, state.timeLeft - dt);
    state.jarX += (dir < 0 ? -1 : dir > 0 ? 1 : 0) * JAR_SPEED * dt;
    state.jarX = Math.min(W - JAR_W / 2, Math.max(JAR_W / 2, state.jarX));

    state.spawnIn -= dt;
    if (state.spawnIn <= 0 && state.flies.length < MAX_FIREFLIES) {
      spawn(state);
      state.spawnIn = range(state, 1.6, 2.6) * (state.reduced ? 1.3 : 1);
    } else if (state.spawnIn <= 0) {
      state.spawnIn = 1;
    }

    var caught = 0;
    var keep = [];
    for (var i = 0; i < state.flies.length; i++) {
      var f = state.flies[i];
      f.t += dt;
      f.y += f.vy * dt;
      f.x = f.x0 + f.vx * f.t + (f.amp ? f.amp * Math.sin(f.phase + f.freq * f.t) : 0);
      if (!f.away) {
        if (overlapsJar(state, f)) {
          caught += 1;
          continue;
        }
        if (f.y > H - 8) {
          // Missed: it simply flies away upward. No penalty.
          f.away = true;
          f.vy = -Math.abs(f.vy) * 1.5;
        }
      } else if (f.y < -12) {
        continue;
      }
      keep.push(f);
    }
    state.flies = keep;
    state.caught += caught;
    if (state.caught > state.best) state.best = state.caught;
    if (state.timeLeft <= 0) state.over = true;
    return caught;
  }

  // A fresh round keeping the best score.
  function restart(state, seed) {
    var n = create(seed, state.reduced);
    n.best = state.best;
    return n;
  }

  function snapshot(state) {
    return JSON.parse(JSON.stringify(state));
  }

  function num(v, lo, hi) {
    return typeof v === "number" && isFinite(v) && v >= lo && v <= hi;
  }

  // Returns a clean state or null for anything malformed (restored data is untrusted).
  function validate(raw) {
    if (!raw || typeof raw !== "object" || raw.version !== 1) return null;
    if (!Array.isArray(raw.flies) || raw.flies.length > MAX_FIREFLIES) return null;
    if (!num(raw.seed, 0, 4294967295) || !num(raw.rng, 0, 4294967295)) return null;
    if (!num(raw.time, 0, 1e6) || !num(raw.timeLeft, 0, ROUND_SECONDS)) return null;
    if (!num(raw.jarX, 0, W) || !num(raw.spawnIn, -10, 10)) return null;
    if (!Number.isInteger(raw.caught) || raw.caught < 0 || raw.caught > 100000) return null;
    if (!Number.isInteger(raw.best) || raw.best < raw.caught || raw.best > 100000) return null;
    var flies = [];
    for (var i = 0; i < raw.flies.length; i++) {
      var f = raw.flies[i];
      if (!f || typeof f !== "object") return null;
      if (
        !num(f.x0, -2000, 2000) ||
        !num(f.x, -2000, 2000) ||
        !num(f.y, -100, H + 100) ||
        !num(f.t, 0, 1e5) ||
        !num(f.vy, -200, 200) ||
        !num(f.vx, -50, 50) ||
        !num(f.amp, 0, 100) ||
        !num(f.freq, 0, 10) ||
        !num(f.phase, 0, 7)
      ) {
        return null;
      }
      flies.push({
        x0: f.x0,
        x: f.x,
        y: f.y,
        t: f.t,
        vy: f.vy,
        vx: f.vx,
        amp: f.amp,
        freq: f.freq,
        phase: f.phase,
        away: f.away === true,
      });
    }
    return {
      version: 1,
      seed: raw.seed,
      rng: raw.rng,
      reduced: raw.reduced === true,
      time: raw.time,
      timeLeft: raw.timeLeft,
      jarX: raw.jarX,
      caught: raw.caught,
      best: raw.best,
      over: raw.over === true || raw.timeLeft <= 0,
      spawnIn: raw.spawnIn,
      flies: flies,
    };
  }

  root.FireflyLogic = {
    W: W,
    H: H,
    JAR_W: JAR_W,
    JAR_H: JAR_H,
    JAR_TOP: JAR_TOP,
    JAR_SPEED: JAR_SPEED,
    ROUND_SECONDS: ROUND_SECONDS,
    create: create,
    step: step,
    restart: restart,
    snapshot: snapshot,
    validate: validate,
  };
})(globalThis);
