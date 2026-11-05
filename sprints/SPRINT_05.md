# Sprint 05 - Activity SDK and Creator CLI

**Goal:** Creators can build, simulate, validate and pack activities locally with the published SDK.

**Depends on:** Sprint 03.

## Work items

- [ ] S05-T01: Versioned TypeScript `activity-sdk` (lifecycle, storage, reduced-motion and theme signals)
- [ ] S05-T02: Manifest v1 with `kind: app | content`; engine reference and data-schema validation for content packs
- [ ] S05-T03: CLI `create`, `dev`, `simulate`, `validate`, `pack` (deterministic `.lullpack` for sideloading); `marketplace init` scaffolds an index
- [ ] S05-T04: Event-sequence simulator (needs_attention, duplicates, out-of-order, forced pause, close, crash, resume, offline)
- [ ] S05-T05: Update `/activity-creator` skill and `creator/` prompts; starter templates for custom, kit-based and content activities
- [ ] S05-T06: Tighten manifest strings (Sprint 00 security review): segment-based `data` path pattern (no `.`, `..`, empty or `.git` segments, no leading `-`), `maxLength`; patterns and `maxLength` for `minHostVersion`/`sdkVersion`; reject bidi/control characters in `name`/`description`/`author`. Share one Ajv factory between the runtime validator and the fixture script

## Acceptance criteria

- [ ] S05-A01: Sample code activity and sample content pack pass `validate` and `simulate`
- [ ] S05-A02: Invalid manifests, unknown permissions and code inside content packs are rejected
- [ ] S05-A03: Identical input produces an identical `.lullpack` hash
- [ ] S05-A04: An activity generated from the creator prompt passes without host changes

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 05
Completed tasks (commits):
Commands run and results:
Tests not run and why:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next action:
```
