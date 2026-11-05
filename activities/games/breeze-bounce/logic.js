// Pure rules for Breeze Bounce. No DOM. Attaches its API to globalThis.BreezeBounceLogic.
// Fixed-step: step(state, dir) advances exactly one STEP (1/60 s). The round clock is a tick count
// owned by the state, so it only moves when step() is called (never while the host has paused).
(function (root) {
  "use strict";
  var W = 400;
  var H = 300;
  var STEP = 1 / 60;
  var TPS = 60;
  var PADDLE_W = 68;
  var PADDLE_H = 8;
  var PADDLE_Y = H - 28; // top edge of the leaf
  var PADDLE_SPEED = 230; // px per second
  var BALL_R = 6;
  var DAND_R = 13;
  var ROWS = 4;
  var COLS = 8;
  var SPEED = 150;
  var SPEED_REDUCED = 100;
  var DRIFT_WAIT = 90; // ticks the seed rests at the top before drifting down
  var MAX_ANGLE = 1.0; // radians from vertical at the leaf edge
  var MAX_DRIFTS = 100000;
  var MAX_TICKS = 60 * 60 * 24 * 7;

  // mulberry32 whose whole state is the number stored in state.rng.
  function nextRandom(state) {
    state.rng = (state.rng + 0x6d2b79f5) >>> 0;
    var t = state.rng;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  }

  function speedOf(state) {
    return state.reduced ? SPEED_REDUCED : SPEED;
  }

  // Dandelions in rows: each cell is present about 3 times in 4, every row has at least two.
  // Positions are integers so layouts compare exactly.
  function layout(state) {
    var out = [];
    var cellW = (W - 40) / COLS;
    for (var r = 0; r < ROWS; r++) {
      var row = [];
      for (var c = 0; c < COLS; c++) {
        var present = nextRandom(state) < 0.75;
        var jx = Math.round((nextRandom(state) - 0.5) * 8);
        var jy = Math.round((nextRandom(state) - 0.5) * 6);
        row.push({ present: present, jx: jx, jy: jy });
      }
      var have = 0;
      for (c = 0; c < COLS; c++) if (row[c].present) have++;
      while (have < 2) {
        var k = Math.floor(nextRandom(state) * COLS);
        if (!row[k].present) {
          row[k].present = true;
          have++;
        }
      }
      for (c = 0; c < COLS; c++) {
        if (!row[c].present) continue;
        out.push({
          x: Math.round(20 + cellW * (c + 0.5) + row[c].jx),
          y: 40 + r * 32 + row[c].jy,
          released: false,
        });
      }
    }
    return out;
  }

  // Put the seed at the top, resting, ready to drift down.
  function placeSeed(state) {
    var x = Math.round(40 + nextRandom(state) * (W - 80));
    var dirX = nextRandom(state) < 0.5 ? -1 : 1;
    state.ball = { x: x, y: 14, vx: 0, vy: 0, wait: DRIFT_WAIT, side: dirX };
  }

  function create(seed, reduced, best) {
    var state = {
      version: 1,
      seed: seed >>> 0,
      rng: seed >>> 0,
      reduced: !!reduced,
      ticks: 0,
      drifts: 0,
      best: typeof best === "number" && best > 0 ? Math.floor(best) : 0,
      over: false,
      newBest: false,
      paddleX: W / 2,
      ball: null,
      dands: [],
    };
    state.dands = layout(state);
    placeSeed(state);
    return state;
  }

  function released(state) {
    var n = 0;
    for (var i = 0; i < state.dands.length; i++) if (state.dands[i].released) n++;
    return n;
  }

  function remaining(state) {
    return state.dands.length - released(state);
  }

  function seconds(state) {
    return state.ticks / TPS;
  }

  function launch(state) {
    var b = state.ball;
    var s = speedOf(state);
    var a = 0.35 * b.side;
    b.vx = Math.sin(a) * s;
    b.vy = Math.cos(a) * s;
  }

  function hitDandelions(state) {
    var b = state.ball;
    var reach = BALL_R + DAND_R;
    var count = 0;
    var bounced = false;
    for (var i = 0; i < state.dands.length; i++) {
      var d = state.dands[i];
      if (d.released) continue;
      var dx = b.x - d.x;
      var dy = b.y - d.y;
      if (dx * dx + dy * dy > reach * reach) continue;
      d.released = true;
      count++;
      if (!bounced) {
        bounced = true;
        if (Math.abs(dx) > Math.abs(dy)) b.vx = (dx >= 0 ? 1 : -1) * Math.abs(b.vx || 1);
        else b.vy = (dy >= 0 ? 1 : -1) * Math.abs(b.vy || 1);
      }
    }
    return count;
  }

  // Advance one fixed step. dir: -1 left, 0 none, 1 right. Returns dandelions released this step.
  function step(state, dir) {
    if (state.over) return 0;
    var d = dir > 0 ? 1 : dir < 0 ? -1 : 0;
    state.ticks++;
    var half = PADDLE_W / 2;
    state.paddleX = Math.min(W - half, Math.max(half, state.paddleX + d * PADDLE_SPEED * STEP));
    var b = state.ball;
    if (b.wait > 0) {
      b.wait--;
      if (b.wait === 0) launch(state);
      return 0;
    }
    b.x += b.vx * STEP;
    b.y += b.vy * STEP;
    if (b.x < BALL_R) {
      b.x = BALL_R;
      b.vx = Math.abs(b.vx);
    } else if (b.x > W - BALL_R) {
      b.x = W - BALL_R;
      b.vx = -Math.abs(b.vx);
    }
    if (b.y < BALL_R) {
      b.y = BALL_R;
      b.vy = Math.abs(b.vy);
    }
    // The leaf: a gentle bounce, steered by where the seed lands.
    if (
      b.vy > 0 &&
      b.y + BALL_R >= PADDLE_Y &&
      b.y - BALL_R <= PADDLE_Y + PADDLE_H &&
      Math.abs(b.x - state.paddleX) <= half + BALL_R
    ) {
      var off = Math.max(-1, Math.min(1, (b.x - state.paddleX) / half));
      var a = off * MAX_ANGLE;
      var s = speedOf(state);
      b.vx = Math.sin(a) * s;
      b.vy = -Math.cos(a) * s;
      b.y = PADDLE_Y - BALL_R;
    }
    var got = hitDandelions(state);
    if (remaining(state) === 0) {
      state.over = true;
      if (state.best === 0 || state.ticks < state.best) {
        state.newBest = true;
        state.best = state.ticks;
      }
      return got;
    }
    // Missed: no judgement. A new seed drifts in from the top after a short rest.
    if (b.y - BALL_R > H) {
      state.drifts++;
      placeSeed(state);
    }
    return got;
  }

  function restart(state, seed) {
    return create(seed, state.reduced, state.best);
  }

  function snapshot(state) {
    return JSON.parse(JSON.stringify(state));
  }

  function num(v, lo, hi) {
    return typeof v === "number" && isFinite(v) && v >= lo && v <= hi;
  }
  function int(v, lo, hi) {
    return Number.isInteger(v) && v >= lo && v <= hi;
  }

  // Returns a clean state or null for anything malformed (restored data is untrusted).
  function validate(raw) {
    if (!raw || typeof raw !== "object" || raw.version !== 1) return null;
    if (!int(raw.seed, 0, 4294967295) || !int(raw.rng, 0, 4294967295)) return null;
    if (!int(raw.ticks, 0, MAX_TICKS) || !int(raw.drifts, 0, MAX_DRIFTS)) return null;
    if (!int(raw.best, 0, MAX_TICKS)) return null;
    if (typeof raw.reduced !== "boolean" || typeof raw.over !== "boolean") return null;
    if (typeof raw.newBest !== "boolean") return null;
    if (!num(raw.paddleX, PADDLE_W / 2, W - PADDLE_W / 2)) return null;
    var b = raw.ball;
    if (!b || typeof b !== "object") return null;
    if (!num(b.x, 0, W) || !num(b.y, -50, H + 50)) return null;
    if (!num(b.vx, -400, 400) || !num(b.vy, -400, 400)) return null;
    if (!int(b.wait, 0, DRIFT_WAIT) || (b.side !== 1 && b.side !== -1)) return null;
    if (!Array.isArray(raw.dands) || raw.dands.length < 1 || raw.dands.length > ROWS * COLS) {
      return null;
    }
    var dands = [];
    var left = 0;
    for (var i = 0; i < raw.dands.length; i++) {
      var q = raw.dands[i];
      if (!q || typeof q !== "object") return null;
      if (!num(q.x, 0, W) || !num(q.y, 0, H) || typeof q.released !== "boolean") return null;
      if (!q.released) left++;
      dands.push({ x: q.x, y: q.y, released: q.released });
    }
    if (raw.over !== (left === 0)) return null;
    return {
      version: 1,
      seed: raw.seed,
      rng: raw.rng,
      reduced: raw.reduced,
      ticks: raw.ticks,
      drifts: raw.drifts,
      best: raw.best,
      over: raw.over,
      newBest: raw.newBest,
      paddleX: raw.paddleX,
      ball: { x: b.x, y: b.y, vx: b.vx, vy: b.vy, wait: b.wait, side: b.side },
      dands: dands,
    };
  }

  function formatTime(ticks) {
    var s = ticks / TPS;
    var m = Math.floor(s / 60);
    var rest = (s - m * 60).toFixed(1);
    if (rest === "60.0") {
      m++;
      rest = "0.0";
    }
    return m + ":" + (parseFloat(rest) < 10 ? "0" : "") + rest;
  }

  root.BreezeBounceLogic = {
    W: W,
    H: H,
    STEP: STEP,
    TPS: TPS,
    PADDLE_W: PADDLE_W,
    PADDLE_H: PADDLE_H,
    PADDLE_Y: PADDLE_Y,
    BALL_R: BALL_R,
    DAND_R: DAND_R,
    DRIFT_WAIT: DRIFT_WAIT,
    create: create,
    step: step,
    restart: restart,
    snapshot: snapshot,
    validate: validate,
    released: released,
    remaining: remaining,
    seconds: seconds,
    formatTime: formatTime,
  };
})(globalThis);
