// Pure rules for Guided Pause (engine lullshift.pause@1). No DOM, no timers.
// Attaches its API to globalThis.PauseLogic.
(function (root) {
  "use strict";

  var LOOKS = ["eyes", "hands", "window", "shoulders", "jaw", "breath"];

  // A small sample so the engine can run on its own, without a pack.
  var SAMPLE = {
    title: "Guided Pause",
    intro: "A sample pause. Open a pause pack to choose another.",
    steps: [
      { text: "Let your eyes rest on something far away.", seconds: 10, look: "eyes" },
      { text: "Unclench your hands and let them rest.", seconds: 10, look: "hands" },
    ],
    closing: "That is the pause. Carry on whenever you are ready.",
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
    if (!Array.isArray(c.steps) || c.steps.length < 1 || c.steps.length > 12) return false;
    for (var i = 0; i < c.steps.length; i++) {
      var s = c.steps[i];
      if (!s || typeof s !== "object" || !isText(s.text, 200)) return false;
      if (!isInt(s.seconds, 5, 120)) return false;
      if (s.look !== undefined && LOOKS.indexOf(s.look) < 0) return false;
    }
    return true;
  }

  function stepMs(c, index) {
    return c.steps[index].seconds * 1000;
  }

  function totalMs(c) {
    var t = 0;
    for (var i = 0; i < c.steps.length; i++) t += stepMs(c, i);
    return t;
  }

  // The position is { index, elapsed }: the step number and the milliseconds spent in it.
  // index === steps.length means the pause is finished.
  function isDone(c, pos) {
    return pos.index >= c.steps.length;
  }

  function start() {
    return { index: 0, elapsed: 0 };
  }

  // Adds `delta` milliseconds (at most 250 per call, so a throttled page never skips ahead),
  // moving on to later steps as each one's time runs out.
  function advance(c, pos, delta) {
    var d = Math.max(0, Math.min(delta, 250));
    var index = pos.index;
    var elapsed = pos.elapsed + d;
    while (index < c.steps.length && elapsed >= stepMs(c, index)) {
      elapsed -= stepMs(c, index);
      index++;
    }
    if (index >= c.steps.length) return { index: c.steps.length, elapsed: 0 };
    return { index: index, elapsed: elapsed };
  }

  // Skip to the next step (or to the end).
  function next(c, pos) {
    if (isDone(c, pos)) return pos;
    return { index: pos.index + 1, elapsed: 0 };
  }

  // Back to the start of the current step if it has begun, otherwise to the step before.
  // From the finished screen it goes to the last step.
  function previous(c, pos) {
    if (isDone(c, pos)) return { index: c.steps.length - 1, elapsed: 0 };
    if (pos.elapsed > 0) return { index: pos.index, elapsed: 0 };
    return { index: Math.max(0, pos.index - 1), elapsed: 0 };
  }

  function timeLeft(c, pos) {
    if (isDone(c, pos)) return 0;
    var t = stepMs(c, pos.index) - pos.elapsed;
    for (var i = pos.index + 1; i < c.steps.length; i++) t += stepMs(c, i);
    return t;
  }

  function secondsLeftInStep(c, pos) {
    if (isDone(c, pos)) return 0;
    return Math.max(1, Math.ceil((stepMs(c, pos.index) - pos.elapsed) / 1000));
  }

  function fraction(c, pos) {
    if (isDone(c, pos)) return 1;
    return pos.elapsed / stepMs(c, pos.index);
  }

  // m:ss, rounded up to the next whole second.
  function formatTime(ms) {
    var s = Math.max(0, Math.ceil(ms / 1000));
    var m = Math.floor(s / 60);
    var r = s % 60;
    return m + ":" + (r < 10 ? "0" : "") + r;
  }

  function validState(s, c) {
    if (!s || typeof s !== "object" || s.version !== 1) return false;
    if (!isInt(s.index, 0, c.steps.length)) return false;
    if (s.index === c.steps.length) return s.elapsed === 0;
    return isInt(s.elapsed, 0, stepMs(c, s.index) - 1);
  }

  function makeState(pos) {
    return { version: 1, index: pos.index, elapsed: Math.floor(pos.elapsed) };
  }

  root.PauseLogic = {
    LOOKS: LOOKS,
    SAMPLE: SAMPLE,
    validContent: validContent,
    stepMs: stepMs,
    totalMs: totalMs,
    isDone: isDone,
    start: start,
    advance: advance,
    next: next,
    previous: previous,
    timeLeft: timeLeft,
    secondsLeftInStep: secondsLeftInStep,
    fraction: fraction,
    formatTime: formatTime,
    validState: validState,
    makeState: makeState,
  };
})(globalThis);
