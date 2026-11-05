# Sprint 06 - Game Kits and First Games

**Goal:** Shared kits make games cheap to build well; the first games ship on them.

**Depends on:** Sprint 05.

## Work items

- [ ] S06-T01: Grid kit: cells, selection, keyboard navigation, undo, seeded generators, daily seed
- [ ] S06-T02: Card kit: deck, moves, keyboard play, undo
- [ ] S06-T03: Arcade kit: game loop, forced pause, resume countdown, reduced-motion mode
- [ ] S06-T04: At least 8 original games across the kits, puzzle/turn-based first
- [ ] S06-T05: Kit test suites (lifecycle, persistence, accessibility) and per-game rule tests

## Acceptance criteria

- [ ] S06-A01: Each kit passes the activity gates in `DEFINITION_OF_DONE.md`
- [ ] S06-A02: At least 8 games pass validate, simulate, persistence and keyboard checks
- [ ] S06-A03: Games listed in the built-in marketplace index

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 06
Completed tasks (commits):
Commands run and results:
Tests not run and why:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next action:
```
