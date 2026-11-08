// Pure rules for Quick Quiz (lullshift.quiz@1). No DOM. Attaches its API to globalThis.QuizLogic.
(function (root) {
  "use strict";

  var LIMITS = {
    title: 80,
    intro: 300,
    prompt: 200,
    choice: 100,
    explanation: 300,
    alt: 200,
    maxQuestions: 40,
    maxChoices: 5,
    minRound: 3,
  };

  function isText(v, max) {
    return typeof v === "string" && v.length > 0 && v.length <= max;
  }

  // A pack picture (ADR-016, Sprint 12): a file in the pack's own images/ folder, served by the
  // host beside this page, and alt text. Anything else (another folder, a URL, data:) is refused.
  var IMAGE_SRC = /^images\/[a-z0-9][a-z0-9-]{0,63}\.(?:png|webp)$/;

  function validImage(img) {
    if (!img || typeof img !== "object" || Array.isArray(img)) return false;
    var keys = Object.keys(img);
    for (var i = 0; i < keys.length; i++) if (keys[i] !== "src" && keys[i] !== "alt") return false;
    return typeof img.src === "string" && IMAGE_SRC.test(img.src) && isText(img.alt, LIMITS.alt);
  }

  // The question's picture, or null.
  function imageOf(q) {
    return q && q.image !== undefined && validImage(q.image)
      ? { src: q.image.src, alt: q.image.alt }
      : null;
  }

  // The host validates packs; this is a cheap second look so a bad pack never breaks the page.
  function validQuiz(c) {
    if (!c || typeof c !== "object" || !isText(c.title, LIMITS.title)) return false;
    if (c.intro !== undefined && !isText(c.intro, LIMITS.intro)) return false;
    if (c.shuffle !== undefined && typeof c.shuffle !== "boolean") return false;
    if (
      c.roundSize !== undefined &&
      (!Number.isInteger(c.roundSize) ||
        c.roundSize < LIMITS.minRound ||
        c.roundSize > LIMITS.maxQuestions)
    )
      return false;
    var qs = c.questions;
    if (!Array.isArray(qs) || qs.length < 1 || qs.length > LIMITS.maxQuestions) return false;
    for (var i = 0; i < qs.length; i++) {
      var q = qs[i];
      if (!q || typeof q !== "object" || !isText(q.prompt, LIMITS.prompt)) return false;
      if (!Array.isArray(q.choices) || q.choices.length < 2 || q.choices.length > LIMITS.maxChoices)
        return false;
      for (var k = 0; k < q.choices.length; k++) {
        if (!isText(q.choices[k], LIMITS.choice)) return false;
      }
      if (!Number.isInteger(q.answer) || q.answer < 0 || q.answer >= q.choices.length) return false;
      if (!isText(q.explanation, LIMITS.explanation)) return false;
      if (q.image !== undefined && !validImage(q.image)) return false;
    }
    return true;
  }

  function isSeed(s) {
    return Number.isInteger(s) && s >= 0 && s <= 4294967295;
  }

  // Small seeded generator (mulberry32) so a saved seed rebuilds exactly the same order.
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

  function shuffled(n, rnd) {
    var a = [];
    for (var i = 0; i < n; i++) a.push(i);
    // No generator means keep the order as written.
    for (var j = rnd ? n - 1 : 0; j > 0; j--) {
      var r = Math.floor(rnd() * (j + 1));
      var t = a[j];
      a[j] = a[r];
      a[r] = t;
    }
    return a;
  }

  // --- Rounds (ADR-028 section 10). With `roundSize`, each round asks that many questions drawn
  // with the seeded generator, preferring ones not asked in recent rounds. The snapshot keeps
  // `seen`: question indexes from earlier rounds, least recently asked first, each at most once,
  // so it never holds more than the question count (at most 40). The round itself is not saved:
  // the seed and `seen` rebuild it exactly, so a resume shows the same round at the same place.

  // The number of questions per round, or 0 when a round is the whole quiz. The schema can't
  // say "at most the question count", so a size at or above it is clamped to the whole quiz,
  // which plays exactly as a quiz without roundSize.
  function roundSizeOf(quiz) {
    var k = quiz.roundSize;
    return Number.isInteger(k) && k >= LIMITS.minRound && k < quiz.questions.length ? k : 0;
  }

  // A saved `seen` list fits a quiz of n questions: distinct whole indexes below n.
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

  // The play order: for each position, the question index and the order its choices are shown in.
  // Without the shuffle flag the order is exactly as written. With roundSize, `seen` steers which
  // questions the round asks; they keep the written order unless the quiz shuffles.
  function plan(quiz, seed, seen) {
    var k = roundSizeOf(quiz);
    if (k) return roundPlan(quiz, seed, seen || [], k);
    var out = [];
    var rnd = rng(seed);
    var qs = quiz.questions;
    var order = shuffled(qs.length, quiz.shuffle ? rnd : null);
    for (var p = 0; p < order.length; p++) {
      var q = qs[order[p]];
      out.push({ q: order[p], choices: shuffled(q.choices.length, quiz.shuffle ? rnd : null) });
    }
    return out;
  }

  function roundPlan(quiz, seed, seen, k) {
    var rnd = rng(seed);
    var qs = quiz.questions;
    var picked = draw(qs.length, k, seen, rnd);
    var order = quiz.shuffle
      ? shuffled(k, rnd).map(function (p) {
          return picked[p];
        })
      : picked.slice().sort(function (a, b) {
          return a - b;
        });
    var out = [];
    for (var p = 0; p < order.length; p++) {
      var q = qs[order[p]];
      out.push({ q: order[p], choices: shuffled(q.choices.length, quiz.shuffle ? rnd : null) });
    }
    return out;
  }

  // The displayed position of the right choice at a play position.
  function correctPosition(quiz, steps, i) {
    var step = steps[i];
    return step.choices.indexOf(quiz.questions[step.q].answer);
  }

  // Copies `seen` onto a new state when the old one has it (only quizzes with roundSize do).
  function carry(from, to) {
    if (from.seen) to.seen = from.seen.slice();
    return to;
  }

  function initialState(seed, seen) {
    var s = { seed: seed, i: 0, picked: null, results: [] };
    return seen ? carry({ seen: seen }, s) : s;
  }

  // A first round: nothing seen yet when the quiz has roundSize, the plain state otherwise.
  function freshState(quiz, seed) {
    return initialState(seed, roundSizeOf(quiz) ? [] : undefined);
  }

  // The next round after `state` (played with `steps`): its questions now count as seen.
  function nextRound(quiz, steps, state, seed) {
    if (!roundSizeOf(quiz)) return initialState(seed);
    var round = steps.map(function (s) {
      return s.q;
    });
    return initialState(seed, remember(state.seen || [], round, quiz.questions.length));
  }

  function pick(quiz, steps, state, shown) {
    if (state.i >= steps.length || state.picked !== null) return null;
    if (!Number.isInteger(shown) || shown < 0 || shown >= steps[state.i].choices.length)
      return null;
    var right = shown === correctPosition(quiz, steps, state.i);
    return carry(state, {
      seed: state.seed,
      i: state.i,
      picked: shown,
      results: state.results.concat([right]),
    });
  }

  function advance(steps, state) {
    if (state.i >= steps.length || state.picked === null) return null;
    return carry(state, {
      seed: state.seed,
      i: state.i + 1,
      picked: null,
      results: state.results.slice(),
    });
  }

  function score(state) {
    var n = 0;
    for (var i = 0; i < state.results.length; i++) if (state.results[i] === true) n++;
    return n;
  }

  // Returns a clean state, or null when a saved one does not fit this quiz. With roundSize the
  // state must carry a fitting `seen` list (an older save without one starts a new round).
  function restoreState(quiz, raw) {
    if (!raw || typeof raw !== "object" || !isSeed(raw.seed)) return null;
    var seen;
    if (roundSizeOf(quiz)) {
      if (!validSeen(raw.seen, quiz.questions.length)) return null;
      seen = raw.seen.slice();
    }
    var steps = plan(quiz, raw.seed, seen);
    var n = steps.length;
    if (!Number.isInteger(raw.i) || raw.i < 0 || raw.i > n) return null;
    var picked = raw.picked;
    if (picked !== null) {
      if (raw.i >= n || !Number.isInteger(picked)) return null;
      if (picked < 0 || picked >= steps[raw.i].choices.length) return null;
    }
    if (!Array.isArray(raw.results)) return null;
    var expected = raw.i + (picked === null ? 0 : 1);
    if (raw.results.length !== expected) return null;
    for (var k = 0; k < raw.results.length; k++)
      if (typeof raw.results[k] !== "boolean") return null;
    var out = { seed: raw.seed, i: raw.i, picked: picked, results: raw.results.slice() };
    return seen ? carry({ seen: seen }, out) : out;
  }

  // Kind, never scolding. Wording only; the score itself stays private to the person.
  function closing(correct, total) {
    if (total > 0 && correct === total) return "Every one. Nicely done.";
    if (correct * 2 >= total) return "A good run. There is always more to wonder about.";
    return "Plenty of new things to take away from that one.";
  }

  var SAMPLE = {
    title: "Quick Quiz",
    intro: "A tiny sample quiz. Install a quiz pack to play with more questions.",
    shuffle: false,
    questions: [
      {
        prompt: "Which of these is the largest planet in our solar system?",
        choices: ["Mars", "Jupiter", "Venus"],
        answer: 1,
        explanation:
          "Jupiter is the largest planet, big enough to hold over a thousand Earths by volume.",
      },
      {
        prompt: "How many sides does a hexagon have?",
        choices: ["Five", "Six", "Eight", "Ten"],
        answer: 1,
        explanation: "Hex means six, so a hexagon has six sides. Honeycomb cells are hexagons.",
      },
      {
        prompt: "Which of these is a primary colour of light?",
        choices: ["Green", "Orange", "Purple"],
        answer: 0,
        explanation: "Screens mix red, green and blue light to make every other colour.",
      },
    ],
  };

  root.QuizLogic = {
    LIMITS: LIMITS,
    SAMPLE: SAMPLE,
    validQuiz: validQuiz,
    imageOf: imageOf,
    isSeed: isSeed,
    roundSizeOf: roundSizeOf,
    draw: draw,
    remember: remember,
    plan: plan,
    correctPosition: correctPosition,
    initialState: initialState,
    freshState: freshState,
    nextRound: nextRound,
    pick: pick,
    advance: advance,
    score: score,
    restoreState: restoreState,
    closing: closing,
  };
})(globalThis);
