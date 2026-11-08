// Rule, schema and fixture tests for the Brain Teasers engine (activities/engines/teaser).
/// <reference types="node" />
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { hostDataCheck } from "./hostDataCheck";
import { describe, expect, it } from "vitest";

const root = fileURLToPath(new URL("../../../../../", import.meta.url));
new Function("window", readFileSync(join(root, "activities/engines/teaser/logic.js"), "utf8"))(
  globalThis,
);

interface Teaser {
  question: string;
  hint?: string;
  answer: string;
  explanation: string;
  kind?: string;
}
interface Pack {
  title: string;
  intro?: string;
  roundSize?: number;
  teasers: Teaser[];
}
interface State {
  i: number;
  hint: boolean;
  revealed: boolean;
  marks: (boolean | null)[];
  seed?: number;
  seen?: number[];
}
interface Logic {
  SAMPLE: Pack;
  validTeasers(c: unknown): boolean;
  initialState(n: number): State;
  atEnd(s: State, n: number): boolean;
  go(s: State, n: number, to: number): State;
  showHint(s: State, n: number, teasers: Teaser[]): State;
  reveal(s: State, n: number): State;
  mark(s: State, n: number, value: boolean): State;
  again(n: number): State;
  tally(s: State): { got: number; missed: number };
  restoreState(teasers: Teaser[], raw: unknown): State | null;
  roundSizeOf(pack: Pack): number;
  roundOf(pack: Pack, s: { seed?: number; seen?: number[] }): number[];
  roundTeasers(pack: Pack, s: { seed?: number; seen?: number[] }): Teaser[];
  remember(seen: number[], round: number[], n: number): number[];
  freshState(pack: Pack, seed: number): State;
  nextRound(pack: Pack, s: State, seed: number): State;
  restorePack(pack: Pack, raw: unknown): State | null;
}
const L = (globalThis as unknown as { TeaserLogic: Logic }).TeaserLogic;

const readJson = (p: string): unknown => JSON.parse(readFileSync(p, "utf8"));
const schema = readJson(join(root, "activities/engines/teaser/data.schema.json"));
const validate = hostDataCheck(schema as object);

const teasers: Teaser[] = [
  { question: "q1", hint: "h1", answer: "a1", explanation: "e1" },
  { question: "q2", answer: "a2", explanation: "e2" },
  { question: "q3", hint: "h3", answer: "a3", explanation: "e3" },
];
const N = teasers.length;

describe("TeaserLogic play", () => {
  it("starts at the first teaser with everything hidden", () => {
    expect(L.initialState(N)).toEqual({
      i: 0,
      hint: false,
      revealed: false,
      marks: [null, null, null],
    });
  });

  it("shows a hint only when the teaser has one, and not after the reveal", () => {
    let s = L.initialState(N);
    s = L.showHint(s, N, teasers);
    expect(s.hint).toBe(true);
    const on = L.go(s, N, 1);
    expect(on.hint).toBe(false); // moving on hides it again
    expect(L.showHint(on, N, teasers)).toBe(on); // no hint on teaser 2
    const shown = L.reveal(L.initialState(N), N);
    expect(L.showHint(shown, N, teasers)).toBe(shown);
  });

  it("reveals the answer once, and marks only after a reveal", () => {
    const s0 = L.initialState(N);
    expect(L.mark(s0, N, true)).toBe(s0);
    const s1 = L.reveal(s0, N);
    expect(s1.revealed).toBe(true);
    expect(L.reveal(s1, N)).toBe(s1);
    const s2 = L.mark(s1, N, true);
    expect(s2.marks).toEqual([true, null, null]);
    expect(L.mark(s2, N, false).marks).toEqual([false, null, null]);
    expect(L.mark(s2, N, true).marks).toEqual([null, null, null]); // same mark clears it
    expect(L.mark(s1, N, "yes" as unknown as boolean)).toBe(s1);
  });

  it("does not mutate the state it is given", () => {
    const s = L.reveal(L.initialState(N), N);
    const frozen = JSON.stringify(s);
    L.mark(s, N, true);
    L.go(s, N, 2);
    expect(JSON.stringify(s)).toBe(frozen);
  });

  it("clamps movement and ends after the last teaser", () => {
    let s = L.initialState(N);
    expect(L.go(s, N, -4)).toBe(s);
    s = L.go(s, N, 99);
    expect(s.i).toBe(N);
    expect(L.atEnd(s, N)).toBe(true);
    expect(L.reveal(s, N)).toBe(s);
    expect(L.mark(s, N, true)).toBe(s);
  });

  it("keeps marks when moving, tallies them privately, and clears them on restart", () => {
    let s = L.reveal(L.initialState(N), N);
    s = L.mark(s, N, true);
    s = L.go(s, N, 1);
    s = L.mark(L.reveal(s, N), N, false);
    s = L.go(s, N, N);
    expect(L.tally(s)).toEqual({ got: 1, missed: 1 });
    expect(L.tally(L.again(N))).toEqual({ got: 0, missed: 0 });
    expect(L.again(N).i).toBe(0);
  });
});

describe("TeaserLogic restoreState", () => {
  it("round-trips a state through JSON", () => {
    let s = L.reveal(L.initialState(N), N);
    s = L.mark(s, N, false);
    s = L.go(s, N, 2);
    s = L.showHint(s, N, teasers);
    expect(L.restoreState(teasers, JSON.parse(JSON.stringify(s)))).toEqual(s);
  });

  it("rejects states that do not fit the pack", () => {
    const bad: unknown[] = [
      null,
      7,
      {},
      { i: -1, hint: false, revealed: false, marks: [null, null, null] },
      { i: 4, hint: false, revealed: false, marks: [null, null, null] },
      { i: 0, hint: "no", revealed: false, marks: [null, null, null] },
      { i: 0, hint: false, revealed: false, marks: [null, null] },
      { i: 0, hint: false, revealed: false, marks: [null, "x", null] },
    ];
    for (const b of bad) expect(L.restoreState(teasers, b)).toBeNull();
  });

  it("repairs impossible combinations instead of trusting them", () => {
    const noHint = L.restoreState(teasers, {
      i: 1,
      hint: true,
      revealed: false,
      marks: [null, null, null],
    });
    expect(noHint?.hint).toBe(false);
    const both = L.restoreState(teasers, {
      i: 0,
      hint: true,
      revealed: true,
      marks: [null, null, null],
    });
    expect(both?.hint).toBe(false);
    expect(both?.revealed).toBe(true);
    const end = L.restoreState(teasers, {
      i: 3,
      hint: true,
      revealed: true,
      marks: [true, null, false],
    });
    expect(end).toEqual({ i: 3, hint: false, revealed: false, marks: [true, null, false] });
  });
});

describe("TeaserLogic rounds (roundSize, ADR-028 section 10)", () => {
  const pack = (roundSize?: number, n = 12): Pack => ({
    title: "T",
    ...(roundSize === undefined ? {} : { roundSize }),
    teasers: Array.from({ length: n }, (_, i) => ({
      question: `q${i}`,
      ...(i % 2 ? { hint: `h${i}` } : {}),
      answer: `a${i}`,
      explanation: `e${i}`,
    })),
  });
  const sorted = (a: number[]) => [...a].sort((x, y) => x - y);

  it("leaves a pack without roundSize exactly as before", () => {
    const p = pack();
    expect(L.roundSizeOf(p)).toBe(0);
    expect(L.freshState(p, 5)).toEqual(L.initialState(12));
    expect(L.roundTeasers(p, {})).toEqual(p.teasers);
    const played = L.mark(L.reveal(L.initialState(12), 12), 12, true);
    expect(L.nextRound(p, played, 5)).toEqual(L.again(12));
    // An old save restores unchanged, extra fields or not.
    const old = { i: 2, hint: false, revealed: true, marks: Array(12).fill(null) };
    expect(L.restorePack(p, old)).toEqual(old);
    expect(L.restorePack(p, { ...old, seed: 3, seen: [1] })).toEqual(old);
  });

  it("is deterministic for a seed, draws roundSize teasers and keeps the written order", () => {
    const p = pack(5);
    const a = L.roundOf(p, { seed: 9, seen: [] });
    expect(a).toEqual(L.roundOf(p, { seed: 9, seen: [] }));
    expect(a).toHaveLength(5);
    expect(new Set(a).size).toBe(5);
    expect(a).toEqual(sorted(a));
    const s = L.freshState(p, 9);
    expect(s).toEqual({ i: 0, hint: false, revealed: false, marks: Array(5).fill(null), seed: 9, seen: [] });
    const rounds = [1, 2, 3, 4, 5].map((seed) => L.roundOf(p, { seed, seen: [] }).join());
    expect(new Set(rounds).size).toBeGreaterThan(1);
  }); // prettier-ignore

  it("prefers unseen teasers, then fills from the least recently seen", () => {
    const p = pack(4);
    for (const seed of [1, 2, 3, 77]) {
      expect(L.roundOf(p, { seed, seen: [0, 1, 2, 3, 4, 5, 6, 7] })).toEqual([8, 9, 10, 11]);
      expect(L.roundOf(p, { seed, seen: [6, 2, 0, 1, 3, 4, 5, 7, 8, 9] })).toEqual([2, 6, 10, 11]);
    }
  });

  it("moves through rounds with a bounded seen list", () => {
    const p = pack(5);
    let s = L.freshState(p, 1);
    const shown: number[] = [];
    for (let round = 0; round < 20; round++) {
      shown.push(...L.roundOf(p, s));
      s = L.go(s, 5, 5);
      s = L.nextRound(p, s, round + 50);
      expect(s.i).toBe(0);
      expect(s.marks).toEqual(Array(5).fill(null));
      expect(s.seen!.length).toBeLessThanOrEqual(12);
      expect(new Set(s.seen).size).toBe(s.seen!.length);
    }
    expect(new Set(shown.slice(0, 10)).size).toBe(10);
    expect(new Set(shown.slice(0, 15)).size).toBe(12);
    expect(L.remember([4, 5], [5, 6], 12)).toEqual([4, 5, 6]);
  });

  it("carries the round through play and resumes the same round at the same place", () => {
    const p = pack(4);
    let s = L.nextRound(p, L.freshState(p, 3), 8);
    const items = L.roundTeasers(p, s);
    s = L.go(s, 4, 1);
    s = L.reveal(s, 4);
    s = L.mark(s, 4, false);
    s = L.go(s, 4, 2);
    // Teaser 2 of the round has a hint only if its pack index is odd.
    if (items[2]!.hint !== undefined) s = L.showHint(s, 4, items);
    expect(s.seed).toBe(8);
    const back = L.restorePack(p, JSON.parse(JSON.stringify(s)));
    expect(back).toEqual(s);
    expect(L.roundTeasers(p, back!)).toEqual(items);
  });

  it("refuses saves that don't fit a pack with roundSize", () => {
    const p = pack(4);
    const ok = L.freshState(p, 2);
    expect(L.restorePack(p, ok)).toEqual(ok);
    for (const bad of [
      { i: 0, hint: false, revealed: false, marks: Array(12).fill(null) }, // older save
      { ...ok, seed: -1 },
      { ...ok, seed: 2.5 },
      { ...ok, seen: [0, 0] },
      { ...ok, seen: [12] },
      { ...ok, seen: Array.from({ length: 13 }, (_, i) => i % 12) },
      { ...ok, marks: Array(12).fill(null) },
      { ...ok, i: 5 },
    ]) {
      expect(L.restorePack(p, bad), JSON.stringify(bad)).toBeNull();
    }
  });

  it("treats a roundSize at or above the teaser count as the whole pack", () => {
    for (const k of [12, 40]) {
      expect(L.roundSizeOf(pack(k))).toBe(0);
      expect(L.freshState(pack(k), 1)).toEqual(L.initialState(12));
      expect(L.validTeasers(pack(k))).toBe(true);
    }
    expect(L.validTeasers(pack(2))).toBe(false);
    expect(L.validTeasers(pack(41))).toBe(false);
    expect(L.validTeasers({ ...pack(), roundSize: 4.5 })).toBe(false);
  });
});

describe("TeaserLogic validTeasers", () => {
  it("accepts the sample and rejects malformed packs", () => {
    expect(L.validTeasers(L.SAMPLE)).toBe(true);
    expect(validate(L.SAMPLE)).toBe(true);
    expect(L.validTeasers(undefined)).toBe(false);
    expect(L.validTeasers({ title: "T", teasers: [] })).toBe(false);
    expect(L.validTeasers({ title: "T", teasers: [{ question: "q", answer: "a" }] })).toBe(false);
    expect(L.validTeasers({ title: "T", teasers: [{ ...teasers[0], kind: "trivia" }] })).toBe(
      false,
    );
  });
});

describe("lullshift.teaser@1 data schema and fixtures", () => {
  const dir = join(root, "tests/fixtures/engines/lullshift.teaser@1");
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
      expect(L.validTeasers(data)).toBe(true);
    });
  }
  for (const f of files("invalid")) {
    it(`invalid/${f} is rejected`, () => {
      expect(validate(readJson(join(dir, "invalid", f)))).toBe(false);
    });
  }

  it("uses a closed schema", () => {
    expect(JSON.stringify(schema)).not.toContain('"additionalProperties":true');
  });
});

describe("bundled teaser packs", () => {
  const base = join(root, "activities/content/teaser");
  const packs = existsSync(base) ? readdirSync(base) : [];

  it("has seven packs", () => {
    expect(packs).toHaveLength(7);
  });

  for (const slug of packs) {
    it(`${slug} fits the schema and is a sensible size`, () => {
      const manifest = readJson(join(base, slug, "manifest.json")) as { data: string };
      const data = readJson(join(base, slug, manifest.data)) as Pack;
      expect(validate(data), JSON.stringify(validate.errors)).toBe(true);
      expect(data.teasers.length).toBeGreaterThanOrEqual(6);
      expect(data.teasers.length).toBeLessThanOrEqual(10);
      // ADR-028 section 10: packs with 12 or more teasers would draw rounds; these have fewer.
      expect(data.roundSize).toBeUndefined();
    });
  }
});
