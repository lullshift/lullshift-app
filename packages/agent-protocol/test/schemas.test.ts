// The generated schemas module must equal the canonical files in /reference (ADR-006), key for
// key and in the same order: the validators and the published package use it instead of the JSON.
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { activityManifestSchema, agentEventSchema, marketplaceSchema } from "../src/index";

const reference = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../../reference");
const read = (file: string): unknown =>
  JSON.parse(readFileSync(path.join(reference, file), "utf8"));

describe("generated schemas", () => {
  it.each([
    ["AGENT_EVENT_V1.schema.json", agentEventSchema],
    ["ACTIVITY_MANIFEST_V1.schema.json", activityManifestSchema],
    ["MARKETPLACE_V1.schema.json", marketplaceSchema],
  ])("%s matches the module", (file, schema) => {
    expect(JSON.stringify(schema)).toBe(JSON.stringify(read(file)));
  });
});
