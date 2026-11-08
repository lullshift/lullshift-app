// Rule, schema and fixture tests for the Quick Quiz engine (activities/engines/quiz).
/// <reference types="node" />
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { hostDataCheck } from "./hostDataCheck";
import { describe, expect, it } from "vitest";

const root = fileURLToPath(new URL("../../../../../", import.meta.url));
new Function("window", readFileSync(join(root, "activities/engines/quiz/logic.js"), "utf8"))(
  globalThis,
);

interface Question {
  prompt: string;
  choices: string[];
  answer: number;
  explanation: string;
}
interface Quiz {
  title: string;
  intro?: string;
  shuffle?: boolean;
  roundSize?: number;
  questions: Question[];
}
interface Step {
  q: number;
  choices: number[];
}
interface State {
  seed: number;
  i: number;
  picked: number | null;
  results: boolean[];
  seen?: number[];
}
interface Logic {
  SAMPLE: Quiz;
  validQuiz(c: unknown): boolean;
  imageOf(q: unknown): { src: string; alt: string } | null;
  roundSizeOf(quiz: Quiz): number;
  draw(n: number, k: number, seen: unknown[], rnd: () => number): number[];
  remember(seen: number[], round: number[], n: number): number[];
  plan(quiz: Quiz, seed: number, seen?: number[]): Step[];
  correctPosition(quiz: Quiz, steps: Step[], i: number): number;
  initialState(seed: number, seen?: number[]): State;
  freshState(quiz: Quiz, seed: number): State;
  nextRound(quiz: Quiz, steps: Step[], state: State, seed: number): State;
  pick(quiz: Quiz, steps: Step[], state: State, shown: number): State | null;
  advance(steps: Step[], state: State): State | null;
  score(state: State): number;
  restoreState(quiz: Quiz, raw: unknown): State | null;
  closing(correct: number, total: number): string;
}
const L = (globalThis as unknown as { QuizLogic: Logic }).QuizLogic;

const readJson = (p: string): unknown => JSON.parse(readFileSync(p, "utf8"));
const schema = readJson(join(root, "activities/engines/quiz/data.schema.json"));
const validate = hostDataCheck(schema as object);

const q = (answer = 1): Question => ({
  prompt: "Pick one",
  choices: ["a", "b", "c"],
  answer,
  explanation: "Because.",
});
const quiz = (shuffle: boolean, n = 6): Quiz => ({
  title: "T",
  shuffle,
  questions: Array.from({ length: n }, (_, i) => q(i % 3)),
});

describe("QuizLogic plan", () => {
  it("keeps written order without the shuffle flag", () => {
    const steps = L.plan(quiz(false), 123);
    expect(steps.map((s) => s.q)).toEqual([0, 1, 2, 3, 4, 5]);
    expect(steps[0]?.choices).toEqual([0, 1, 2]);
  });

  it("is deterministic for a seed and varies between seeds when shuffled", () => {
    const z = quiz(true, 12);
    expect(L.plan(z, 7)).toEqual(L.plan(z, 7));
    expect(L.plan(z, 7)).not.toEqual(L.plan(z, 8));
  });

  it("is a permutation of questions and choices", () => {
    const steps = L.plan(quiz(true, 12), 99);
    expect([...steps.map((s) => s.q)].sort((a, b) => a - b)).toEqual([...Array(12).keys()]);
    for (const s of steps) expect([...s.choices].sort()).toEqual([0, 1, 2]);
  });

  it("finds the right answer wherever the choices were shuffled to", () => {
    const z = quiz(true, 12);
    const steps = L.plan(z, 5);
    steps.forEach((s, i) => {
      const shown = L.correctPosition(z, steps, i);
      expect(s.choices[shown]).toBe(z.questions[s.q]?.answer);
    });
  });
});

describe("QuizLogic play", () => {
  it("scores right and wrong picks and walks to the end", () => {
    const z = quiz(false, 3);
    const steps = L.plan(z, 1);
    let s = L.initialState(1);
    s = L.pick(z, steps, s, 0) as State; // answer is 0
    expect(s.results).toEqual([true]);
    expect(L.pick(z, steps, s, 1)).toBeNull(); // already answered
    s = L.advance(steps, s) as State;
    s = L.pick(z, steps, s, 0) as State; // answer is 1
    expect(s.results).toEqual([true, false]);
    s = L.advance(steps, s) as State;
    s = L.pick(z, steps, s, 2) as State; // answer is 2
    s = L.advance(steps, s) as State;
    expect(s.i).toBe(3);
    expect(L.score(s)).toBe(2);
    expect(L.advance(steps, s)).toBeNull();
    expect(L.pick(z, steps, s, 0)).toBeNull();
  });

  it("will not advance before answering or accept a bad choice", () => {
    const z = quiz(false, 2);
    const steps = L.plan(z, 1);
    const s = L.initialState(1);
    expect(L.advance(steps, s)).toBeNull();
    expect(L.pick(z, steps, s, 3)).toBeNull();
    expect(L.pick(z, steps, s, -1)).toBeNull();
    expect(L.pick(z, steps, s, 1.5)).toBeNull();
  });

  it("keeps its wording kind", () => {
    expect(L.closing(5, 5)).toMatch(/nicely done/i);
    expect(L.closing(0, 5)).not.toMatch(/fail|wrong|bad/i);
  });
});

describe("QuizLogic restoreState", () => {
  const z = quiz(true, 4);

  it("round-trips a state exactly through JSON", () => {
    const steps = L.plan(z, 42);
    let s = L.initialState(42);
    s = L.pick(z, steps, s, 1) as State;
    s = L.advance(steps, s) as State;
    s = L.pick(z, steps, s, 2) as State;
    const back = L.restoreState(z, JSON.parse(JSON.stringify(s)));
    expect(back).toEqual(s);
    expect(L.plan(z, (back as State).seed)).toEqual(steps);
  });

  it("rejects states that do not fit the quiz", () => {
    const bad: unknown[] = [
      null,
      "x",
      {},
      { seed: -1, i: 0, picked: null, results: [] },
      { seed: 1.5, i: 0, picked: null, results: [] },
      { seed: 1, i: 5, picked: null, results: [] },
      { seed: 1, i: 0, picked: 3, results: [true] },
      { seed: 1, i: 0, picked: null, results: [true] },
      { seed: 1, i: 1, picked: null, results: [] },
      { seed: 1, i: 0, picked: 0, results: ["yes"] },
      { seed: 1, i: 4, picked: 0, results: [true, true, true, true, true] },
    ];
    for (const b of bad) expect(L.restoreState(z, b)).toBeNull();
  });

  it("accepts the finished state", () => {
    const done = { seed: 3, i: 4, picked: null, results: [true, false, true, true] };
    expect(L.restoreState(z, done)).toEqual(done);
  });
});

describe("QuizLogic rounds (roundSize, ADR-028 section 10)", () => {
  const big = (roundSize?: number, shuffle = true, n = 12): Quiz => ({
    ...quiz(shuffle, n),
    ...(roundSize === undefined ? {} : { roundSize }),
  });
  const qs = (steps: Step[]) => steps.map((s) => s.q);
  // Plays a whole round, always picking the first shown choice.
  const playRound = (z: Quiz, steps: Step[], s: State): State => {
    while (s.i < steps.length) {
      s = L.pick(z, steps, s, 0) as State;
      s = L.advance(steps, s) as State;
    }
    return s;
  };

  it("leaves a quiz without roundSize exactly as before", () => {
    const z = big();
    expect(L.roundSizeOf(z)).toBe(0);
    expect(L.plan(z, 7)).toEqual(L.plan(z, 7, [3, 4, 5]));
    expect(L.freshState(z, 7)).toEqual({ seed: 7, i: 0, picked: null, results: [] });
    // Same shape after play and after "start again": no seen list appears.
    const steps = L.plan(z, 7);
    const done = playRound(z, steps, L.freshState(z, 7));
    expect(Object.keys(done).sort()).toEqual(["i", "picked", "results", "seed"]);
    expect(L.nextRound(z, steps, done, 9)).toEqual(L.initialState(9));
    // An old save restores unchanged.
    const old = { seed: 7, i: 2, picked: null, results: [true, false] };
    expect(L.restoreState(z, old)).toEqual(old);
  });

  it("is deterministic for a seed and asks roundSize distinct questions", () => {
    const z = big(8);
    expect(L.roundSizeOf(z)).toBe(8);
    const a = L.plan(z, 11, []);
    expect(a).toEqual(L.plan(z, 11, []));
    expect(a).toHaveLength(8);
    expect(new Set(qs(a)).size).toBe(8);
    for (const s of a) expect(s.q).toBeGreaterThanOrEqual(0);
    for (const s of a) expect(s.q).toBeLessThan(12);
    const seeds = [1, 2, 3, 4, 5].map((seed) => qs(L.plan(z, seed, [])).join());
    expect(new Set(seeds).size).toBeGreaterThan(1);
  });

  it("keeps written order without the shuffle flag, and shuffles choices only with it", () => {
    const z = big(5, false);
    const steps = L.plan(z, 3, []);
    expect(qs(steps)).toEqual([...qs(steps)].sort((x, y) => x - y));
    for (const s of steps) expect(s.choices).toEqual([0, 1, 2]);
  });

  it("prefers questions not asked in recent rounds", () => {
    const z = big(4);
    const seen = [0, 1, 2, 3, 4, 5, 6, 7];
    for (const seed of [1, 2, 3, 99, 4242]) {
      expect(qs(L.plan(z, seed, seen)).sort((x, y) => x - y)).toEqual([8, 9, 10, 11]);
    }
  });

  it("fills from the least recently seen when too few are unseen", () => {
    const z = big(5);
    // 10 and 11 are unseen; then the front of the list (least recent) fills the round.
    const seen = [7, 3, 9, 0, 1, 2, 4, 5, 6, 8];
    for (const seed of [1, 2, 3]) {
      expect(qs(L.plan(z, seed, seen)).sort((x, y) => x - y)).toEqual([3, 7, 9, 10, 11]);
    }
    // Nothing unseen at all: the five least recent.
    const all = [5, 6, 7, 8, 9, 10, 11, 0, 1, 2, 3, 4];
    expect(qs(L.plan(z, 1, all)).sort((x, y) => x - y)).toEqual([5, 6, 7, 8, 9]);
  });

  it("draws a whole pack once before repeating, and keeps the seen list bounded", () => {
    const z = big(5);
    let steps = L.plan(z, 1, []);
    let s = L.freshState(z, 1);
    const asked: number[] = [];
    for (let round = 0; round < 30; round++) {
      asked.push(...qs(steps));
      s = playRound(z, steps, s);
      s = L.nextRound(z, steps, s, round + 100);
      expect(s.i).toBe(0);
      expect(s.seen!.length).toBeLessThanOrEqual(12);
      expect(new Set(s.seen).size).toBe(s.seen!.length);
      steps = L.plan(z, s.seed, s.seen);
    }
    // Rounds one and two never repeat a question; with round three they cover the whole quiz.
    expect(new Set(asked.slice(0, 10)).size).toBe(10);
    expect(new Set(asked.slice(0, 15)).size).toBe(12);
    // The just-finished round is the most recent part of seen.
    expect(L.remember([1, 2, 3], [3, 4], 12)).toEqual([1, 2, 3, 4]);
    expect(L.remember([0, 1, 2, 3, 4], [5, 6, 0], 6)).toEqual([2, 3, 4, 5, 6, 0]);
  });

  it("ignores seen entries that don't fit when drawing", () => {
    const rnd = () => 0;
    expect(L.draw(4, 3, [9, -1, 1.5, "x", 0, 0], rnd)).toEqual([1, 2, 3]);
  });

  it("resumes exactly the same round at the same place after a save", () => {
    const z = big(6);
    const first = L.plan(z, 5, []);
    let s = L.freshState(z, 5);
    s = playRound(z, first, s);
    s = L.nextRound(z, first, s, 77);
    const steps = L.plan(z, s.seed, s.seen);
    s = L.pick(z, steps, s, 1) as State;
    s = L.advance(steps, s) as State;
    s = L.pick(z, steps, s, 2) as State;
    const saved = JSON.parse(JSON.stringify(s)) as unknown;
    const back = L.restoreState(z, saved) as State;
    expect(back).toEqual(s);
    expect(L.plan(z, back.seed, back.seen)).toEqual(steps);
  });

  it("refuses saves that don't fit a quiz with roundSize", () => {
    const z = big(6);
    const ok = { seed: 1, i: 0, picked: null, results: [], seen: [] };
    expect(L.restoreState(z, ok)).toEqual(ok);
    for (const bad of [
      { seed: 1, i: 0, picked: null, results: [] }, // an older save: starts a new round
      { ...ok, seen: [1, 1] },
      { ...ok, seen: [12] },
      { ...ok, seen: [-1] },
      { ...ok, seen: [0.5] },
      { ...ok, seen: "0,1" },
      { ...ok, seen: Array.from({ length: 13 }, (_, i) => i) },
      { ...ok, i: 7 },
      { ...ok, i: 6, results: [true, true, true, true, true, true, true] },
    ]) {
      expect(L.restoreState(z, bad), JSON.stringify(bad)).toBeNull();
    }
    expect(L.restoreState(z, { ...ok, i: 6, results: Array(6).fill(true) })).not.toBeNull();
  });

  it("treats a roundSize at or above the question count as the whole quiz", () => {
    for (const k of [12, 13, 40]) {
      const z = big(k);
      expect(L.roundSizeOf(z)).toBe(0);
      expect(L.plan(z, 4, [])).toEqual(L.plan(big(), 4));
      expect(L.validQuiz(z)).toBe(true);
    }
    expect(L.validQuiz(big(2))).toBe(false);
    expect(L.validQuiz(big(41))).toBe(false);
    expect(L.validQuiz(big(3.5))).toBe(false);
    expect(L.validQuiz({ ...big(), roundSize: "8" })).toBe(false);
  });
});

describe("QuizLogic validQuiz", () => {
  it("accepts the sample and rejects malformed quizzes", () => {
    expect(L.validQuiz(L.SAMPLE)).toBe(true);
    expect(validate(L.SAMPLE)).toBe(true);
    expect(L.validQuiz(null)).toBe(false);
    expect(L.validQuiz({ title: "T", questions: [] })).toBe(false);
    expect(L.validQuiz({ title: "T", questions: [{ ...q(), answer: 3 }] })).toBe(false);
    expect(L.validQuiz({ title: "T", questions: [{ ...q(), choices: ["only"] }] })).toBe(false);
    expect(L.validQuiz({ title: "T", shuffle: "yes", questions: [q()] })).toBe(false);
    expect(L.validQuiz({ title: "T", questions: [{ ...q(), explanation: "" }] })).toBe(false);
  });

  it("accepts a picture from the pack's images folder, with alt text, and nothing else", () => {
    const pic = { src: "images/hexagon.png", alt: "A shape with six sides." };
    const withPic = { title: "T", questions: [{ ...q(), image: pic }] };
    expect(L.validQuiz(withPic)).toBe(true);
    expect(validate(withPic)).toBe(true);
    expect(L.imageOf(withPic.questions[0])).toEqual(pic);
    expect(L.imageOf(q())).toBeNull();
    for (const image of [
      { src: "https://example.com/a.png", alt: "x" },
      { src: "data:image/png;base64,AAAA", alt: "x" },
      { src: "//example.com/a.png", alt: "x" },
      { src: "images/../index.html", alt: "x" },
      { src: "images/a.svg", alt: "x" },
      { src: "images/sub/a.png", alt: "x" },
      { src: "images/a.png", alt: "" },
      { src: "images/a.png" },
      { src: "images/a.png", alt: "x", onerror: "alert(1)" },
      "images/a.png",
      null,
    ]) {
      const quiz = { title: "T", questions: [{ ...q(), image }] };
      expect(L.validQuiz(quiz), JSON.stringify(image)).toBe(false);
      expect(validate(quiz), JSON.stringify(image)).toBe(false);
    }
  });
});

describe("lullshift.quiz@1 data schema and fixtures", () => {
  const dir = join(root, "tests/fixtures/engines/lullshift.quiz@1");
  const files = (kind: string): string[] =>
    existsSync(join(dir, kind))
      ? readdirSync(join(dir, kind)).filter((f) => f.endsWith(".json"))
      : [];

  it("has enough fixtures", () => {
    expect(files("valid").length).toBeGreaterThanOrEqual(3);
    expect(files("invalid").length).toBeGreaterThanOrEqual(4);
  });

  for (const f of files("valid")) {
    it(`valid/${f} validates and plays`, () => {
      const data = readJson(join(dir, "valid", f));
      expect(validate(data), JSON.stringify(validate.errors)).toBe(true);
      expect(L.validQuiz(data)).toBe(true);
    });
  }
  for (const f of files("invalid")) {
    it(`invalid/${f} is rejected`, () => {
      expect(validate(readJson(join(dir, "invalid", f)))).toBe(false);
    });
  }

  it("rejects an answer that points past the choices", () => {
    for (const choices of [2, 3, 4]) {
      const data = {
        title: "T",
        questions: [
          {
            prompt: "p",
            choices: Array.from({ length: choices }, (_, i) => `c${i}`),
            answer: choices,
            explanation: "e",
          },
        ],
      };
      expect(validate(data)).toBe(false);
    }
  });

  it("uses a closed schema", () => {
    const text = JSON.stringify(schema);
    expect(text).not.toContain('"additionalProperties":true');
  });
});

describe("bundled quiz packs", () => {
  const base = join(root, "activities/content/quiz");
  const packs = existsSync(base) ? readdirSync(base) : [];

  it("has eight packs", () => {
    expect(packs).toHaveLength(8);
  });

  for (const slug of packs) {
    it(`${slug} fits the schema, plays, and has varied answer positions`, () => {
      const manifest = readJson(join(base, slug, "manifest.json")) as { data: string };
      const data = readJson(join(base, slug, manifest.data)) as Quiz;
      expect(validate(data), JSON.stringify(validate.errors)).toBe(true);
      expect(data.questions.length).toBeGreaterThanOrEqual(8);
      expect(data.questions.length).toBeLessThanOrEqual(12);
      expect(new Set(data.questions.map((x) => x.answer)).size).toBeGreaterThan(1);
      // ADR-028 section 10: packs with 12 or more questions draw rounds.
      if (data.questions.length >= 12) expect(L.roundSizeOf(data)).toBe(data.roundSize);
      else expect(data.roundSize).toBeUndefined();
      for (const x of data.questions) expect(new Set(x.choices).size).toBe(x.choices.length);
    });
  }
});
