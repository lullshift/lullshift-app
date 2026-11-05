// Pure rules for Breathing Space (engine lullshift.breathing@1). No DOM, no timers.
// Attaches its API to globalThis.BreathingLogic.
(function (root) {
  "use strict";

  var KINDS = ["inhale", "hold", "exhale", "rest"];
  var MIN_SCALE = 0.4;
  var MAX_SCALE = 1;
  var DEFAULT_CUES = { inhale: "Breathe in", hold: "Hold", exhale: "Breathe out", rest: "Rest" };

  // A small sample so the engine can run on its own, without a pack.
  var SAMPLE = {
    title: "Breathing Space",
    intro: "A sample pattern. Open a breathing pack to choose another.",
    phases: [
      { kind: "inhale", seconds: 4 },
      { kind: "exhale", seconds: 4 },
    ],
    rounds: 3,
    closing: "That is the set. Carry on whenever you are ready.",
  };

  function isText(v, max) {
    return typeof v === "string" && v.length > 0 && v.length <= max;
  }

  function isInt(v, min, max) {
    return typeof v === "number" && Number.isInteger(v) && v >= min && v <= max;
  }

  // The host validates packs against the schema; this is a cheap second look.
  function validContent(c) {
    if (!c || typeof c !== "object" || Array.isArray(c)) return false;
    if (!isText(c.title, 80)) return false;
    if (c.intro !== undefined && !isText(c.intro, 300)) return false;
    if (c.closing !== undefined && !isText(c.closing, 200)) return false;
    if (!isInt(c.rounds, 1, 20)) return false;
    if (!Array.isArray(c.phases) || c.phases.length < 2 || c.phases.length > 12) return false;
    for (var i = 0; i < c.phases.length; i++) {
      var p = c.phases[i];
      if (!p || typeof p !== "object" || KINDS.indexOf(p.kind) < 0) return false;
      if (!isInt(p.seconds, 1, p.kind === "hold" ? 4 : 8)) return false;
      if (p.cue !== undefined && !isText(p.cue, 24)) return false;
    }
    return true;
  }

  function cycleMs(c) {
    var t = 0;
    for (var i = 0; i < c.phases.length; i++) t += c.phases[i].seconds * 1000;
    return t;
  }

  function totalMs(c) {
    return cycleMs(c) * c.rounds;
  }

  // Start and end size of each phase. Inhale grows to full size, exhale shrinks to the smallest,
  // hold and rest keep the size they arrive with. Walked twice so a pattern that begins with a
  // hold or rest still starts from where the previous cycle ended.
  function levels(c) {
    var out = [];
    var level = MIN_SCALE;
    for (var pass = 0; pass < 2; pass++) {
      out = [];
      for (var i = 0; i < c.phases.length; i++) {
        var start = level;
        var kind = c.phases[i].kind;
        if (kind === "inhale") level = MAX_SCALE;
        else if (kind === "exhale") level = MIN_SCALE;
        out.push({ start: start, end: level });
      }
    }
    return out;
  }

  // Where the session is at `elapsed` milliseconds.
  function locate(c, elapsed) {
    var total = totalMs(c);
    var cycle = cycleMs(c);
    if (elapsed >= total) {
      var last = c.phases.length - 1;
      return {
        done: true,
        round: c.rounds,
        phaseIndex: last,
        phaseMs: c.phases[last].seconds * 1000,
        phaseElapsed: c.phases[last].seconds * 1000,
        fraction: 1,
      };
    }
    var e = Math.max(0, elapsed);
    var round = Math.floor(e / cycle) + 1;
    var within = e - (round - 1) * cycle;
    for (var i = 0; i < c.phases.length; i++) {
      var ms = c.phases[i].seconds * 1000;
      if (within < ms) {
        return {
          done: false,
          round: round,
          phaseIndex: i,
          phaseMs: ms,
          phaseElapsed: within,
          fraction: within / ms,
        };
      }
      within -= ms;
    }
    // Unreachable: within is always less than the cycle length.
    return { done: true, round: c.rounds, phaseIndex: 0, phaseMs: 0, phaseElapsed: 0, fraction: 1 };
  }

  function scaleAt(c, elapsed) {
    var pos = locate(c, elapsed);
    var lv = levels(c)[pos.phaseIndex];
    var eased = 0.5 - 0.5 * Math.cos(Math.PI * pos.fraction);
    return lv.start + (lv.end - lv.start) * eased;
  }

  function cueFor(phase) {
    return phase.cue || DEFAULT_CUES[phase.kind];
  }

  // Whole seconds left in a phase, counting down from its length to 1.
  function secondsLeft(pos) {
    return Math.max(1, Math.ceil((pos.phaseMs - pos.phaseElapsed) / 1000));
  }

  // m:ss, rounded up to the next whole second.
  function formatTime(ms) {
    var s = Math.max(0, Math.ceil(ms / 1000));
    var m = Math.floor(s / 60);
    var r = s % 60;
    return m + ":" + (r < 10 ? "0" : "") + r;
  }

  function validState(s, c) {
    return !!s && typeof s === "object" && s.version === 1 && isInt(s.elapsed, 0, totalMs(c));
  }

  function makeState(elapsed) {
    return { version: 1, elapsed: Math.round(elapsed) };
  }

  // Adds `delta` to the elapsed time. A long gap (a throttled page) counts as at most 250 ms.
  function advance(c, elapsed, delta) {
    var d = Math.max(0, Math.min(delta, 250));
    return Math.min(totalMs(c), elapsed + d);
  }

  root.BreathingLogic = {
    KINDS: KINDS,
    MIN_SCALE: MIN_SCALE,
    MAX_SCALE: MAX_SCALE,
    DEFAULT_CUES: DEFAULT_CUES,
    SAMPLE: SAMPLE,
    validContent: validContent,
    cycleMs: cycleMs,
    totalMs: totalMs,
    levels: levels,
    locate: locate,
    scaleAt: scaleAt,
    cueFor: cueFor,
    secondsLeft: secondsLeft,
    formatTime: formatTime,
    validState: validState,
    makeState: makeState,
    advance: advance,
  };
})(globalThis);
