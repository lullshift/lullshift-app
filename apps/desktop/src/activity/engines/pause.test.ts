// Rule, schema and fixture tests for the Guided Pause engine (lullshift.pause@1).
/// <reference types="node" />
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createAjv } from "@lullshift/agent-protocol";
import { describe, expect, it } from "vitest";

const root = fileURLToPath(new URL("../../../../../", import.meta.url));
new Function("window", readFileSync(join(root, "activities/engines/pause/logic.js"), "utf8"))(
  globalThis,
);

interface Step {
  text: string;
  seconds: number;
  look?: string;
}
interface Content {
  title: string;
  intro?: string;
  steps: Step[];
  closing?: string;
}
interface Pos {
  index: number;
  elapsed: number;
}
interface Logic {
  LOOKS: string[];
  SAMPLE: Content;
  validContent(c: unknown): boolean;
  stepMs(c: Content, i: number): number;
  totalMs(c: Content): number;
  isDone(c: Content, p: Pos): boolean;
  start(): Pos;
  advance(c: Content, p: Pos, delta: number): Pos;
  next(c: Content, p: Pos): Pos;
  previous(c: Content, p: Pos): Pos;
  timeLeft(c: Content, p: Pos): number;
  secondsLeftInStep(c: Content, p: Pos): number;
  fraction(c: Content, p: Pos): number;
  formatTime(ms: number): string;
  validState(s: unknown, c: Content): boolean;
  makeState(p: Pos): { version: number; index: number; elapsed: number };
}
const L = (globalThis as unknown as { PauseLogic: Logic }).PauseLogic;

const readJson = (path: string) => JSON.parse(readFileSync(path, "utf8")) as unknown;
const schema = readJson(join(root, "activities/engines/pause/data.schema.json"));
const validate = createAjv({ allErrors: true }).compile(schema as object);

const three: Content = {
  title: "Three",
  steps: [
    { text: "One", seconds: 5 },
    { text: "Two", seconds: 10, look: "eyes" },
    { text: "Three", seconds: 5 },
  ],
};

describe("PauseLogic", () => {
  it("adds up step and total time", () => {
    expect(L.stepMs(three, 1)).toBe(10000);
    expect(L.totalMs(three)).toBe(20000);
    expect(L.timeLeft(three, L.start())).toBe(20000);
    expect(L.timeLeft(three, { index: 1, elapsed: 4000 })).toBe(11000);
  });

  it("moves on to the next step when a step's time runs out", () => {
    let p = L.start();
    for (let i = 0; i < 20; i++) p = L.advance(three, p, 250);
    expect(p).toEqual({ index: 1, elapsed: 0 });
    p = L.advance(three, { index: 0, elapsed: 4900 }, 250);
    expect(p).toEqual({ index: 1, elapsed: 150 });
  });

  it("finishes after the last step and stays finished", () => {
    const p = L.advance(three, { index: 2, elapsed: 4900 }, 250);
    expect(p).toEqual({ index: 3, elapsed: 0 });
    expect(L.isDone(three, p)).toBe(true);
    expect(L.advance(three, p, 250)).toEqual(p);
    expect(L.timeLeft(three, p)).toBe(0);
    expect(L.fraction(three, p)).toBe(1);
  });

  it("caps one tick at 250 ms so a throttled page never skips ahead", () => {
    expect(L.advance(three, L.start(), 60000)).toEqual({ index: 0, elapsed: 250 });
    expect(L.advance(three, L.start(), -5)).toEqual({ index: 0, elapsed: 0 });
  });

  it("skips ahead and goes back", () => {
    expect(L.next(three, { index: 0, elapsed: 2000 })).toEqual({ index: 1, elapsed: 0 });
    expect(L.next(three, { index: 2, elapsed: 0 })).toEqual({ index: 3, elapsed: 0 });
    expect(L.next(three, { index: 3, elapsed: 0 })).toEqual({ index: 3, elapsed: 0 });
    // Back restarts a step that has begun, then goes to the one before.
    expect(L.previous(three, { index: 1, elapsed: 3000 })).toEqual({ index: 1, elapsed: 0 });
    expect(L.previous(three, { index: 1, elapsed: 0 })).toEqual({ index: 0, elapsed: 0 });
    expect(L.previous(three, { index: 0, elapsed: 0 })).toEqual({ index: 0, elapsed: 0 });
    expect(L.previous(three, { index: 3, elapsed: 0 })).toEqual({ index: 2, elapsed: 0 });
  });

  it("reports progress within a step and whole seconds left", () => {
    expect(L.fraction(three, { index: 1, elapsed: 5000 })).toBe(0.5);
    expect(L.secondsLeftInStep(three, { index: 1, elapsed: 0 })).toBe(10);
    expect(L.secondsLeftInStep(three, { index: 1, elapsed: 9001 })).toBe(1);
    expect(L.formatTime(61000)).toBe("1:01");
  });

  it("round-trips its state exactly", () => {
    const s = L.makeState({ index: 1, elapsed: 4321.9 });
    expect(s).toEqual({ version: 1, index: 1, elapsed: 4321 });
    expect(L.validState(s, three)).toBe(true);
    expect(L.validState(L.makeState({ index: 3, elapsed: 0 }), three)).toBe(true);
    expect(JSON.parse(JSON.stringify(s))).toEqual(s);
  });

  it("rejects bad saved state", () => {
    expect(L.validState(null, three)).toBe(false);
    expect(L.validState({ version: 2, index: 0, elapsed: 0 }, three)).toBe(false);
    expect(L.validState({ version: 1, index: 4, elapsed: 0 }, three)).toBe(false);
    expect(L.validState({ version: 1, index: -1, elapsed: 0 }, three)).toBe(false);
    expect(L.validState({ version: 1, index: 0, elapsed: 5000 }, three)).toBe(false);
    expect(L.validState({ version: 1, index: 3, elapsed: 10 }, three)).toBe(false);
    expect(L.validState({ version: 1, index: 0, elapsed: 1.5 }, three)).toBe(false);
  });

  it("checks content the way the schema does", () => {
    expect(L.validContent(three)).toBe(true);
    expect(L.validContent(L.SAMPLE)).toBe(true);
    expect(L.validContent(null)).toBe(false);
    expect(L.validContent({ ...three, steps: [] })).toBe(false);
    expect(L.validContent({ ...three, steps: [{ text: "x", seconds: 4 }] })).toBe(false);
    expect(L.validContent({ ...three, steps: [{ text: "x", seconds: 10, look: "toes" }] })).toBe(
      false,
    );
    expect(L.validContent({ ...three, title: "" })).toBe(false);
  });
});

describe("data schema", () => {
  it("is a strict 2020-12 schema", () => {
    expect((schema as { $id: string }).$id).toBe("urn:lullshift:engine:pause:v1:data");
    expect((schema as { additionalProperties: boolean }).additionalProperties).toBe(false);
  });

  it("lists the same looks as the logic", () => {
    const looks = (
      schema as {
        properties: { steps: { items: { properties: { look: { enum: string[] } } } } };
      }
    ).properties.steps.items.properties.look.enum;
    expect(looks).toEqual(L.LOOKS);
  });

  it("agrees with the logic check on the built-in sample", () => {
    expect(validate(L.SAMPLE), JSON.stringify(validate.errors)).toBe(true);
  });
});

const fixtureDir = join(root, "tests/fixtures/engines/lullshift.pause@1");
const names = (kind: string) =>
  readdirSync(join(fixtureDir, kind)).filter((f) => f.endsWith(".json"));

describe("compatibility fixtures (lullshift.pause@1)", () => {
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

describe("bundled pause packs", () => {
  const packsDir = join(root, "activities/content/pause");
  const packs = readdirSync(packsDir);
  it("has five packs", () => {
    expect(packs).toHaveLength(5);
  });
  for (const slug of packs) {
    it(`${slug} is valid`, () => {
      const manifest = readJson(join(packsDir, slug, "manifest.json")) as { data: string };
      const data = readJson(join(packsDir, slug, manifest.data)) as Content;
      expect(validate(data), JSON.stringify(validate.errors)).toBe(true);
      expect(L.validContent(data)).toBe(true);
    });
  }
});
