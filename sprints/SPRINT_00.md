# Sprint 00 - Foundation, Schemas, Local Checks and ADRs

**Status:** Done on Windows 11 (2026-11-05). macOS and Linux not yet run (see closeout).

**Goal:** Reproducible monorepo with validated contracts and a local check suite, ready for feature work.

**Depends on:** None. Read `CLAUDE.md`, both docs, ADR-001..003 and `sprints/ROADMAP.md`.

## Work items

- [x] S00-T01: Create monorepo layout (`apps/desktop`, `crates/`, `packages/`, `tools/lullshift-cli`, `activities/`, `tests/`) with a README per area
- [x] S00-T02: Pin toolchains (Rust, Node, package manager) and scaffold Tauri 2 + React/TS app and Rust workspace with a mock agent feed
- [x] S00-T03: Make `reference/` schemas (agent event, activity manifest, marketplace index) the source for generated TS/Rust types; add valid and invalid fixtures. *TS types are generated; Rust types are hand-written and held to the same fixtures (ADR-006).*
- [x] S00-T04: Add one cross-platform local check command (build, test, lint, format, typecheck, schema-fixture validation, secret scan) and opt-in pre-commit/pre-push hooks; no hosted CI (ADR-004)
- [x] S00-T05: Add LICENSE (Apache-2.0), CONTRIBUTING, `.env.example` with placeholders only
- [x] S00-T06: Write ADR-005+ for Sprint 00 choices (toolchain pinning, SQLite library, type generation from schemas) and a developer bootstrap guide

## Acceptance criteria

- [x] S00-A01: Fresh clone builds the desktop scaffold and runs the unit test suite on the dev OS
- [x] S00-A02: Schema fixtures validate; the local check command fails on an invalid fixture of each schema
- [x] S00-A03: App launches with no sign-in, no network and no credentials

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 00
Completed tasks (commits): 8c02d7d (Sprint 00 implementation), f221715 (lint fix found by fresh-clone run)

Commands run and results (Windows 11 Enterprise 10.0.26200, x64, Node 24.14.1, pnpm 12.6.0, Rust 1.99.1):
- Fresh `git clone` + `pnpm install --frozen-lockfile` + `pnpm check`: all 11 steps pass
  (secret scan, schema fixtures, generated types, typecheck, lint, format, TS tests,
  cargo fmt, cargo clippy -D warnings, cargo test, desktop build). First fresh run failed lint
  (no-useless-assignment); fixed in f221715 and rerun green.
- Tests: 60 TS (56 schema fixture cases + 4 desktop view tests), 10 Rust (8 agent-core unit
  tests + 2 shared-fixture suites covering all agent-event fixtures).
- A02 negative proof: a planted invalid fixture in each schema's valid/ folder makes
  validation exit 1 (agent-event, activity-manifest, marketplace); removed afterwards.
- Secret scan negative proof: a planted RSA private key fails `pnpm secrets` (exit 1);
  a key staged then cleaned on disk fails the staged scan used by the pre-commit hook (exit 1).
- App smoke test: `tauri build --debug --no-bundle`, launched lullshift-desktop.exe; read the
  rendered page through WebView2 DevTools: title "Lullshift", 5 mock sessions shown with
  "Needs you" first and "Unknown / no events" for the unreachable one. No sign-in, no
  credentials. With the `allow-mock-sessions` permission removed, the command is denied and the
  UI shows "Agent status is unknown" (ACL enforced).

Tests not run and why:
- macOS and Linux: no machines available in this session. Their checks and app smoke tests
  are still owed before any release gate.
- "No network" for A03 was verified by configuration (CSP connect-src allows only Tauri IPC;
  no HTTP code exists), not by running with the network adapter disabled.
- No end-to-end UI automation yet (Playwright + Tauri arrives with Sprint 01 UI work).

Security/privacy review: security-qa-reviewer agent, read-only.
- Fixed now: event parser errors no longer echo input (test proves it, M1); TS and Rust
  validators agree on UUID and timestamp forms, with 8 new invalid fixtures (M2); app
  permissions reduced from core:default to the single app command, gated by Tauri's ACL (M3);
  CSP form-action 'none' and freezePrototype (L1); pre-commit scans staged content (L2);
  .gitignore covers keystores, signing keys, .npmrc and local databases (L3); runtime
  validators stop at the first error (L5).
- Tracked: S01-T06 (event payload limits, metadata shape, control/bidi characters),
  S03-T06 (activity origin and frame-src), S05-T06 (manifest string patterns, shared Ajv
  factory), S08-T06 (marketplace git URL/ref/path hardening).

Remaining defects and risks:
- App icons are Tauri template placeholders.
- Bundle identifier "app.lullshift.desktop" depends on the unconfirmed product name.
- Hooks are opt-in and skippable; with no CI, regressions are caught only when someone runs
  `pnpm check`.
- TypeScript pinned to 6.0.x until typescript-eslint supports TypeScript 7.

Acceptance checks passed/failed: A01 pass (Windows), A02 pass, A03 pass (network by config).

Recommended next action: Sprint 01 (desktop shell, reducer, SQLite, local IPC), starting
with S01-T06 hardening alongside the IPC work. Run `pnpm check` on macOS and Linux when available.
```
