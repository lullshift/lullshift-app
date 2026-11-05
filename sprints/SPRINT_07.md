# Sprint 07 - Engines and First Content

**Goal:** Data-driven engines let most activities ship as code-free content packs.

**Depends on:** Sprint 05.

## Work items

- [x] S07-T01: Engines with public versioned data schemas: breathing, movement routine, guided pause, quiz, brain teaser, reflection, creative toy, typing drills, away prompts
- [x] S07-T02: Engine data-schema compatibility tests (old packs keep working within a major version)
- [x] S07-T03: At least 40 content packs across engines, original text and assets with licenses
- [x] S07-T04: Content style guide (tone, no medical claims, play not curriculum)
- [x] S07-T05: Accessibility review of every engine

## Acceptance criteria

- [x] S07-A01: Each engine passes the activity gates
- [x] S07-A02: At least 40 content packs validate against their engine schemas
- [x] S07-A03: Breaking a pack's data fails validation with a clear message

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 07
Completed tasks (commits): local commits "Check engine pack data in the app...", "Add nine
  engines and 50 content packs..." and this closeout. Local only, not pushed.
Commands run and results:
  pnpm check (full, Windows 11, Node 24, Edge headless): all 14 steps pass after formatting the
    new parity corpus (first run: one simulate flake, see risks; second run: format only).
  TS tests: agent-protocol 101, activity-sdk 18, kits 82, desktop 670 (26 files, incl. 236
    engine tests), CLI 43. cargo test: all pass (desktop crate 71).
  check-engine-compat: 10 engines, 115 fixtures, 50 packs, 45 Ajv/Rust parity cases agree; a
    valid file planted in an invalid/ folder fails the check as expected.
  check-bundled: all 70 activities validate and simulate; index lists all of them.
  Broken pack copy (unknown field, 5000-char prompt, number for text) fails validate with
    "surprise isn't a known field", "questions[0].prompt is longer than 200 characters",
    "questions[1].prompt should be text" (A03).
  Debug app over CDP: six packs (breathing, teaser, reflection, typing, quiz, toy) open from
    their cards with their own content and no error card; engines are not listed as cards.
Tests not run and why:
  Screen reader, keyboard play-through and 200% zoom of every engine: needs a person (M07-1).
  Timed packs watched in real time (M07-2); human read of the content (M07-3); error card
  seen by a person (M07-4); macOS and Linux (M07-5).
Security/privacy review: security-qa-reviewer on data_schema.rs, the pack runtime, content.ts
  and friendly.ts: no critical or high. Fixed M1 (regex . and \s differ from JS), M2 (no
  validation work budget; O(n^2) uniqueItems), L3 (multipleOf tolerance), L4 (narrow parity
  test), L5 (raw text in compat output), each with a regression test. L6 (check cache must be
  keyed by commit/hash once installs exist) and U+2028/2029 in the plain-text pattern are
  logged as decision 14 for Sprint 08.
  Accessibility review (desktop-ux-engineer, static): three must-fix and seven minor findings,
  all fixed (teaser announcements, 3:1 field borders, typing target as text, card text
  announced, aria-pressed removed from label-changing toggles, wrapping controls, focus kept
  when buttons disable, toy canvas semantics, quiz digit keys scoped).
  Content review: 17 issues fixed (wrong answers, stock riddles, a health claim, wording).
Remaining defects and risks:
  Simulate occasionally failed to start Edge under full-gate load; the start timeout is now
    30 s instead of 10 s. Watch for recurrence.
  The 500k-step validation budget was not tested against a pack at the 256 KiB data limit.
  The app refuses fractional multipleOf that Ajv accepts (fails closed).
  Accessibility fixes not yet confirmed with a screen reader.
  Product choices made while the owner was away: sprints/DECISIONS_FOR_REVIEW.md (items 9-14).
Acceptance checks passed/failed: A01 every engine passes validate, simulate and its unit tests;
  A02 50 packs validate against their engine schemas, in the CLI (Ajv) and in the app (Rust);
  A03 broken pack data fails with clear, field-level messages. Manual checks pending.
Recommended next action: product-owner review of DECISIONS_FOR_REVIEW.md and a read of the
  content; then Sprint 08 (Git marketplaces).
```
