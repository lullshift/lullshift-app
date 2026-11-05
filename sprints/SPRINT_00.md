# Sprint 00 - Foundation, Schemas, Local Checks and ADRs

**Goal:** Reproducible monorepo with validated contracts and a local check suite, ready for feature work.

**Depends on:** None. Read `CLAUDE.md`, both docs, ADR-001..003 and `sprints/ROADMAP.md`.

## Work items

- [ ] S00-T01: Create monorepo layout (`apps/desktop`, `crates/`, `packages/`, `tools/lullshift-cli`, `activities/`, `tests/`) with a README per area
- [ ] S00-T02: Pin toolchains (Rust, Node, package manager) and scaffold Tauri 2 + React/TS app and Rust workspace with a mock agent feed
- [ ] S00-T03: Make `reference/` schemas (agent event, activity manifest, marketplace index) the source for generated TS/Rust types; add valid and invalid fixtures
- [ ] S00-T04: Add one cross-platform local check command (build, test, lint, format, typecheck, schema-fixture validation, secret scan) and opt-in pre-commit/pre-push hooks; no hosted CI (ADR-004)
- [ ] S00-T05: Add LICENSE (Apache-2.0), CONTRIBUTING, `.env.example` with placeholders only
- [ ] S00-T06: Write ADR-005+ for Sprint 00 choices (toolchain pinning, SQLite library, type generation from schemas) and a developer bootstrap guide

## Acceptance criteria

- [ ] S00-A01: Fresh clone builds the desktop scaffold and runs the unit test suite on the dev OS
- [ ] S00-A02: Schema fixtures validate; the local check command fails on an invalid fixture of each schema
- [ ] S00-A03: App launches with no sign-in, no network and no credentials

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 00
Completed tasks (commits):
Commands run and results:
Tests not run and why:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next action:
```
