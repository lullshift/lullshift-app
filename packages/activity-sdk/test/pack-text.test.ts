// Security review LOW-2: every bundled engine's plain-text pattern refuses the host's hidden
// characters, and `packTextProblems` adds the combining-mark rule the pattern can't express.
// Together they must agree with the shared corpus, as the host does
// (crates/marketplace/src/tests/plain_text.rs runs the same corpus through the app's validator).
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createAjv, dataPath } from "@lullshift/agent-protocol";
import { describe, expect, it } from "vitest";
import { packTextProblems } from "../src/host/protocol";

interface Case {
  name: string;
  text: string;
  plain: boolean;
  reason: "hidden" | "marks" | null;
}

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const corpus = JSON.parse(
  readFileSync(path.join(root, "tests/fixtures/plain-text/cases.json"), "utf8"),
) as { cases: Case[] };

/** Every distinct plain-text pattern used by a bundled engine's data schema. */
function enginePatterns(): string[] {
  const found = new Set<string>();
  const dir = path.join(root, "activities/engines");
  const walk = (v: unknown): void => {
    if (Array.isArray(v)) v.forEach(walk);
    else if (v && typeof v === "object") {
      const o = v as Record<string, unknown>;
      if (typeof o.pattern === "string" && o.pattern.startsWith("^[^\\u0000-\\u001F")) {
        found.add(o.pattern);
      }
      Object.values(o).forEach(walk);
    }
  };
  for (const e of readdirSync(dir)) {
    const file = path.join(dir, e, "data.schema.json");
    if (existsSync(file)) walk(JSON.parse(readFileSync(file, "utf8")));
  }
  return [...found];
}

const patterns = enginePatterns();

describe("engine data schemas and the shared plain-text corpus", () => {
  it("all engines share the host's one plain-text pattern", () => {
    expect(patterns).toHaveLength(1);
  });

  it.each(corpus.cases)("pattern plus the text walk agree with the host: $name", (c) => {
    const validate = createAjv().compile({
      type: "object",
      properties: { field: { type: "string", pattern: patterns[0] } },
    });
    const byPattern = validate({ field: c.text });
    // The pattern refuses every hidden character; mark runs are the walk's half.
    expect(byPattern).toBe(c.plain || c.reason === "marks");
    const combined = byPattern && packTextProblems({ field: c.text }, dataPath).length === 0;
    expect(combined).toBe(c.plain);
  });
});

describe("packTextProblems", () => {
  const marks = "a" + String.fromCodePoint(0x301).repeat(5);
  const zwsp = String.fromCodePoint(0x200b);

  it("words problems the way the host does", () => {
    expect(
      packTextProblems(
        {
          cards: [{ front: "ok", back: marks }, { front: "tag" + String.fromCodePoint(0xe0041) }],
          ["we" + zwsp + "ird"]: 1,
        },
        dataPath,
      ),
    ).toEqual([
      "cards[0].back has more than four combining marks in a row, which isn't allowed",
      "cards[1].front contains a line break, tab or invisible character, which isn't allowed",
      'the field name ["we?ird"] contains a line break, tab or invisible character, which isn\'t allowed',
    ]);
    expect(packTextProblems("x" + String.fromCodePoint(0x2028), dataPath)).toEqual([
      "the data contains a line break, tab or invisible character, which isn't allowed",
    ]);
    expect(packTextProblems({ a: ["plain", 1, null, true] }, dataPath)).toEqual([]);
  });

  it("stops after twenty problems", () => {
    expect(packTextProblems(Array(100).fill(marks), dataPath)).toHaveLength(20);
  });
});
