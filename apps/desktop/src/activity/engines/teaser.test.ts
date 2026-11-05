// Rule, schema and fixture tests for the Brain Teasers engine (activities/engines/teaser).
/// <reference types="node" />
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createAjv } from "@lullshift/agent-protocol";
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
  teasers: Teaser[];
}
interface State {
  i: number;
  hint: boolean;
  revealed: boolean;
  marks: (boolean | null)[];
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
}
const L = (globalThis as unknown as { TeaserLogic: Logic }).TeaserLogic;

const readJson = (p: string): unknown => JSON.parse(readFileSync(p, "utf8"));
const schema = readJson(join(root, "activities/engines/teaser/data.schema.json"));
const validate = createAjv({ allErrors: true }).compile(schema as object);

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

  it("has five packs", () => {
    expect(packs).toHaveLength(5);
  });

  for (const slug of packs) {
    it(`${slug} fits the schema and is a sensible size`, () => {
      const manifest = readJson(join(base, slug, "manifest.json")) as { data: string };
      const data = readJson(join(base, slug, manifest.data)) as Pack;
      expect(validate(data), JSON.stringify(validate.errors)).toBe(true);
      expect(data.teasers.length).toBeGreaterThanOrEqual(6);
      expect(data.teasers.length).toBeLessThanOrEqual(10);
    });
  }
});
