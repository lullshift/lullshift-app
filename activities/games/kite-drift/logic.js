// Pure rules for Kite Drift. No DOM. Attaches its API to globalThis.KiteLogic.
// All state is plain JSON; stepping uses a fixed dt so a snapshot restores exactly.
(function (root) {
  "use strict";
  var W = 400;
  var H = 300;
  var KITE_X = 110; // screen x of the kite; the world scrolls past it
  var KITE_R = 10;
  var SCROLL = 60; // px per second
  var BANK_W = 46;
  var SPACING = 230;
  var GAP_H = 130;
  var MAX_SHIFT = 90; // largest change of gap height between neighbours (always reachable)
  var LIFT = -240; // acceleration while holding
  var SINK = 120; // acceleration while released
  var VMIN = -100;
  var VMAX = 80;
  var MAX_CLOUDS = 12;

  function nextRandom(state) {
    state.rng = (state.rng + 0x6d2b79f5) >>> 0;
    var t = state.rng;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  }

  function nextCloud(state) {
    var last = state.clouds.length ? state.clouds[state.clouds.length - 1] : null;
    var lo = GAP_H / 2 + 24;
    var hi = H - GAP_H / 2 - 24;
    var gapY = lo + (hi - lo) * nextRandom(state);
    if (last) {
      gapY = Math.min(last.gapY + MAX_SHIFT, Math.max(last.gapY - MAX_SHIFT, gapY));
      gapY = Math.min(hi, Math.max(lo, gapY));
    }
    state.clouds.push({
      x: last ? last.x + SPACING : KITE_X + 260,
      gapY: gapY,
      done: false,
    });
  }

  function create(seed, reduced) {
    var s = {
      version: 1,
      seed: seed >>> 0,
      rng: seed >>> 0,
      reduced: !!reduced,
      time: 0,
      scroll: 0,
      y: H / 2,
      vy: 0,
      gaps: 0,
      streak: 0,
      bestStreak: 0,
      bump: 0,
      clouds: [],
    };
    while (s.clouds.length < 5) nextCloud(s);
    return s;
  }

  // Advance by dt seconds. hold: true while lifting. Returns { passed, bumped } counts.
  function step(state, dt, hold) {
    var out = { passed: 0, bumped: 0 };
    state.time += dt;
    state.scroll += SCROLL * dt;
    state.vy += (hold ? LIFT : SINK) * dt;
    state.vy = Math.min(VMAX, Math.max(VMIN, state.vy));
    state.y += state.vy * dt;
    if (state.y < KITE_R) {
      state.y = KITE_R;
      if (state.vy < 0) state.vy = 0;
    }
    if (state.y > H - KITE_R) {
      state.y = H - KITE_R;
      if (state.vy > 0) state.vy = 0;
    }
    if (state.bump > 0) state.bump = Math.max(0, state.bump - dt);

    var wx = state.scroll + KITE_X; // kite position in world coordinates
    for (var i = 0; i < state.clouds.length; i++) {
      var c = state.clouds[i];
      if (c.done) continue;
      var inX = wx + KITE_R > c.x && wx - KITE_R < c.x + BANK_W;
      if (inX) {
        var top = c.gapY - GAP_H / 2;
        var bottom = c.gapY + GAP_H / 2;
        if (state.y - KITE_R < top || state.y + KITE_R > bottom) {
          // A bump: back to the middle of this opening, streak over, no penalty.
          c.done = true;
          state.y = c.gapY;
          state.vy = 0;
          state.streak = 0;
          state.bump = 0.6;
          out.bumped += 1;
        }
      } else if (wx - KITE_R >= c.x + BANK_W) {
        c.done = true;
        state.gaps += 1;
        state.streak += 1;
        if (state.streak > state.bestStreak) state.bestStreak = state.streak;
        out.passed += 1;
      }
    }
    // Drop banks well behind the kite, keep some ahead.
    while (state.clouds.length && state.clouds[0].x + BANK_W < state.scroll - 40) {
      state.clouds.shift();
    }
    while (
      state.clouds.length < MAX_CLOUDS &&
      state.clouds[state.clouds.length - 1].x < state.scroll + W + 260
    ) {
      nextCloud(state);
    }
    return out;
  }

  // A fresh flight keeping the best streak.
  function restart(state, seed) {
    var n = create(seed, state.reduced);
    n.bestStreak = state.bestStreak;
    return n;
  }

  function snapshot(state) {
    return JSON.parse(JSON.stringify(state));
  }

  function num(v, lo, hi) {
    return typeof v === "number" && isFinite(v) && v >= lo && v <= hi;
  }
  function count(v) {
    return Number.isInteger(v) && v >= 0 && v <= 1000000;
  }

  function validate(raw) {
    if (!raw || typeof raw !== "object" || raw.version !== 1) return null;
    if (!Array.isArray(raw.clouds) || raw.clouds.length < 1 || raw.clouds.length > MAX_CLOUDS) {
      return null;
    }
    if (!num(raw.seed, 0, 4294967295) || !num(raw.rng, 0, 4294967295)) return null;
    if (!num(raw.time, 0, 1e9) || !num(raw.scroll, 0, 1e9)) return null;
    if (!num(raw.y, 0, H) || !num(raw.vy, VMIN, VMAX) || !num(raw.bump, 0, 5)) return null;
    if (!count(raw.gaps) || !count(raw.streak) || !count(raw.bestStreak)) return null;
    if (raw.bestStreak < raw.streak) return null;
    var clouds = [];
    for (var i = 0; i < raw.clouds.length; i++) {
      var c = raw.clouds[i];
      if (!c || typeof c !== "object") return null;
      if (!num(c.x, -1000, 1e9) || !num(c.gapY, GAP_H / 2, H - GAP_H / 2)) return null;
      clouds.push({ x: c.x, gapY: c.gapY, done: c.done === true });
    }
    return {
      version: 1,
      seed: raw.seed,
      rng: raw.rng,
      reduced: raw.reduced === true,
      time: raw.time,
      scroll: raw.scroll,
      y: raw.y,
      vy: raw.vy,
      gaps: raw.gaps,
      streak: raw.streak,
      bestStreak: raw.bestStreak,
      bump: raw.bump,
      clouds: clouds,
    };
  }

  root.KiteLogic = {
    W: W,
    H: H,
    KITE_X: KITE_X,
    KITE_R: KITE_R,
    BANK_W: BANK_W,
    GAP_H: GAP_H,
    SPACING: SPACING,
    MAX_SHIFT: MAX_SHIFT,
    VMIN: VMIN,
    VMAX: VMAX,
    create: create,
    step: step,
    restart: restart,
    snapshot: snapshot,
    validate: validate,
  };
})(globalThis);
