# ADR-006: Schema-first contracts and type generation

- **Status:** Accepted (Sprint 00)
- **Date:** 2026-11-05
- **Owner:** Implementation lead

## Context

Three contracts cross trust or language boundaries: the agent event (hooks → Rust core), the activity manifest, and the marketplace index (untrusted repos → Rust core and the CLI). TypeScript and Rust must agree on them, and invalid input must be rejected identically.

## Decision

1. **The JSON Schemas in `reference/` are canonical** (draft 2020-12). Schema `$id`s use the `urn:lullshift:schema:<name>:v<major>` form so that no domain is implied while the product name is unconfirmed.
2. **TypeScript:** `packages/agent-protocol` holds types generated from the schemas with `json-schema-to-typescript` into `src/generated/` (committed). `pnpm check` fails if regenerating changes them. Runtime validation uses Ajv (draft 2020-12, formats enabled), with the schemas compiled from the same files. Ajv runs in strict mode except `strictRequired`, which wrongly flags `required` inside `if/then` branches. Fields that a branch forbids use `"field": false` under `properties`.
3. **Rust:** types are hand-written `serde` structs (`deny_unknown_fields`) in the crates that own each contract (`crates/agent-core` for events in Sprint 00). Generators do not yet handle the manifest's `if/then` rules well. Drift is caught by **shared fixtures**: every fixture in `tests/fixtures/<schema>/valid` must deserialize and every one in `invalid` must be rejected, in both TS (Ajv) and Rust tests. When a rule cannot be expressed in the serde types alone, the Rust side validates it explicitly.
4. **Fixtures:** each schema has `valid/` and `invalid/` folders. Every invalid fixture breaks exactly one rule, and its file name says which (for example `missing-event-id.json`). The check command proves each invalid fixture fails.
5. **Schema changes** require updating fixtures in the same commit, and a new major version (`v2`, new `$id`) for breaking changes.

## Consequences

- One source of truth with two independent validators, cross-checked by the same fixtures.
- Rust-side validation for untrusted marketplace content (Sprint 08) may later add a JSON Schema validator crate; that decision is left to the marketplace ADR.
