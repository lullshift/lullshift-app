# Sprint 02 - Claude Code Adapter

**Goal:** Real Claude Code sessions are observed through hooks that are installed and removed safely.

**Depends on:** Sprint 01.

## Work items

- [x] S02-T01: Confirm current Claude Code hook payloads and event semantics from official docs; write capability matrix
- [x] S02-T02: Hook forwarder and consent-based, idempotent installer with backups and clean uninstall
- [x] S02-T03: Map hook events to normalized session state, including ambiguous turn-stop vs task completion
- [x] S02-T04: Handle simultaneous sessions and missing/replayed events
- [x] S02-T05: Adapter fixtures plus install/upgrade/uninstall tests
- [x] S02-T06 (Sprint 01 review follow-up): decide on an owner-only DACL for the Windows pipe (ADR-009 residual risk); forwarder reads `endpoint.json` on every send since the pipe name changes per launch, and fails fast when the app isn't running

## Acceptance criteria

- [ ] S02-A01: Real Claude Code smoke test shows working / needs_attention / turn events
  (pending: product owner runs it with the other manual checks, `docs/MANUAL_TESTS.md` M02-1)
- [x] S02-A02: No duplicated attention notifications on replay
- [x] S02-A03: Uninstall restores previous hook configuration exactly
- [x] S02-A04: Capability matrix documented

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 02
Completed tasks (commits): not committed yet (awaiting approval)
  T01 capability matrix and event mapping: docs/adapters/CLAUDE_CODE.md, checked against the
      official hooks reference (exec form, async, Stop per turn, Notification types, SessionEnd).
  T02 forwarder = app executable in `hook` mode (zero-effect contract); consent-based installer
      with backup, atomic writes, exact restore, idempotent repair: crates/agent-adapters,
      desktop Integrations card, migration 0002 (adapter_installations). ADR-010.
  T03 mapping: Stop -> turn_ended (never completed), StopFailure -> failed, SessionEnd ->
      new session_ended/ended state (schema, fixture, both validators, ADR-008).
  T04 sessions keyed by provider session id; duplicate eventIds ignored; provider timestamp
      ordering with receipt cap; hook time taken at process start.
  T05 adapter unit tests (9) and install/upgrade/uninstall tests (15, plus 1 Unix-only).
  T06 default pipe DACL kept for v1 (ADR-009/ADR-010); forwarder reads endpoint.json per send
      and fails fast when the app isn't running (tested).
Commands run and results:
  pnpm check (Windows 11): all 11 steps pass. Rust tests: 69 pass. TS tests: 16 desktop + 80
  protocol pass.
  Debug app + hook mode: exit 0 and empty stdout/stderr for valid, garbage, app-not-running,
  stalled stdin (2.09 s cut-off) and 2 MB input (42 ms, drained). About 70-150 ms per run.
  Simulated Claude Code hook sequences through hook mode into the running app: events stored
  in order, migration to v2 with backup, final states ended / idle, no prompt, command,
  message or path text stored.
Tests not run and why:
  S02-A01 real Claude Code session: `claude -p` from the agent sandbox failed ("OAuth session
  expired"), and needs_attention needs an interactive session. Steps handed to the product
  owner. Not yet seen: real hook payloads, OS notification count, console flash from a debug
  build, behaviour with a missing hook executable.
  Unix-only permission/symlink test: compiled out on Windows; run on macOS/Linux.
  Store-save failure rollback in the desktop command: no injection seam; relies on the tested
  uninstall path.
Security/privacy review: security-qa-reviewer, 1 high, 7 medium, 8 low. Fixed: H1 (repair after
  a user edit made uninstall roll the edit back; regression test), M1 (unique synced temp file,
  permissions kept, symlink/read-only refused, 0600 backups), M2 (rollback if the record
  can't be saved; install/uninstall serialized), M3 (act on the recorded settings path; consent
  panel names CLAUDE_CONFIG_DIR), M4 (missing backup falls back to removal), M5 (time taken at
  process start; one notification per session per 15 s), L1 (BOM kept), L2 (consent text says
  formatting may change), L3 (non-array event is invalid), L5 (panic hook first, args_os,
  non-Unicode paths refused), L7 (oversized stdin drained). Deferred: M6 native consent
  (S03-T07), M7 uninstaller cleanup (S11-T06, warning in consent panel), L4 lost-update window
  (documented in ADR-010), L6 debug console flash (check in A01), L8 (no-record uninstall may
  drop a user's empty event array; minor).
Remaining defects and risks:
  Consent is enforced in the webview until S03-T07. Moving or removing the app without
  Disconnect leaves hooks pointing at a missing executable (S11-T06). Interrupted turns and
  untrusted folders send no events; those sessions go stale rather than ending. Default pipe
  DACL (ADR-009). Event log retention (S04-T06).
Acceptance checks passed/failed:
  A01 pending (product owner's interactive run). A02 passed by tests (state-change rule plus
  15 s per-session gate); toast count to be confirmed in A01. A03 passed (exact byte restore,
  repair and edit cases). A04 passed.
Manual checks owed: docs/MANUAL_TESTS.md M02-1 to M02-5 (run after all sprints, S11-T02).
Recommended next action: start Sprint 03.
```
