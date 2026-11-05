import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  validateActivityManifest,
  validateAgentEvent,
  validateMarketplaceIndex,
  type ValidationResult,
} from "../src/index";

const fixtures = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../../../tests/fixtures",
);

const cases: [string, (input: unknown) => ValidationResult<unknown>][] = [
  ["agent-event", validateAgentEvent],
  ["activity-manifest", validateActivityManifest],
  ["marketplace", validateMarketplaceIndex],
];

const load = (dir: string) =>
  readdirSync(dir)
    .filter((f) => f.endsWith(".json"))
    .map((f) => [f, JSON.parse(readFileSync(path.join(dir, f), "utf8"))] as const);

describe.each(cases)("%s fixtures", (name, validate) => {
  it.each(load(path.join(fixtures, name, "valid")))("accepts valid/%s", (_file, data) => {
    expect(validate(data)).toMatchObject({ ok: true });
  });

  it.each(load(path.join(fixtures, name, "invalid")))("rejects invalid/%s", (_file, data) => {
    const result = validate(data);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.errors.length).toBeGreaterThan(0);
  });
});
