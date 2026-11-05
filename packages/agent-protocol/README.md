# @lullshift/agent-protocol

TypeScript types and runtime validators for Lullshift's contracts. The JSON Schemas in [`/reference`](../../reference) are canonical (ADR-006).

| Schema | Type | Validator |
|---|---|---|
| `AGENT_EVENT_V1.schema.json` | `AgentEventV1` | `validateAgentEvent` |
| `ACTIVITY_MANIFEST_V1.schema.json` | `ActivityManifestV1` | `validateActivityManifest` |
| `MARKETPLACE_V1.schema.json` | `MarketplaceIndexV1` | `validateMarketplaceIndex` |

- `pnpm gen:types` regenerates `src/generated/` after a schema change. `pnpm check` fails if they are stale.
- `pnpm validate:schemas` checks every file in `tests/fixtures/<schema>/{valid,invalid}`.
- The Rust side (`crates/agent-core`) reads the same fixtures in its own tests.
