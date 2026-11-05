// Rule, schema and fixture tests for the Breathing Space engine (lullshift.breathing@1).
/// <reference types="node" />
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createAjv } from "@lullshift/agent-protocol";
import { describe, expect, it } from "vitest";

const root = fileURLToPath(new URL("../../../../../", import.meta.url));
new Function("window", readFileSync(join(root, "activities/engines/breathing/logic.js"), "utf8"))(
  globalThis,
);

interface Phase {
  kind: string;
  seconds: number;
  cue?: string;
}
interface Content {
  title: string;
  intro?: string;
  phases: Phase[];
  rounds: number;
  closing?: string;
}
interface Pos {
  done: boolean;
  round: number;
  phaseIndex: number;
  phaseMs: number;
  phaseElapsed: number;
  fraction: number;
}
interface Logic {
  MIN_SCALE: number;
  MAX_SCALE: number;
  SAMPLE: Content;
  validContent(c: unknown): boolean;
  cycleMs(c: Content): number;
  totalMs(c: Content): number;
  levels(c: Content): { start: number; end: number }[];
  locate(c: Content, elapsed: number): Pos;
  scaleAt(c: Content, elapsed: number): number;
  cueFor(p: Phase): string;
  secondsLeft(pos: Pos): number;
  formatTime(ms: number): string;
  validState(s: unknown, c: Content): boolean;
  makeState(elapsed: number): { version: number; elapsed: number };
  advance(c: Content, elapsed: number, delta: number): number;
}
const L = (globalThis as unknown as { BreathingLogic: Logic }).BreathingLogic;

const readJson = (path: string) => JSON.parse(readFileSync(path, "utf8")) as unknown;
const schema = readJson(join(root, "activities/engines/breathing/data.schema.json"));
const validate = createAjv({ allErrors: true }).compile(schema as object);

const box: Content = {
  title: "Box",
  phases: [
    { kind: "inhale", seconds: 4 },
    { kind: "hold", seconds: 2 },
    { kind: "exhale", seconds: 4 },
    { kind: "hold", seconds: 2 },
  ],
  rounds: 2,
};

describe("BreathingLogic", () => {
  it("adds up cycle and total time", () => {
    expect(L.cycleMs(box)).toBe(12000);
    expect(L.totalMs(box)).toBe(24000);
  });

  it("locates the phase, round and fraction", () => {
    expect(L.locate(box, 0)).toMatchObject({ round: 1, phaseIndex: 0, fraction: 0, done: false });
    expect(L.locate(box, 2000)).toMatchObject({ phaseIndex: 0, fraction: 0.5 });
    expect(L.locate(box, 4000)).toMatchObject({ phaseIndex: 1, phaseElapsed: 0 });
    expect(L.locate(box, 11999)).toMatchObject({ round: 1, phaseIndex: 3 });
    expect(L.locate(box, 12000)).toMatchObject({ round: 2, phaseIndex: 0 });
    expect(L.locate(box, 24000).done).toBe(true);
    expect(L.locate(box, 99999)).toMatchObject({ done: true, round: 2 });
  });

  it("grows on inhale, shrinks on exhale and keeps its size on holds", () => {
    expect(L.scaleAt(box, 0)).toBeCloseTo(L.MIN_SCALE);
    expect(L.scaleAt(box, 4000)).toBeCloseTo(L.MAX_SCALE);
    expect(L.scaleAt(box, 5000)).toBeCloseTo(L.MAX_SCALE);
    expect(L.scaleAt(box, 8000)).toBeCloseTo(0.7);
    expect(L.scaleAt(box, 10000)).toBeCloseTo(L.MIN_SCALE);
    expect(L.scaleAt(box, 11000)).toBeCloseTo(L.MIN_SCALE);
    const mid = L.scaleAt(box, 2000);
    expect(mid).toBeGreaterThan(L.MIN_SCALE);
    expect(mid).toBeLessThan(L.MAX_SCALE);
  });

  it("starts a pattern that begins with a hold from the previous cycle's size", () => {
    const c: Content = {
      title: "H",
      phases: [
        { kind: "hold", seconds: 2 },
        { kind: "inhale", seconds: 3 },
        { kind: "exhale", seconds: 3 },
      ],
      rounds: 1,
    };
    expect(L.levels(c)[0]).toEqual({ start: L.MIN_SCALE, end: L.MIN_SCALE });
  });

  it("uses default cue words and honours pack cues", () => {
    expect(L.cueFor({ kind: "inhale", seconds: 4 })).toBe("Breathe in");
    expect(L.cueFor({ kind: "rest", seconds: 2 })).toBe("Rest");
    expect(L.cueFor({ kind: "inhale", seconds: 4, cue: "Easy in" })).toBe("Easy in");
  });

  it("counts seconds left and formats time", () => {
    expect(L.secondsLeft(L.locate(box, 0))).toBe(4);
    expect(L.secondsLeft(L.locate(box, 3001))).toBe(1);
    expect(L.formatTime(0)).toBe("0:00");
    expect(L.formatTime(61000)).toBe("1:01");
    expect(L.formatTime(61001)).toBe("1:02");
  });

  it("advances in capped steps and never past the end", () => {
    expect(L.advance(box, 0, 50)).toBe(50);
    expect(L.advance(box, 0, 5000)).toBe(250);
    expect(L.advance(box, 0, -10)).toBe(0);
    expect(L.advance(box, 23900, 250)).toBe(24000);
  });

  it("round-trips its state exactly", () => {
    const s = L.makeState(4321.4);
    expect(s).toEqual({ version: 1, elapsed: 4321 });
    expect(L.validState(s, box)).toBe(true);
    expect(JSON.parse(JSON.stringify(s))).toEqual(s);
  });

  it("rejects bad saved state", () => {
    expect(L.validState(null, box)).toBe(false);
    expect(L.validState({ version: 2, elapsed: 5 }, box)).toBe(false);
    expect(L.validState({ version: 1, elapsed: -1 }, box)).toBe(false);
    expect(L.validState({ version: 1, elapsed: 24001 }, box)).toBe(false);
    expect(L.validState({ version: 1, elapsed: 1.5 }, box)).toBe(false);
    expect(L.validState({ version: 1, elapsed: "5" }, box)).toBe(false);
  });

  it("checks content the way the schema does", () => {
    expect(L.validContent(box)).toBe(true);
    expect(L.validContent(L.SAMPLE)).toBe(true);
    expect(L.validContent(null)).toBe(false);
    expect(L.validContent({ ...box, rounds: 0 })).toBe(false);
    expect(L.validContent({ ...box, phases: [box.phases[0]] })).toBe(false);
    expect(
      L.validContent({ ...box, phases: [...box.phases.slice(0, 1), { kind: "hold", seconds: 5 }] }),
    ).toBe(false);
    expect(L.validContent({ ...box, title: "" })).toBe(false);
  });
});

describe("data schema", () => {
  it("is a strict 2020-12 schema", () => {
    expect((schema as { $id: string }).$id).toBe("urn:lullshift:engine:breathing:v1:data");
    expect((schema as { additionalProperties: boolean }).additionalProperties).toBe(false);
  });

  it("agrees with the logic check on the built-in sample", () => {
    expect(validate(L.SAMPLE), JSON.stringify(validate.errors)).toBe(true);
  });
});

const fixtureDir = join(root, "tests/fixtures/engines/lullshift.breathing@1");
const names = (kind: string) =>
  readdirSync(join(fixtureDir, kind)).filter((f) => f.endsWith(".json"));

describe("compatibility fixtures (lullshift.breathing@1)", () => {
  it("has enough fixtures", () => {
    expect(names("valid").length).toBeGreaterThanOrEqual(3);
    expect(names("invalid").length).toBeGreaterThanOrEqual(4);
  });
  for (const f of names("valid")) {
    it(`accepts valid/${f}`, () => {
      const data = readJson(join(fixtureDir, "valid", f));
      expect(validate(data), JSON.stringify(validate.errors)).toBe(true);
      expect(L.validContent(data)).toBe(true);
    });
  }
  for (const f of names("invalid")) {
    it(`rejects invalid/${f}`, () => {
      const data = readJson(join(fixtureDir, "invalid", f));
      expect(validate(data)).toBe(false);
    });
  }
});

describe("bundled breathing packs", () => {
  const packsDir = join(root, "activities/content/breathing");
  const packs = readdirSync(packsDir);
  it("has five packs", () => {
    expect(packs).toHaveLength(5);
  });
  for (const slug of packs) {
    it(`${slug} is valid and gentle`, () => {
      const manifest = readJson(join(packsDir, slug, "manifest.json")) as { data: string };
      const data = readJson(join(packsDir, slug, manifest.data)) as Content;
      expect(validate(data), JSON.stringify(validate.errors)).toBe(true);
      expect(L.validContent(data)).toBe(true);
      for (const p of data.phases) if (p.kind === "hold") expect(p.seconds).toBeLessThanOrEqual(4);
    });
  }
});
