// Shared list of canonical schemas (ADR-006). Paths are relative to the repo root.
import { fileURLToPath } from "node:url";
import path from "node:path";

export const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");

export const schemas = [
  {
    name: "agent-event",
    file: "reference/AGENT_EVENT_V1.schema.json",
    typeName: "AgentEventV1",
    exportName: "agentEventSchema",
  },
  {
    name: "activity-manifest",
    file: "reference/ACTIVITY_MANIFEST_V1.schema.json",
    typeName: "ActivityManifestV1",
    exportName: "activityManifestSchema",
  },
  {
    name: "marketplace",
    file: "reference/MARKETPLACE_V1.schema.json",
    typeName: "MarketplaceIndexV1",
    exportName: "marketplaceSchema",
  },
];

export const fixturesDir = (name) => path.join(repoRoot, "tests", "fixtures", name);
