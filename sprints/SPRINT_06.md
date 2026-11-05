# Sprint 06 - Game Kits and First Games

**Goal:** Shared kits make games cheap to build well; the first games ship on them.

**Depends on:** Sprint 05.

## Work items

- [x] S06-T01: Grid kit: cells, selection, keyboard navigation, undo, seeded generators, daily seed
- [x] S06-T02: Card kit: deck, moves, keyboard play, undo
- [x] S06-T03: Arcade kit: game loop, forced pause, resume countdown, reduced-motion mode
- [x] S06-T04: At least 8 original games across the kits, puzzle/turn-based first
- [x] S06-T05: Kit test suites (lifecycle, persistence, accessibility) and per-game rule tests

## Acceptance criteria

- [x] S06-A01: Each kit passes the activity gates in `DEFINITION_OF_DONE.md`
- [x] S06-A02: At least 8 games pass validate, simulate, persistence and keyboard checks
- [x] S06-A03: Games listed in the built-in marketplace index

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 06
Completed tasks (commits): T01-T03 in "Add grid, card and arcade activity kits..." (54c108e);
  T04-T05, kit review fixes and closeout in the following commits. Local only, not pushed.
Commands run and results:
  pnpm check (full, Windows 11, Node 24, Edge headless): all 13 steps pass, including
    "activity kits up to date" and "bundled activities (validate, simulate)" for 12 activities.
  Kit tests 82, CLI tests 41, game rule tests 94 (8 files), desktop activity tests 151: pass.
  check-bundled: the marketplace index lists all 12 bundled activities by manifest id; a
    tampered index (missing and extra entries) fails as expected.
  Debug app driven over CDP: all 8 games open from their cards and render; Firefly Catch's
    timer runs after the countdown; Fog Garden and Moss Paths overflowed a 900 px window and
    now cap the board by window height.
Tests not run and why:
  Full keyboard play-through and screen reader pass of each game: needs a person (M06-1).
  Real-time pause/countdown/reduced motion watched by a person (M06-2).
  macOS and Linux (M06-3).
Security/privacy review: security-qa-reviewer on the kits, no critical or high. Fixed all 9
  findings plus a globalThis fallback, each with a regression test: import parsing in the kit
  generator, bounded restore of hostile saved state, NaN options, loop recovery after a throw,
  visibility listener timing, player vs host pause, focus fallbacks, export let, CLI create
  writing nothing when a kit file is missing. Games scanned: no HTML injection, eval, network
  or storage APIs; previews are small static SVGs.
Remaining defects and risks:
  The card kit's Table/bindTable is not used by a shipped game yet (decision 8).
  DOM keyboard behaviour of the kits is covered by the simulator and a fake DOM, not a real
    DOM test library.
  Node's stripTypeScriptTypes is experimental; the kits drift check depends on it.
  Product choices made while the owner was away: sprints/DECISIONS_FOR_REVIEW.md (items 4-8).
Acceptance checks passed/failed: A01 kits pass the activity gates through the games that use
  them (validate, simulate, persistence, keyboard focus); A02 8 games pass validate, simulate
  and rule tests; A03 games listed in .lullshift/marketplace.json. Manual checks pending.
Recommended next action: product-owner review of DECISIONS_FOR_REVIEW.md and the games; then
  Sprint 07 (engines and content).
```
