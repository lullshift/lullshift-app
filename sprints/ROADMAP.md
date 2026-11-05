# Lullshift - Sprint Roadmap

**Cadence:** 2-week indicative sprints; adjust to team capacity. **Ordering is by dependencies and risk**, not promised calendar dates. Opus handles architecture/security; Sonnet handles bounded implementation and tests. Each sprint has explicit exit tests.

| Sprint | Scope | Key deliverable |
|---|---|---|
| 00 | Baseline, ADRs, repo and CI | Reproducible builds and contracts |
| 01 | Identity, account, device and offline lease | Required login with provider mocks |
| 02 | Tauri shell, local SQLite, IPC, tray | Desktop runs and persists state |
| 03 | Claude Code integration and state machine | Real hook observations and safe uninstall |
| 04 | Notifications, Break, Arcade | Interruptible activity experience |
| 05 | Context Keeper and Next Move | Re-entry card and local decisions |
| 06 | Companion BYOK and activity SDK | Privacy-scoped conversations and contract |
| 07 | Codex/Cursor adapters, security and release | Community v1 release candidate |
| 08 | Commercial sync, plans, hosted services | Pro backend pilot |
| 09 | Flutter mobile and business administration | Mobile/Business pilot |

## Sprint-wide entry conditions

- Previous sprint gate met; ADRs resolved or explicitly provisional.
- Dependencies and test fixture versions captured.
- Any provider credentials required are handled through secure test accounts; never in Git.

## Sprint-wide exit conditions

- Completed work demonstrable in running code or validated tests.
- Rust/TS/.NET/Flutter checks applicable to scope succeed and evidence recorded.
- Security/privacy/UX regressions reviewed.
- README, runbooks and environment examples updated.
- Actual blocking issues disclosed, not silently marked done.

## Boundary reminders

**Every edition requires an account**. Only free local features run offline after activation; identity and device registration are cloud hosted for all. No Academy. No unapproved agent actions, cloud upload of task content, or employee surveillance.

## Extension Roadmap 1.1 (Creator and Marketplace)

| Sprint | Workstream | Release gate |
|---|---|---|
| 10 | `.lullpack` schema, sandbox, local Creator SDK/CLI/simulator | Adversarial sandbox and deterministic pack tests |
| 11 | Free catalog, publishing intake, scanning, moderation, signed CDN distribution | Only approved and valid packages install |
| 12 | Stripe Billing, subscription entitlements, commerce groundwork | Idempotent checkout and verified event tests |
| 13 | Stripe Connect, creator KYC/KYB workflow, one-time paid listings, payout reconciliation | Legal/MoR approval plus full commerce E2E |
| 14 | AI creator experiences, third-party content curation, marketplace hardening | Creator experience validated and publish/rollback tested |

**Sprint order:** Sprint 10 depends on 04/06; Sprint 11 depends on 10 and hosted identity; Sprint 12 depends on account/Pro groundwork (01/08); Sprint 13 depends on 11/12; Sprint 14 follows public free-catalog validation. Teams can parallelize only when explicit contracts are stable. Payments remain feature-flagged until regulatory/business decisions are approved.
