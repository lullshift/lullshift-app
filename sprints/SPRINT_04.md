# Sprint 04 - Context Keeper and Next Move

**Goal:** Users return to agent work with their intent, notes and next decisions in front of them.

**Depends on:** Sprint 03.

## Work items

- [ ] S04-T01: Consent-gated context bookmark editor (metadata-only by default) and retention controls
- [ ] S04-T02: Return card on attention/completion: original intent, notes, open decisions, review checklist
- [ ] S04-T03: Next Move queue for follow-up questions, acceptance criteria and test ideas with copy-to-clipboard (user-initiated)
- [ ] S04-T04: JSON/Markdown export, per-session deletion, session linking
- [ ] S04-T05: Tests proving no task content is captured without consent and nothing leaves the device
- [ ] S04-T06 (Sprint 01 review follow-up): retention limit for the agent event log in `local-store` (it grows without bound today), with a migration test

## Acceptance criteria

- [ ] S04-A01: Return card works with a synthetic multi-session fixture
- [ ] S04-A02: Export and delete work; retention rules visible
- [ ] S04-A03: Nothing is ever injected into an active agent
- [ ] S04-A04: With context off, no content is captured

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 04
Completed tasks (commits):
Commands run and results:
Tests not run and why:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next action:
```
