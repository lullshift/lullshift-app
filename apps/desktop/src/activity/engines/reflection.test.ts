// Rules, schema and fixtures for the Reflection Prompts engine (lullshift.reflection@1).
/// <reference types="node" />
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { hostDataCheck } from "./hostDataCheck";
import { describe, expect, it } from "vitest";
import "../../../../../activities/engines/reflection/logic.js";

interface Pack {
  title: string;
  prompts: { text: string; followUp?: string }[];
  allowNotes?: boolean;
}
interface Logic {
  MAX_NOTE: number;
  validPack(c: unknown): boolean;
  clampIndex(i: unknown, count: number): number;
  cleanNotes(n: unknown, count: number): string[];
  setNote(n: string[], i: number, t: string): string[];
  hasNotes(n: string[]): boolean;
  clearNotes(count: number): string[];
}
const L = (globalThis as unknown as { ReflectionLogic: Logic }).ReflectionLogic;

const root = fileURLToPath(new URL("../../../../../", import.meta.url));
const readJson = (p: string): unknown => JSON.parse(readFileSync(p, "utf8"));
const schema = readJson(join(root, "activities/engines/reflection/data.schema.json"));
const validate = hostDataCheck(schema as object);

describe("ReflectionLogic", () => {
  it("accepts a good pack and rejects bad shapes", () => {
    expect(L.validPack({ title: "T", prompts: [{ text: "One?" }] })).toBe(true);
    expect(L.validPack({ title: "T", prompts: [] })).toBe(false);
    expect(L.validPack({ title: "T", prompts: [{ text: "x".repeat(201) }] })).toBe(false);
    expect(L.validPack({ title: "T", prompts: [{ text: "a", followUp: 3 }] })).toBe(false);
    expect(L.validPack({ title: "T", allowNotes: "yes", prompts: [{ text: "a" }] })).toBe(false);
    expect(L.validPack(null)).toBe(false);
  });

  it("clamps the index", () => {
    expect(L.clampIndex(2, 5)).toBe(2);
    expect(L.clampIndex(-3, 5)).toBe(0);
    expect(L.clampIndex(99, 5)).toBe(4);
    expect(L.clampIndex("2", 5)).toBe(0);
    expect(L.clampIndex(Number.NaN, 5)).toBe(0);
    expect(L.clampIndex(1.9, 5)).toBe(1);
  });

  it("cleans restored notes to one capped string per prompt", () => {
    const cleaned = L.cleanNotes(["a", 5, "b".repeat(2000)], 4);
    expect(cleaned).toHaveLength(4);
    expect(cleaned[0]).toBe("a");
    expect(cleaned[1]).toBe("");
    expect(cleaned[2]).toHaveLength(L.MAX_NOTE);
    expect(cleaned[3]).toBe("");
    expect(L.cleanNotes("nope", 2)).toEqual(["", ""]);
  });

  it("sets, detects and clears notes without mutating", () => {
    const empty = L.clearNotes(3);
    expect(L.hasNotes(empty)).toBe(false);
    const one = L.setNote(empty, 1, "hello");
    expect(empty[1]).toBe("");
    expect(one).toEqual(["", "hello", ""]);
    expect(L.hasNotes(one)).toBe(true);
    expect(L.setNote(empty, 0, "z".repeat(1500))[0]).toHaveLength(L.MAX_NOTE);
    expect(L.hasNotes(L.clearNotes(3))).toBe(false);
  });
});

describe("reflection data schema", () => {
  const dir = join(root, "tests/fixtures/engines/lullshift.reflection@1");
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

describe("bundled reflection packs", () => {
  const base = join(root, "activities/content/reflection");
  const packs = readdirSync(base);

  it("ships seven packs", () => {
    expect(packs).toHaveLength(7);
  });

  for (const slug of packs) {
    it(`${slug} is valid and follows the style guide basics`, () => {
      const manifest = readJson(join(base, slug, "manifest.json")) as { data: string };
      const data = readJson(join(base, slug, manifest.data)) as Pack;
      expect(validate(data), JSON.stringify(validate.errors)).toBe(true);
      expect(L.validPack(data)).toBe(true);
      for (const p of data.prompts) {
        expect(p.text).not.toMatch(/!/);
        expect(p.text).not.toMatch(/\b(your|their) (boss|manager|colleague|coworker)s?\b/i);
      }
    });
  }
});
