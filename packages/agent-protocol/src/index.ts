import { Ajv2020, type ErrorObject, type ValidateFunction } from "ajv/dist/2020.js";
import addFormatsModule from "ajv-formats";
import agentEventSchema from "../../../reference/AGENT_EVENT_V1.schema.json";
import activityManifestSchema from "../../../reference/ACTIVITY_MANIFEST_V1.schema.json";
import marketplaceSchema from "../../../reference/MARKETPLACE_V1.schema.json";
import type { AgentEventV1 } from "./generated/agent-event";
import type { ActivityManifestV1 } from "./generated/activity-manifest";
import type { MarketplaceIndexV1 } from "./generated/marketplace";

export type { AgentEventV1, ActivityManifestV1, MarketplaceIndexV1 };

export type ValidationResult<T> = { ok: true; value: T } | { ok: false; errors: string[] };

// ajv-formats is CommonJS; normalise the default export across bundlers.
const addFormats = ((addFormatsModule as unknown as { default?: typeof addFormatsModule })
  .default ?? addFormatsModule) as typeof addFormatsModule;

const ajv = new Ajv2020({ allErrors: false, strict: true, strictRequired: false });
addFormats(ajv);

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
