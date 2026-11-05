// Pure rules for Typing Drills (lullshift.typing@1). No DOM, no timers, no clock of its own.
// Only counts are kept. The typed text itself is never stored.
(function (root) {
  "use strict";
  var MAX_LINE = 120;
  var MAX_LINES = 40;

  function isText(v, max) {
    return typeof v === "string" && v.length > 0 && v.length <= max;
  }

  function printable(s) {
    return /^[ -~]*$/.test(s);
  }

  // The host validates packs; this is a cheap second look so a bad pack never breaks the page.
  function validPack(c) {
    if (!c || typeof c !== "object" || !isText(c.title, 80)) return false;
    if (c.intro !== undefined && !isText(c.intro, 300)) return false;
    if (c.mode !== undefined && c.mode !== "accuracy" && c.mode !== "flow") return false;
    if (!Array.isArray(c.lines) || c.lines.length < 1 || c.lines.length > MAX_LINES) return false;
    for (var i = 0; i < c.lines.length; i++) {
      if (!isText(c.lines[i], MAX_LINE) || !printable(c.lines[i])) return false;
    }
    return true;
  }

  /** One status per character of the target: "correct", "wrong" or "pending". */
  function compare(target, typed) {
    var out = [];
    for (var i = 0; i < target.length; i++) {
      if (i >= typed.length) out.push("pending");
      else out.push(typed.charAt(i) === target.charAt(i) ? "correct" : "wrong");
    }
    return out;
  }

  function countOf(statuses, what) {
    var n = 0;
    for (var i = 0; i < statuses.length; i++) if (statuses[i] === what) n++;
    return n;
  }

  function isComplete(target, typed) {
    return typed.length >= target.length;
  }

  function mistakeText(n) {
    return n === 0 ? "no mistakes" : n === 1 ? "1 mistake" : n + " mistakes";
  }

  function emptyStats() {
    return { done: 0, chars: 0, correct: 0, activeMs: 0 };
  }

  /** Adds one finished line. Skipped lines are never added. */
  function addLine(stats, chars, correct, activeMs) {
    return {
      done: stats.done + 1,
      chars: stats.chars + chars,
      correct: stats.correct + correct,
      activeMs: stats.activeMs + Math.max(0, Math.floor(activeMs)),
    };
  }

  function accuracy(stats) {
    if (stats.chars <= 0) return null;
    return Math.round((stats.correct / stats.chars) * 100);
  }

  /** Words per minute from correct characters (five to a word) and active time only. */
  function wpm(stats) {
    if (stats.activeMs < 1000 || stats.correct <= 0) return null;
    return Math.round(stats.correct / 5 / (stats.activeMs / 60000));
  }

  /** Milliseconds of active typing: what was banked plus the current running stretch. */
  function activeMs(banked, runningSince, now) {
    return banked + (runningSince === null ? 0 : Math.max(0, now - runningSince));
  }

  function cleanStats(raw, lineCount) {
    var s = emptyStats();
    if (!raw || typeof raw !== "object") return s;
    function num(v, max) {
      return typeof v === "number" && isFinite(v) && v >= 0 ? Math.min(max, Math.floor(v)) : 0;
    }
    s.done = Math.min(lineCount, num(raw.done, lineCount));
    s.chars = num(raw.chars, MAX_LINE * MAX_LINES);
    s.correct = Math.min(s.chars, num(raw.correct, MAX_LINE * MAX_LINES));
    s.activeMs = num(raw.activeMs, 86400000);
    return s;
  }

  root.TypingLogic = {
    MAX_LINE: MAX_LINE,
    validPack: validPack,
    compare: compare,
    countOf: countOf,
    isComplete: isComplete,
    mistakeText: mistakeText,
    emptyStats: emptyStats,
    addLine: addLine,
    accuracy: accuracy,
    wpm: wpm,
    activeMs: activeMs,
    cleanStats: cleanStats,
  };
})(typeof globalThis !== "undefined" ? globalThis : this);
