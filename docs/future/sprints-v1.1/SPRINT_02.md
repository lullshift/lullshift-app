# Sprint 02 - Desktop Runtime and Local Persistence

**Goal:** Desktop Runtime and Local Persistence. **Duration:** indicative two weeks. **Owner:** implementation lead, with relevant Claude Code specialist agents.

## Dependencies and scope

Read `CLAUDE.md`, product and technical docs, and `sprints/ROADMAP.md`. Complete earlier sprint acceptance criteria. Do not implement later sprints or substitute unsupported provider behavior.

## Work items

- [ ] S02-T01: Implement Tauri tray lifecycle, hide-to-tray and notification permission UX
- [ ] S02-T02: Implement Rust event reducer with persisted sessions and migration-managed SQLite
- [ ] S02-T03: Create authenticated UDS / Windows named pipe interface and hook ingestion
- [ ] S02-T04: Build React dashboard with Zustand view state and offline/account status
- [ ] S02-T05: Add restart, duplicate/reorder and stale-state tests

## Acceptance criteria / demonstrable exit tests

- [ ] S02-A01: Window closing does not terminate monitoring runtime
- [ ] S02-A02: Session state survives restart and invalid input is rejected
- [ ] S02-A03: Unknown and stale agent status shown truthfully
- [ ] S02-A04: Tray and SQLite local behavior tested on at least primary dev OS

## Engineering guardrails

- Account is mandatory for both Community and commercial clients; every provider is covered by interface and tests.
- No secrets in git, no covert source/prompt upload, no unapproved hook/agent changes.
- Maintain Windows/macOS/Linux compatibility in native code where applicable.
- Always represent unknown agent status accurately, and retain local user-owned data.

## Delegation guidance

- Use Opus 5.5 for architecture, identity, cryptography, cross-module risks, and acceptance review.
- Use Sonnet for scoped code generation, fixtures, UX components and tests.
- Delegate reviews to `.claude/agents/architecture-reviewer.md`, `.claude/agents/security-qa-reviewer.md` or other relevant agents.

## Sprint closeout template

```text
Sprint: 02
Completed tasks (links/commits):
Commands run and observed results:
Tests not run and reason:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next sprint action:
```
