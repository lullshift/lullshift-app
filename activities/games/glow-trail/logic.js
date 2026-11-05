// Pure rules for Glow Trail. No DOM. Attaches its API to globalThis.GlowTrailLogic.
// Fixed-step: step(state) advances exactly one tick (1/60 s). The round clock is a tick count
// owned by the state, so it only moves when step() is called (never while the host has paused).
(function (root) {
  "use strict";
  var COLS = 20;
  var ROWS = 15;
  var CELL = 20;
  var TPS = 60;
  var ROUND_TICKS = 120 * TPS; // 2 minutes of game time
  var MOVE_TICKS = 15; // one cell every 0.25 s
  var MOVE_TICKS_SLOW = 24; // one cell every 0.4 s
  var START_LEN = 3;
  var MAX_LEN = 120;
  var DEW_COUNT = 3;
  // 0 up, 1 right, 2 down, 3 left
  var DX = [0, 1, 0, -1];
  var DY = [-1, 0, 1, 0];

  // mulberry32 whose whole state is the number stored in state.rng.
  function nextRandom(state) {
    state.rng = (state.rng + 0x6d2b79f5) >>> 0;
    var t = state.rng;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  }

  function interval(state) {
    return state.slower ? MOVE_TICKS_SLOW : MOVE_TICKS;
  }

  function occupied(state, x, y) {
    for (var i = 0; i < state.trail.length; i++) {
      if (state.trail[i][0] === x && state.trail[i][1] === y) return true;
    }
    for (var j = 0; j < state.dew.length; j++) {
      if (state.dew[j][0] === x && state.dew[j][1] === y) return true;
    }
    return false;
  }

  // A free cell chosen with the seeded generator (always exists: the trail is far shorter than the grid).
  function spawnDew(state) {
    for (var tries = 0; tries < 200; tries++) {
      var x = Math.floor(nextRandom(state) * COLS);
      var y = Math.floor(nextRandom(state) * ROWS);
      if (!occupied(state, x, y)) {
        state.dew.push([x, y]);
        return;
      }
    }
    for (var yy = 0; yy < ROWS; yy++) {
      for (var xx = 0; xx < COLS; xx++) {
        if (!occupied(state, xx, yy)) {
          state.dew.push([xx, yy]);
          return;
        }
      }
    }
  }

  function create(seed, slower, best) {
    var cx = Math.floor(COLS / 2);
    var cy = Math.floor(ROWS / 2);
    var state = {
      version: 1,
      seed: seed >>> 0,
      rng: seed >>> 0,
      slower: !!slower,
      ticks: 0,
      cool: 0,
      dir: 1,
      want: 1,
      trail: [],
      dew: [],
      length: START_LEN,
      longest: START_LEN,
      collected: 0,
      best: typeof best === "number" && best > START_LEN ? Math.floor(best) : 0,
      over: false,
      newBest: false,
    };
    for (var i = 0; i < START_LEN; i++) state.trail.push([cx - i, cy]);
    state.cool = interval(state);
    while (state.dew.length < DEW_COUNT) spawnDew(state);
    return state;
  }

  // Ask for a direction. A straight reversal into the trail is ignored; so is a request once the round is over.
  function steer(state, d) {
    if (state.over || !Number.isInteger(d) || d < 0 || d > 3) return false;
    if (d === (state.dir + 2) % 4) return false;
    state.want = d;
    return true;
  }

  function setSlower(state, slower) {
    state.slower = !!slower;
    if (state.cool > interval(state)) state.cool = interval(state);
  }

  function secondsLeft(state) {
    return Math.ceil((ROUND_TICKS - state.ticks) / TPS);
  }

  // Event bits returned from step().
  var COLLECTED = 1;
  var EASED = 2;

  function trimToStart(state) {
    state.trail.length = Math.min(state.trail.length, START_LEN);
    state.length = START_LEN;
  }

  function move(state) {
    var d = state.want;
    var head = state.trail[0];
    var nx = head[0] + DX[d];
    var ny = head[1] + DY[d];
    var events = 0;
    state.dir = d;
    if (nx < 0 || ny < 0 || nx >= COLS || ny >= ROWS) {
      // The edge: the firefly stays where it is and the trail eases back to its starting length.
      if (state.trail.length > START_LEN) events |= EASED;
      trimToStart(state);
      return events;
    }
    // The tail cell is about to leave, so moving onto it is fine unless the trail is growing.
    var hitIndex = -1;
    for (var i = 0; i < state.trail.length; i++) {
      if (state.trail[i][0] === nx && state.trail[i][1] === ny) {
        hitIndex = i;
        break;
      }
    }
    var dewIndex = -1;
    for (var j = 0; j < state.dew.length; j++) {
      if (state.dew[j][0] === nx && state.dew[j][1] === ny) {
        dewIndex = j;
        break;
      }
    }
    var tailLeaves = hitIndex === state.trail.length - 1 && state.trail.length >= state.length;
    if (hitIndex >= 0 && !(tailLeaves && dewIndex < 0)) {
      // Touched its own trail: shorten back to the start length and carry on.
      trimToStart(state);
      events |= EASED;
      for (var m = 0; m < state.trail.length; m++) {
        if (state.trail[m][0] === nx && state.trail[m][1] === ny) return events; // only for odd saved states
      }
    }
    state.trail.unshift([nx, ny]);
    if (dewIndex >= 0) {
      state.dew.splice(dewIndex, 1);
      state.collected++;
      events |= COLLECTED;
      if (state.length < MAX_LEN) state.length++;
      spawnDew(state);
    }
    while (state.trail.length > state.length) state.trail.pop();
    if (state.length > state.longest) state.longest = state.length;
    return events;
  }

  // Advance one fixed tick. Returns event bits (COLLECTED, EASED), 0 when nothing happened.
  function step(state) {
    if (state.over) return 0;
    state.ticks++;
    var events = 0;
    state.cool--;
    if (state.cool <= 0) {
      state.cool = interval(state);
      events = move(state);
    }
    if (state.ticks >= ROUND_TICKS) {
      state.over = true;
      if (state.longest > state.best) {
        state.best = state.longest;
        state.newBest = true;
      }
    }
    return events;
  }

  function restart(state, seed) {
    return create(seed, state.slower, state.best);
  }

  function snapshot(state) {
    return JSON.parse(JSON.stringify(state));
  }

  function int(v, lo, hi) {
    return Number.isInteger(v) && v >= lo && v <= hi;
  }
  function cellOk(c) {
    return Array.isArray(c) && c.length === 2 && int(c[0], 0, COLS - 1) && int(c[1], 0, ROWS - 1);
  }

  // Returns a clean state or null for anything malformed (restored data is untrusted).
  function validate(raw) {
    if (!raw || typeof raw !== "object" || raw.version !== 1) return null;
    if (!int(raw.seed, 0, 4294967295) || !int(raw.rng, 0, 4294967295)) return null;
    if (typeof raw.slower !== "boolean" || typeof raw.over !== "boolean") return null;
    if (typeof raw.newBest !== "boolean") return null;
    if (!int(raw.ticks, 0, ROUND_TICKS) || !int(raw.cool, 0, MOVE_TICKS_SLOW)) return null;
    if (!int(raw.dir, 0, 3) || !int(raw.want, 0, 3)) return null;
    if (!int(raw.length, START_LEN, MAX_LEN) || !int(raw.longest, START_LEN, MAX_LEN)) return null;
    if (raw.longest < raw.length || !int(raw.collected, 0, 100000)) return null;
    if (!int(raw.best, 0, MAX_LEN)) return null;
    if (raw.over !== raw.ticks >= ROUND_TICKS) return null;
    if (
      !Array.isArray(raw.trail) ||
      raw.trail.length < START_LEN ||
      raw.trail.length > raw.length
    ) {
      return null;
    }
    if (!Array.isArray(raw.dew) || raw.dew.length !== DEW_COUNT) return null;
    var seen = {};
    var trail = [];
    var dew = [];
    var i;
    for (i = 0; i < raw.trail.length; i++) {
      var c = raw.trail[i];
      if (!cellOk(c)) return null;
      var k = c[0] + "," + c[1];
      if (seen[k]) return null;
      seen[k] = 1;
      if (i > 0) {
        var p = raw.trail[i - 1];
        if (Math.abs(p[0] - c[0]) + Math.abs(p[1] - c[1]) !== 1) return null;
      }
      trail.push([c[0], c[1]]);
    }
    for (i = 0; i < raw.dew.length; i++) {
      var q = raw.dew[i];
      if (!cellOk(q)) return null;
      var kq = q[0] + "," + q[1];
      if (seen[kq]) return null;
      seen[kq] = 1;
      dew.push([q[0], q[1]]);
    }
    return {
      version: 1,
      seed: raw.seed,
      rng: raw.rng,
      slower: raw.slower,
      ticks: raw.ticks,
      cool: raw.cool,
      dir: raw.dir,
      want: raw.want,
      trail: trail,
      dew: dew,
      length: raw.length,
      longest: raw.longest,
      collected: raw.collected,
      best: raw.best,
      over: raw.over,
      newBest: raw.newBest,
    };
  }

  root.GlowTrailLogic = {
    COLS: COLS,
    ROWS: ROWS,
    CELL: CELL,
    TPS: TPS,
    ROUND_TICKS: ROUND_TICKS,
    MOVE_TICKS: MOVE_TICKS,
    MOVE_TICKS_SLOW: MOVE_TICKS_SLOW,
    START_LEN: START_LEN,
    DEW_COUNT: DEW_COUNT,
    COLLECTED: COLLECTED,
    EASED: EASED,
    create: create,
    step: step,
    steer: steer,
    setSlower: setSlower,
    restart: restart,
    snapshot: snapshot,
    validate: validate,
    secondsLeft: secondsLeft,
  };
})(globalThis);
