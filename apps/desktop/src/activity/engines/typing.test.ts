// Rules, schema and fixtures for the Typing Drills engine (lullshift.typing@1).
/// <reference types="node" />
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createAjv } from "@lullshift/agent-protocol";
import { describe, expect, it } from "vitest";
import "../../../../../activities/engines/typing/logic.js";

interface Stats {
  done: number;
  chars: number;
  correct: number;
  activeMs: number;
}
interface Logic {
  validPack(c: unknown): boolean;
  compare(target: string, typed: string): string[];
  countOf(statuses: string[], what: string): number;
  isComplete(target: string, typed: string): boolean;
  mistakeText(n: number): string;
  emptyStats(): Stats;
  addLine(s: Stats, chars: number, correct: number, ms: number): Stats;
  accuracy(s: Stats): number | null;
  wpm(s: Stats): number | null;
  activeMs(banked: number, since: number | null, now: number): number;
  cleanStats(raw: unknown, lineCount: number): Stats;
}
const L = (globalThis as unknown as { TypingLogic: Logic }).TypingLogic;

const root = fileURLToPath(new URL("../../../../../", import.meta.url));
const readJson = (p: string): unknown => JSON.parse(readFileSync(p, "utf8"));
const schema = readJson(join(root, "activities/engines/typing/data.schema.json"));
const validate = createAjv({ allErrors: true }).compile(schema as object);

describe("TypingLogic", () => {
  it("marks each character as correct, wrong or pending", () => {
    expect(L.compare("cat", "")).toEqual(["pending", "pending", "pending"]);
    expect(L.compare("cat", "cx")).toEqual(["correct", "wrong", "pending"]);
    expect(L.compare("cat", "cat")).toEqual(["correct", "correct", "correct"]);
    expect(L.compare("cat", "CAT")).toEqual(["wrong", "wrong", "wrong"]);
    expect(L.countOf(L.compare("cat", "cxz"), "wrong")).toBe(2);
  });

  it("knows when a line is complete", () => {
    expect(L.isComplete("cat", "ca")).toBe(false);
    expect(L.isComplete("cat", "cat")).toBe(true);
  });

  it("describes mistakes in words", () => {
    expect(L.mistakeText(0)).toBe("no mistakes");
    expect(L.mistakeText(1)).toBe("1 mistake");
    expect(L.mistakeText(3)).toBe("3 mistakes");
  });

  it("computes accuracy over finished lines only", () => {
    const s0 = L.emptyStats();
    expect(L.accuracy(s0)).toBeNull();
    const s1 = L.addLine(s0, 20, 19, 6000);
    expect(L.accuracy(s1)).toBe(95);
    const s2 = L.addLine(s1, 20, 20, 6000);
    expect(L.accuracy(s2)).toBe(98);
    expect(s2.done).toBe(2);
    expect(s0.done).toBe(0);
  });

  it("computes words per minute from correct characters and active time", () => {
    // 50 correct characters is ten words; in 60 seconds that is 10 wpm.
    expect(L.wpm({ done: 1, chars: 50, correct: 50, activeMs: 60000 })).toBe(10);
    expect(L.wpm({ done: 1, chars: 50, correct: 25, activeMs: 30000 })).toBe(10);
    expect(L.wpm({ done: 1, chars: 5, correct: 5, activeMs: 500 })).toBeNull();
    expect(L.wpm(L.emptyStats())).toBeNull();
  });

  it("only counts time while the clock is running", () => {
    expect(L.activeMs(1000, null, 99999)).toBe(1000);
    expect(L.activeMs(1000, 5000, 7500)).toBe(3500);
    expect(L.activeMs(0, 9000, 8000)).toBe(0);
  });

  it("cleans restored stats", () => {
    expect(L.cleanStats(null, 5)).toEqual(L.emptyStats());
    expect(L.cleanStats({ done: 99, chars: 100, correct: 500, activeMs: -4 }, 5)).toEqual({
      done: 5,
      chars: 100,
      correct: 100,
      activeMs: 0,
    });
    expect(L.cleanStats({ done: "x", chars: Infinity }, 5)).toEqual(L.emptyStats());
  });

  it("accepts a good pack and rejects bad shapes", () => {
    expect(L.validPack({ title: "T", lines: ["abc"] })).toBe(true);
    expect(L.validPack({ title: "T", lines: [] })).toBe(false);
    expect(L.validPack({ title: "T", lines: ["café"] })).toBe(false);
    expect(L.validPack({ title: "T", lines: ["a".repeat(121)] })).toBe(false);
    expect(L.validPack({ title: "T", lines: ["a"], mode: "fast" })).toBe(false);
    expect(L.validPack({ title: "T", lines: ["a"], mode: "flow" })).toBe(true);
  });

  it("keeps saved stats free of typed text", () => {
    const s = L.addLine(L.emptyStats(), 10, 9, 4000);
    expect(Object.keys(s).sort()).toEqual(["activeMs", "chars", "correct", "done"]);
  });
});

describe("typing data schema", () => {
  const dir = join(root, "tests/fixtures/engines/lullshift.typing@1");
  const list = (kind: string) => readdirSync(join(dir, kind)).filter((f) => f.endsWith(".json"));

  it("has the fixtures the compatibility check expects", () => {
    expect(list("valid").length).toBeGreaterThanOrEqual(3);
    expect(list("invalid").length).toBeGreaterThanOrEqual(4);
  });

  for (const f of list("valid")) {
    it(`accepts valid/${f}`, () => {
      const data = readJson(join(dir, "valid", f));
      expect(validate(data), JSON.stringify(validate.errors)).toBe(true);
      expect(L.validPack(data)).toBe(true);
    });
  }
  for (const f of list("invalid")) {
    it(`rejects invalid/${f}`, () => {
      expect(validate(readJson(join(dir, "invalid", f)))).toBe(false);
    });
  }
});

describe("bundled typing packs", () => {
  const base = join(root, "activities/content/typing");
  const packs = readdirSync(base);

  it("ships five packs", () => {
    expect(packs).toHaveLength(5);
  });

  for (const slug of packs) {
    it(`${slug} is valid`, () => {
      const manifest = readJson(join(base, slug, "manifest.json")) as { data: string };
      const data = readJson(join(base, slug, manifest.data)) as { lines: string[] };
      expect(validate(data), JSON.stringify(validate.errors)).toBe(true);
      expect(L.validPack(data)).toBe(true);
      for (const line of data.lines) expect(line).toBe(line.trim());
    });
  }
});
