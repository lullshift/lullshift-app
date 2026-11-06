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

  // The play order: for each position, the question index and the order its choices are shown in.
  // Without the shuffle flag the order is exactly as written.
  function plan(quiz, seed) {
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

  // The displayed position of the right choice at a play position.
  function correctPosition(quiz, steps, i) {
    var step = steps[i];
    return step.choices.indexOf(quiz.questions[step.q].answer);
  }

  function initialState(seed) {
    return { seed: seed, i: 0, picked: null, results: [] };
  }

  function pick(quiz, steps, state, shown) {
    if (state.i >= steps.length || state.picked !== null) return null;
    if (!Number.isInteger(shown) || shown < 0 || shown >= steps[state.i].choices.length)
      return null;
    var right = shown === correctPosition(quiz, steps, state.i);
    return {
      seed: state.seed,
      i: state.i,
      picked: shown,
      results: state.results.concat([right]),
    };
  }

  function advance(steps, state) {
    if (state.i >= steps.length || state.picked === null) return null;
    return { seed: state.seed, i: state.i + 1, picked: null, results: state.results.slice() };
  }

  function score(state) {
    var n = 0;
    for (var i = 0; i < state.results.length; i++) if (state.results[i] === true) n++;
    return n;
  }

  // Returns a clean state, or null when a saved one does not fit this quiz.
  function restoreState(quiz, raw) {
    if (!raw || typeof raw !== "object" || !isSeed(raw.seed)) return null;
    var n = quiz.questions.length;
    if (!Number.isInteger(raw.i) || raw.i < 0 || raw.i > n) return null;
    var steps = plan(quiz, raw.seed);
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
    return { seed: raw.seed, i: raw.i, picked: picked, results: raw.results.slice() };
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
    plan: plan,
    correctPosition: correctPosition,
    initialState: initialState,
    pick: pick,
    advance: advance,
    score: score,
    restoreState: restoreState,
    closing: closing,
  };
})(globalThis);
