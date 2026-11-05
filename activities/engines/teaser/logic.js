// Pure rules for Brain Teasers (lullshift.teaser@1). No DOM. Attaches to globalThis.TeaserLogic.
(function (root) {
  "use strict";

  var LIMITS = {
    title: 80,
    intro: 300,
    question: 300,
    hint: 200,
    answer: 200,
    explanation: 300,
    maxTeasers: 40,
  };
  var KINDS = ["logic", "pattern", "wordplay", "math"];
  var KIND_LABELS = { logic: "Logic", pattern: "Pattern", wordplay: "Word play", math: "Maths" };

  function isText(v, max) {
    return typeof v === "string" && v.length > 0 && v.length <= max;
  }

  // The host validates packs; this is a cheap second look so a bad pack never breaks the page.
  function validTeasers(c) {
    if (!c || typeof c !== "object" || !isText(c.title, LIMITS.title)) return false;
    if (c.intro !== undefined && !isText(c.intro, LIMITS.intro)) return false;
    var ts = c.teasers;
    if (!Array.isArray(ts) || ts.length < 1 || ts.length > LIMITS.maxTeasers) return false;
    for (var i = 0; i < ts.length; i++) {
      var t = ts[i];
      if (!t || typeof t !== "object" || !isText(t.question, LIMITS.question)) return false;
      if (t.hint !== undefined && !isText(t.hint, LIMITS.hint)) return false;
      if (!isText(t.answer, LIMITS.answer) || !isText(t.explanation, LIMITS.explanation))
        return false;
      if (t.kind !== undefined && KINDS.indexOf(t.kind) < 0) return false;
    }
    return true;
  }

  function initialState(n) {
    var marks = [];
    for (var i = 0; i < n; i++) marks.push(null);
    return { i: 0, hint: false, revealed: false, marks: marks };
  }

  function copy(s) {
    return { i: s.i, hint: s.hint, revealed: s.revealed, marks: s.marks.slice() };
  }

  function atEnd(s, n) {
    return s.i >= n;
  }

  // Moves to a teaser (clamped to 0..n). The hint and the answer start hidden again.
  function go(s, n, to) {
    var target = Math.max(0, Math.min(n, to));
    if (target === s.i) return s;
    var next = copy(s);
    next.i = target;
    next.hint = false;
    next.revealed = false;
    return next;
  }

  function showHint(s, n, teasers) {
    if (atEnd(s, n) || s.revealed || s.hint || teasers[s.i].hint === undefined) return s;
    var next = copy(s);
    next.hint = true;
    return next;
  }

  function reveal(s, n) {
    if (atEnd(s, n) || s.revealed) return s;
    var next = copy(s);
    next.revealed = true;
    return next;
  }

  // Self-check: true = got it, false = not this time. Only after the answer is shown. Choosing
  // the same mark again clears it.
  function mark(s, n, value) {
    if (atEnd(s, n) || !s.revealed || typeof value !== "boolean") return s;
    var next = copy(s);
    next.marks[s.i] = next.marks[s.i] === value ? null : value;
    return next;
  }

  function again(n) {
    return initialState(n);
  }

  function tally(s) {
    var got = 0;
    var missed = 0;
    for (var i = 0; i < s.marks.length; i++) {
      if (s.marks[i] === true) got++;
      else if (s.marks[i] === false) missed++;
    }
    return { got: got, missed: missed };
  }

  // Returns a clean state, or null when a saved one does not fit this pack.
  function restoreState(teasers, raw) {
    var n = teasers.length;
    if (!raw || typeof raw !== "object") return null;
    if (!Number.isInteger(raw.i) || raw.i < 0 || raw.i > n) return null;
    if (typeof raw.hint !== "boolean" || typeof raw.revealed !== "boolean") return null;
    if (!Array.isArray(raw.marks) || raw.marks.length !== n) return null;
    for (var k = 0; k < n; k++) {
      if (raw.marks[k] !== null && typeof raw.marks[k] !== "boolean") return null;
    }
    var hint = raw.hint;
    var revealed = raw.revealed;
    if (raw.i >= n) {
      hint = false;
      revealed = false;
    } else if (hint && teasers[raw.i].hint === undefined) {
      hint = false;
    }
    if (revealed) hint = false;
    return { i: raw.i, hint: hint, revealed: revealed, marks: raw.marks.slice() };
  }

  var SAMPLE = {
    title: "Brain Teasers",
    intro: "A tiny sample. Install a teaser pack for more.",
    teasers: [
      {
        question: "What comes next: 2, 4, 8, 16, and then what?",
        hint: "Look at how each number compares with the one before it.",
        answer: "32",
        explanation: "Each number is double the one before, so after 16 comes 32.",
        kind: "pattern",
      },
      {
        question:
          "A pencil case holds five pencils. Three of them are sharp. How many are not sharp?",
        answer: "Two",
        explanation: "Five pencils in total, take away the three sharp ones, and two are left.",
        kind: "math",
      },
    ],
  };

  root.TeaserLogic = {
    LIMITS: LIMITS,
    KINDS: KINDS,
    KIND_LABELS: KIND_LABELS,
    SAMPLE: SAMPLE,
    validTeasers: validTeasers,
    initialState: initialState,
    atEnd: atEnd,
    go: go,
    showHint: showHint,
    reveal: reveal,
    mark: mark,
    again: again,
    tally: tally,
    restoreState: restoreState,
  };
})(globalThis);
