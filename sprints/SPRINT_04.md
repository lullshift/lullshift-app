# Sprint 04 - Context Keeper and Next Move

**Goal:** Users return to agent work with their intent, notes and next decisions in front of them.

**Depends on:** Sprint 03.

## Work items

- [x] S04-T01: Consent-gated context bookmark editor (metadata-only by default) and retention controls
- [x] S04-T02: Return card on attention/completion: original intent, notes, open decisions, review checklist
- [x] S04-T03: Next Move queue for follow-up questions, acceptance criteria and test ideas with copy-to-clipboard (user-initiated)
- [x] S04-T04: JSON/Markdown export, per-session deletion, session linking
- [x] S04-T05: Tests proving no task content is captured without consent and nothing leaves the device
- [x] S04-T06 (Sprint 01 review follow-up): retention limit for the agent event log in `local-store` (it grows without bound today), with a migration test
- [x] S04-T07 (product owner, 2026-11-05): session detail panel. Clicking a session card or History row opens a side panel for that session, with the card focusable and opened by Enter, the panel closed by Esc and focus returned to the card. Content by state: Needs you shows what is being asked (if the adapter reports it) and the return card; Working shows notes and Next Move items for the session; History shows a timeline of the session's events, export and delete. No "go to the agent window" action (PRD 4.1: don't claim unsupported control)

## Acceptance criteria

- [x] S04-A01: Return card works with a synthetic multi-session fixture
- [x] S04-A02: Export and delete work; retention rules visible (delete and retention verified in the app; export rendering unit-tested, the native save dialog is manual test M04-2)
- [x] S04-A03: Nothing is ever injected into an active agent
- [x] S04-A04: With context off, no content is captured
- [x] S04-A05: Every session card and History row opens its detail panel by mouse and by keyboard; the panel shows only data the user consented to (screen-reader pass is manual test M04-3)

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 04
Completed tasks (commits): uncommitted at closeout; to be committed after approval.
  - T01/T02: Context Keeper with opt-in capture (ADR-013). Off by default, per tool,
    first prompt as intent and the agent's question text only, cleaned and redacted, two
    gates (marker file in the hook, stored setting in the app), stored apart from the event
    log. Return card ("Welcome back") for ended sessions with a Review pill; it never
    claims the task is done.
  - T03: Next Move per session (question, acceptance, test idea, decision), with edit,
    reorder, done and user-initiated copy only.
  - T04: Markdown/JSON export through a native save dialog from Rust (free text fenced),
    per-session delete (one transaction, secure_delete, WAL checkpoint), session links.
  - T05: Rust tests for both capture gates, a raw event-log scan, redaction fixtures and the
    zero-effect hook contract with capture on; tests/context/run-context-checks.mjs.
  - T06: migration 0004 with upgrade and rollback tests; event retention of 7/30/90/365 days
    (default 30), purged at start-up and hourly.
  - T07: detail panel, pinned (default, saved preference) or floating; two columns at
    1,150 px and up, otherwise a grouped list with Needs you fixed on top; History uses
    compact rows when docked in a narrow window; always floating under 720 px.
Commands run and results:
  - pnpm check: all steps passed (secret scan, schemas, types, typecheck, lint, format, TS
    tests 99 + 80, cargo fmt, clippy -D warnings, cargo test 155 passed, desktop build).
  - node tests/context/run-context-checks.mjs (debug build, Windows 11): 14/14. Covers the
    synthetic 4-session fixture, captured intent with a fake key hidden, the captured ask,
    a timeline free of task text, needsReview, capture off storing nothing, Enter opening
    the panel and Esc returning focus, the return card and Done reviewing, and deleting
    the fixture sessions. Settings were restored afterwards.
  - node tests/sandbox/run-denial-tests.mjs: 40/40, 1 known gap (WebRTC), unchanged after
    the new commands.
  - Screenshots checked at 900 px and 1,300 px, pinned and floating, light and dark.
Tests not run and why:
  - Native save dialog for export (M04-2), a real Claude Code session with capture (M04-1)
    and a screen-reader pass (M04-3): manual, logged in docs/MANUAL_TESTS.md.
  - No React component tests (vitest runs in node); keyboard behaviour was checked in the
    running app instead.
Security/privacy review: security-qa-reviewer, adversarial. Nothing critical or high.
  - Fixed: every prompt was stored as "latest request" (now first prompt only, and a new
    prompt clears the old question); redaction gaps (suffixed key names, URL credentials,
    base64 runs, more prefixes); captured intent becoming "yours" when edited (now
    read-only, with a deliberate "Copy into my intent"); deleted text lingering on disk
    (secure_delete and WAL checkpoint); capture setting and marker able to disagree
    (mutex and rollback); export Markdown injection (fenced blocks); consent text now
    lists exactly what is kept, with a test.
  - Accepted and documented in ADR-013: captured text isn't expired by event retention;
    redaction is a safety net, not a guarantee; pre-migration backups may hold old text.
Remaining defects and risks:
  - Redaction is heuristic, both ways: it can miss secrets, and it hides some harmless
    words, such as "token: expired".
  - The export save dialog and the hourly retention thread have not been exercised at
    runtime yet (M04-2).
Acceptance checks passed/failed: A01 passed (synthetic multi-session fixture); A02 passed
  for delete and retention, export pending M04-2; A03 passed (no command or channel writes
  to an agent, Next Move is copy-only, hook zero-effect test with capture on); A04 passed
  (both gates tested in Rust and in the running app); A05 passed for mouse and keyboard,
  screen reader pending M04-3.
Recommended next action: commit Sprint 04, then Sprint 05 (Activity SDK and creator CLI).
```
