// The shared plain-text corpus (decision 18.7): the manifest and marketplace schemas must accept
// and refuse exactly what the host does. The Rust side runs the same file
// (crates/marketplace/src/tests/plain_text.rs, crates/agent-core/src/text.rs).
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { validateActivityManifest, validateMarketplaceIndex } from "../src/index";

interface Case {
  name: string;
  text: string;
  plain: boolean;
  reason: "hidden" | "marks" | null;
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

const manifest = (text: string) => ({
  schemaVersion: 1,
  kind: "app",
  id: "dev.lullshift.games.grid-logic",
  version: "0.1.0",
  name: text,
  description: text,
  author: text,
  category: "arcade",
  minHostVersion: "0.1.0",
  sdkVersion: "1.0.0",
  permissions: [],
  offline: true,
  entrypoint: "index.html",
  runtime: "isolated-web-v1",
  interruptibility: { pause: true, resume: true, persistState: true },
});

const index = (text: string) => ({
  schemaVersion: 1,
  name: "my-market",
  description: text,
  owner: text,
  activities: [],
});

describe("plain-text corpus", () => {
  it("has cases of every kind", () => {
    expect(corpus.cases.length).toBeGreaterThanOrEqual(50);
    expect(corpus.cases.some((c) => c.reason === "marks")).toBe(true);
    expect(corpus.cases.some((c) => c.plain)).toBe(true);
  });

  it.each(corpus.cases)("manifest schema agrees with the host: $name", (c) => {
    expect(validateActivityManifest(manifest(c.text)).ok).toBe(c.plain);
  });

  it.each(corpus.cases)("marketplace schema agrees with the host: $name", (c) => {
    expect(validateMarketplaceIndex(index(c.text)).ok).toBe(c.plain);
  });
});
