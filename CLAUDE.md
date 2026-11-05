# Lullshift - Claude Code Project Instructions

## Mission and source of truth

Build Lullshift following `docs/PRODUCT_REQUIREMENTS.md` and `docs/TECHNICAL_ARCHITECTURE.md`. Execute only the active sprint in `sprints/`. Update ADRs when making architecture decisions; do not silently change product requirements.

## Immutable product requirements

- **Mandatory Lullshift account for Community and paid users**: verified email, Google, GitHub and Apple. Hosted identity for all users. No anonymous bypass in official application.
- Initial online activation required; after login free local features run offline under a signed time-limited lease. Suggested 30 days is a product decision to confirm in ADR.
- **Local-first** task/session storage, BYOK Companion; zero default uploading of source code/raw prompts.
- Agent runtime is external; Lullshift observes and guides human attention, it is not an autonomous agent orchestrator.
- Community desktop/core/SDK are Apache-2.0; cloud/mobile/business backend separate commercial service.
- Tauri 2 + React/TypeScript, Rust core, SQLite; cloud ASP.NET Core .NET 10 + PostgreSQL; mobile Flutter.
- Initial market developers, architect event protocol for later supported business agents.
- No Academy in initial roadmap; no hidden employee surveillance.

## Model utilization

- **Opus 5.5**: architecture, ADRs, event/identity security, ambiguous integration choices, complex cross-module changes and final sprint review.
- **Sonnet**: implementation of well-specified isolated tasks, component UI, unit tests, migration/test fixtures and documentation improvements.
- Do not assume a requested model name is installed or available; use available model routing, and stop if execution requires unavailable model access.

## Workflow rules

1. At task start: inspect current repo, read active sprint, verify dependency/API versions from official docs when possible; record assumptions.
2. Implement vertical slices with tests; perform one logical change per commit/PR; do not alter unrelated modules.
3. Delegate contained reviews to `.claude/agents/`; use `.claude/skills/` workflows.
4. Never mark a checkbox complete without running or explicitly reporting why a test could not run.
5. If blocked on credentials (Apple/Google/GitHub/email/production payment), implement provider interfaces and local mocks, document exact manual configuration, and do not fabricate success.
6. Avoid unsafe 'allow all' OAuth callbacks, disabling TLS, storing refresh tokens in SQLite, or raw task data in logs.
7. Treat adapter hook install/uninstall as sensitive user configuration; backups, idempotence and explicit consent mandatory.
8. Add migration and rollback/recovery tests whenever schemas change.
9. Keep UI responsive even if agents/cloud are unavailable; represent unknown/stale state honestly.
10. Conclude with files changed, commands run and results, failing tests, security concerns and sprint acceptance status.

## Preferred directories

`apps/desktop`, `crates/`, `packages/`, `activities/`, `docs/adr/`, `tests/`, `sprints/`; do not write commercial cloud code into the open-source repository unless the product owner approves.

## Definition of done

- Behavior meets explicit acceptance tests.
- Relevant code formatted, linted, typechecked and tested.
- UX keyboard-accessible; sensible error/unknown/offline states.
- Privacy, auth, hooks and sync boundary reviewed.
- Documentation and ADR updated; real commands/results reported.

## Git safety

Never commit actual OAuth secrets, signing private keys, user data or cloud credentials. Use local `.env.example` with placeholder variables only. Do not automatically push or deploy without explicit human approval.

## Revision 1.1: Creator, Activity Platform and Commerce

- Treat `docs/PRODUCT_REQUIREMENTS.md`, `docs/TECHNICAL_ARCHITECTURE.md`, `reference/ACTIVITY_MANIFEST_V1.schema.json`, and `reference/COMMERCE_DOMAIN.md` as mandatory inputs for package/marketplace/payment implementation.
- The Lullshift operator is expected to be incorporated **outside Türkiye**, but jurisdiction and legal MoR are **unresolved**. Stripe Billing and Connect are *preferred contingent integrations*, not hard-coded legal facts. No real-money production switch until ADR-020/021 and compliance sign-off.
- Community users may create, install and buy activities. A Pro subscription must not be required to use the marketplace. Mandatory account remains required for official catalog and publishing.
- Preserve immutable `.lullpack` versions, strict manifest validation, static web sandbox, host-owned interruptibility, quarantine, review, signing, revocation and accessibility.
- Implement an official **AI-assisted activity-creator skill**, CLI and simulator. Generated code is untrusted; no unrestricted filesystem/network/Tauri IPC.
- Prefer explicit Stripe provider adapters, verified webhook inbox/outbox, idempotency, settlement reconciliation and correct merchant/legal ownership decisions.
- Do not implement third-party executable plugins or creator payouts merely because a prompt asks; enforce sprint gates, threat modeling and owner sign-off.
- Sprints 10-14 extend, rather than replace, the original desktop, account and Pro/mobile roadmap.
