import type { ErrorObject, ValidateFunction } from "ajv/dist/2020.js";
import { createAjv } from "./ajv.ts";
import agentEventSchema from "../../../reference/AGENT_EVENT_V1.schema.json" with { type: "json" };
import activityManifestSchema from "../../../reference/ACTIVITY_MANIFEST_V1.schema.json" with { type: "json" };
import marketplaceSchema from "../../../reference/MARKETPLACE_V1.schema.json" with { type: "json" };
import type { AgentEventV1 } from "./generated/agent-event";
import type { ActivityManifestV1, Help } from "./generated/activity-manifest";
import type { MarketplaceIndexV1 } from "./generated/marketplace";

export type { AgentEventV1, ActivityManifestV1, Help, MarketplaceIndexV1 };

export type ValidationResult<T> = { ok: true; value: T } | { ok: false; errors: string[] };

export { createAjv } from "./ajv.ts";

const ajv = createAjv();

function makeValidator<T>(schema: object): (input: unknown) => ValidationResult<T> {
  const validate = ajv.compile(schema) as ValidateFunction<T>;
  return (input) =>
    validate(input)
      ? { ok: true, value: input }
      : { ok: false, errors: (validate.errors ?? []).map(formatError) };
}

function formatError(e: ErrorObject): string {
  return `${e.instancePath || "/"} ${e.message ?? "is invalid"}`;
}

export const validateAgentEvent = makeValidator<AgentEventV1>(agentEventSchema);
export const validateActivityManifest = makeValidator<ActivityManifestV1>(activityManifestSchema);
export const validateMarketplaceIndex = makeValidator<MarketplaceIndexV1>(marketplaceSchema);
