import type { ErrorObject, ValidateFunction } from "ajv/dist/2020.js";
import { createAjv } from "./ajv.ts";
// The schemas in /reference, generated into a module so the published package carries them.
import {
  activityManifestSchema,
  agentEventSchema,
  marketplaceSchema,
} from "./generated/schemas.ts";
import type { AgentEventV1 } from "./generated/agent-event.ts";
import type { ActivityManifestV1, Help } from "./generated/activity-manifest.ts";
import type { MarketplaceIndexV1 } from "./generated/marketplace.ts";

export type { AgentEventV1, ActivityManifestV1, Help, MarketplaceIndexV1 };
export { activityManifestSchema, agentEventSchema, marketplaceSchema };

export type ValidationResult<T> = { ok: true; value: T } | { ok: false; errors: string[] };

export { createAjv } from "./ajv.ts";
export { dataPath, friendlyError, friendlyErrors } from "./friendly.ts";

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
