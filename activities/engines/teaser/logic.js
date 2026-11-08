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
    minRound: 3,
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
    if (
      c.roundSize !== undefined &&
      (!Number.isInteger(c.roundSize) ||
        c.roundSize < LIMITS.minRound ||
        c.roundSize > LIMITS.maxTeasers)
    )
      return false;
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

  // Copies the round fields (only packs with roundSize have them) onto a new state.
  function carry(from, to) {
    if (from.seen) {
      to.seed = from.seed;
      to.seen = from.seen.slice();
    }
    return to;
  }

  function copy(s) {
    return carry(s, { i: s.i, hint: s.hint, revealed: s.revealed, marks: s.marks.slice() });
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

  // --- Rounds (ADR-028 section 10). With `roundSize`, each round shows that many teasers drawn
  // with a seeded generator, preferring ones not shown in recent rounds, in the pack's written
  // order. The state then also carries `seed` and `seen` (teaser indexes from earlier rounds,
  // least recently shown first, each at most once, so never more than the teaser count). The
  // round is not saved: the seed and `seen` rebuild it exactly, so a resume shows the same
  // round at the same place. Positions (`i`, `marks`) count within the round.

  function isSeed(s) {
    return Number.isInteger(s) && s >= 0 && s <= 4294967295;
  }

  // Small seeded generator (mulberry32), the same one Quick Quiz uses.
  function rng(seed) {
    var a = seed >>> 0;
    return function () {
      a = (a + 0x6d2b79f5) >>> 0;
      var t = a;
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }

  // Teasers per round, or 0 when a round is the whole pack. The schema can't say "at most the
  // teaser count", so a size at or above it is clamped to the whole pack, which plays exactly as
  // a pack without roundSize.
  function roundSizeOf(pack) {
    var k = pack.roundSize;
    return Number.isInteger(k) && k >= LIMITS.minRound && k < pack.teasers.length ? k : 0;
  }

  // A saved `seen` list fits a pack of n teasers: distinct whole indexes below n.
  function validSeen(raw, n) {
    if (!Array.isArray(raw) || raw.length > n) return false;
    var at = {};
    for (var i = 0; i < raw.length; i++) {
      var v = raw[i];
      if (!Number.isInteger(v) || v < 0 || v >= n || at[v]) return false;
      at[v] = true;
    }
    return true;
  }

  // k distinct indexes below n: first a seeded pick among those not in `seen`, then, if too few
  // are left, the least recently seen (the front of `seen`). Entries that don't fit are ignored.
  function draw(n, k, seen, rnd) {
    var at = {};
    var old = [];
    for (var s = 0; s < seen.length; s++) {
      var v = seen[s];
      if (Number.isInteger(v) && v >= 0 && v < n && !at[v]) {
        at[v] = true;
        old.push(v);
      }
    }
    var fresh = [];
    for (var i = 0; i < n; i++) if (!at[i]) fresh.push(i);
    var out = [];
    for (var j = 0; j < fresh.length && out.length < k; j++) {
      var r = j + Math.floor(rnd() * (fresh.length - j));
      var t = fresh[j];
      fresh[j] = fresh[r];
      fresh[r] = t;
      out.push(fresh[j]);
    }
    for (var o = 0; o < old.length && out.length < k; o++) out.push(old[o]);
    return out;
  }

  // `seen` after a round: the round's indexes move to the end (most recent), each kept once,
  // and the list is cut from the front to at most n entries.
  function remember(seen, round, n) {
    var inRound = {};
    for (var r = 0; r < round.length; r++) inRound[round[r]] = true;
    var out = [];
    for (var s = 0; s < seen.length; s++) if (!inRound[seen[s]]) out.push(seen[s]);
    for (var k = 0; k < round.length; k++) out.push(round[k]);
    return out.slice(Math.max(0, out.length - n));
  }

  // The teaser indexes of the state's round, in play order. Without roundSize, the whole pack.
  function roundOf(pack, s) {
    var n = pack.teasers.length;
    var k = roundSizeOf(pack);
    if (!k) {
      var all = [];
      for (var i = 0; i < n; i++) all.push(i);
      return all;
    }
    return draw(n, k, s.seen || [], rng(s.seed)).sort(function (a, b) {
      return a - b;
    });
  }

  // A round's teaser objects, the list the play rules above work on.
  function roundTeasers(pack, s) {
    return roundOf(pack, s).map(function (i) {
      return pack.teasers[i];
    });
  }

  // A first round: nothing seen yet. Without roundSize, the plain state.
  function freshState(pack, seed) {
    var k = roundSizeOf(pack);
    if (!k) return initialState(pack.teasers.length);
    return carry({ seed: seed, seen: [] }, initialState(k));
  }

  // The next round after `s`: its teasers now count as seen. Without roundSize, start again.
  function nextRound(pack, s, seed) {
    var k = roundSizeOf(pack);
    if (!k) return again(pack.teasers.length);
    var seen = remember(s.seen || [], roundOf(pack, s), pack.teasers.length);
    return carry({ seed: seed, seen: seen }, initialState(k));
  }

  // A saved state for this pack, or null. Without roundSize this is exactly restoreState; with
  // it, the state must also carry a seed and a fitting `seen` list (an older save without them
  // starts a new round).
  function restorePack(pack, raw) {
    if (!roundSizeOf(pack)) return restoreState(pack.teasers, raw);
    if (!raw || typeof raw !== "object" || !isSeed(raw.seed)) return null;
    if (!validSeen(raw.seen, pack.teasers.length)) return null;
    var round = { seed: raw.seed, seen: raw.seen.slice() };
    var base = restoreState(roundTeasers(pack, round), raw);
    return base ? carry(round, base) : null;
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
    roundSizeOf: roundSizeOf,
    draw: draw,
    remember: remember,
    roundOf: roundOf,
    roundTeasers: roundTeasers,
    freshState: freshState,
    nextRound: nextRound,
    restorePack: restorePack,
  };
})(globalThis);
