// The shared plain-text corpus (decision 18.7): `isPlainText` (used for summaries and by the
// CLI for names and authors) must accept and refuse exactly what the host does.
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { isPlainText } from "../src/host/protocol";

interface Case {
  name: string;
  text: string;
  plain: boolean;
}

const corpus = JSON.parse(
  readFileSync(
    path.resolve(
      path.dirname(fileURLToPath(import.meta.url)),
      "../../../tests/fixtures/plain-text/cases.json",
    ),
    "utf8",
  ),
) as { cases: Case[] };

const LEAF = String.fromCodePoint(0x1f33f);

describe("isPlainText and the shared corpus", () => {
  it.each(corpus.cases)("$name", (c) => {
    expect(isPlainText(c.text, 10_000)).toBe(c.plain);
  });

  it("still counts code points and refuses empty text", () => {
    expect(isPlainText("", 10)).toBe(false);
    expect(isPlainText(LEAF.repeat(2), 2)).toBe(true);
    expect(isPlainText(LEAF.repeat(3), 2)).toBe(false);
  });
});
