# Lullshift - Sprint Roadmap (v2.0)

**Cadence:** indicative 2-week sprints; ordering is by dependency and risk, not promised dates. Opus handles architecture/security and sprint review; Sonnet handles bounded implementation, games, engines, content and tests. Governing decisions: ADR-001 (open source, no accounts), ADR-002 (Git marketplaces), ADR-003 (activity model and 20/100 target), ADR-004 (local checks, no hosted CI for now).

| Sprint | Scope | Key deliverable |
|---|---|---|
| 00 | Foundation: monorepo, toolchains, schemas, local checks, ADRs | Reproducible build and validated contracts |
| 01 | Desktop shell: tray, Rust reducer, SQLite, local IPC, dashboard | App runs, persists and survives restart |
| 02 | Claude Code adapter | Real hook observations with safe install/uninstall |
| 02b | Design system and UX pass (product owner request) | Calm, soft visual design; redesigned dashboard, integrations and settings |
| 03 | Notifications and activity host | Sandboxed, host-interruptible activities; Break; one reference game |
| 04 | Context Keeper and Next Move | Return card and local decisions |
| 05 | Activity SDK and creator CLI | SDK, manifest kinds, simulator, `create/dev/simulate/validate/pack` |
| 06 | Game kits and first games | Grid, card, arcade kits; at least 8 games |
| 07 | Engines and first content | ~9 engines with data schemas; at least 40 content packs |
| 08 | Git marketplaces | Add/install/update/uninstall from public and private repos, commit pinning, built-in marketplace |
| 09 | Companion (BYOK) | Privacy-scoped conversations, keys in OS keychain |
| 10 | More adapters and content completion | Codex/Cursor adapters; content to 20 games / ~100 activities |
| 11 | v1.0 release | Installers, signed updater, cross-platform QA, audits, docs |
| 12 | Owner review changes | Decisions reviewed 2026-11-06: OpenCode adapter, sandbox, marketplace, Companion, game and content changes; the release follows |
| 13 | Fresh content from your own model (planned, ADR-028 accepted) | Opt-in BYOK sets for quiz, teaser, calm cards, reflection and typing, plus movement, breathing and pause behind a second opt-in; labelled AI-generated, checked like packs; `roundSize` draws |

## Tracks and parallelism

- **Platform track** (00–05, 08) is sequential; each sprint depends on the previous gate.
- **Content track** (06, 07, 10) can run in parallel once Sprint 05's SDK contract and the kit/engine APIs are stable. Content is a release target, not a release blocker (ADR-003).
- **Companion** (09) depends only on 01 and 04 and can move earlier if capacity allows.
- Third-party **code** activities from marketplaces stay disabled until the Sprint 03 sandbox proof passes on all three OSes; third-party **content packs** can be enabled in Sprint 08 regardless.

## Sprint-wide entry conditions

- Previous sprint gate met; ADRs resolved or explicitly provisional.
- Dependency and fixture versions captured.

## Sprint-wide exit conditions

- Work demonstrable in running code or executed tests, with evidence recorded.
- Applicable Rust/TypeScript checks pass.
- Security, privacy, accessibility and interruption regressions reviewed.
- README, ADRs and developer docs updated.
- Blocking issues disclosed, never silently marked done.

## Boundary reminders

No accounts, no hosted services, no default telemetry, no stubs for `docs/future/` items. No Academy. No unapproved agent actions or uploads of task content. No recreations of branded games.
