# Lullshift - Claude Code Project Instructions

## Mission and source of truth

Build Lullshift following `docs/PRODUCT_REQUIREMENTS.md`, `docs/TECHNICAL_ARCHITECTURE.md` and the accepted ADRs in `docs/adr/`. Execute only the active sprint in `sprints/`. Record architecture decisions as ADRs; do not silently change product requirements.

`docs/future/` holds deferred v1.1 material (accounts, cloud, mobile, commerce). It is **not** a source of requirements. Do not implement, stub or design for anything in it.

## Product requirements (v2.0)

- **Fully open source v1** (Apache-2.0): desktop app, Rust core, protocol, SDK, CLI, kits, engines and all bundled activities. (ADR-001)
- **No accounts of any kind.** No sign-in, registration, device enrollment or offline lease. Fully functional on first launch and fully offline.
- **No Lullshift-hosted services and no default telemetry.** Network use is only user-initiated: Git fetches of added marketplaces, app update checks, BYOK Companion calls.
- **Local-first:** task/session data stays local; zero default uploading of source code or prompts.
- Agent runtime is external; Lullshift observes and guides human attention, it is not an agent orchestrator.
- **Marketplaces are Git repos** (public or private) with `.lullshift/marketplace.json`; installs are pinned to commit SHAs; updates are explicit; this repo is the built-in marketplace. Lullshift never stores Git credentials. (ADR-002)
- **Activity model:** custom code activities, code activities on optional kits, and code-free content packs for engines. Context Keeper, Next Move and Companion are host features, not sandboxed activities. (ADR-003)
- **v1 content target:** 20 original games and ~100 activities, bundled under `activities/`. A target, not a release blocker. No recreations of existing branded games.
- Stack: Tauri 2 + React/TypeScript/Vite, Rust core, SQLite. No backend, no mobile in v1.
- Initial market developers; keep the agent event protocol domain-neutral.
- No Academy/curriculum; no employee surveillance.

## Model utilization

- **Opus 5.5**: architecture, ADRs, sandbox/IPC/marketplace security, ambiguous integration choices, complex cross-module changes and final sprint review.
- **Sonnet**: well-specified isolated tasks, UI components, games and engines on established kits, content packs, unit tests, fixtures and documentation.
- Do not assume a requested model is available; use available routing and stop if execution requires unavailable model access.

## Workflow rules

1. At task start: inspect the repo, read the active sprint, verify dependency/API versions from official docs when possible; record assumptions.
2. Implement vertical slices with tests; one logical change per commit/PR; do not alter unrelated modules.
3. Delegate contained work and reviews to `.claude/agents/`; use `.claude/skills/` workflows.
4. Never mark a checkbox complete without running the check, or explicitly reporting why it could not run.
5. Treat adapter hook install/uninstall as sensitive user configuration: explicit consent, backups, idempotence, clean uninstall.
6. Treat every marketplace repo and every activity as untrusted input. Never weaken the sandbox or Git hardening to make content work.
7. Never store secrets (BYOK keys) in SQLite, logs or UI traces; OS keychain only. Never log raw task data.
8. Add migration and rollback/recovery tests whenever persistent schemas change.
9. Keep UI responsive when agents or the network are unavailable; represent unknown/stale state honestly.
10. Conclude with files changed, commands run and results, failing tests, security concerns and sprint acceptance status.

## Directories

`apps/desktop`, `crates/`, `packages/` (protocol, activity-sdk, activity-kits), `tools/lullshift-cli`, `activities/{games,engines,content}`, `.lullshift/marketplace.json`, `reference/`, `docs/adr/`, `tests/`, `sprints/`.

## Activities and creators

- Read `reference/ACTIVITY_MANIFEST_V1.schema.json` and `reference/MARKETPLACE_V1.schema.json` before touching packages or marketplaces.
- Every activity implements host-owned interruptibility (pause/resume/saveState/restoreState), keyboard access and reduced motion, works offline, and declares only supported permissions.
- Generated (including AI-generated) activity code is untrusted: static web only, no network, filesystem, clipboard or Tauri IPC.
- Games use original names and art; generic mechanics are fine, copies of specific branded games are not.
- Use `/activity-creator` for new activities and `/marketplace-publishing` for marketplace repos.

## Definition of done

See `sprints/DEFINITION_OF_DONE.md`. In short: acceptance tests demonstrated; code formatted, linted, typechecked and tested; keyboard-accessible with honest error/unknown/offline states; privacy, hook and sandbox boundaries reviewed; docs and ADRs updated; real commands and results reported.

## Git safety

Never commit secrets, signing private keys or user data. Use `.env.example` with placeholders only. Do not push, tag, release or publish without explicit human approval.
