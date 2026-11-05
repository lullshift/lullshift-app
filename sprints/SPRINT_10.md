# Sprint 10 - More Adapters and Content Completion

**Goal:** Broader agent support and the full v1 content set.

**Depends on:** Sprints 02, 06, 07, 08.

## Work items

- [ ] S10-T01: Research and implement Codex and Cursor adapters only where documented lifecycle APIs exist; capability matrix
- [ ] S10-T02: Games to 20 and activities to ~100 (release target, not blocker)
- [ ] S10-T03: Daily seeded puzzles for grid games
- [ ] S10-T04: Content review pass: originality, tone, accessibility, licenses

## Acceptance criteria

- [ ] S10-A01: Adapter capability gaps documented; no unsupported claims
- [ ] S10-A02: All bundled items pass activity gates
- [ ] S10-A03: Content count reported honestly against the 20/100 target

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 10
Completed tasks (commits):
Commands run and results:
Tests not run and why:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next action:
```
