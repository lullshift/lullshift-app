# Sprint 01 - Desktop Shell, State and Local IPC

**Status:** Implemented and verified on Windows 11 (2026-11-05). One manual tray check is open (S01-A04), and macOS/Linux have only been compiled (see closeout).

**Goal:** The app runs from the tray, persists agent sessions locally and accepts events over authenticated local IPC.

**Depends on:** Sprint 00.

## Work items

- [x] S01-T01: Tauri tray lifecycle, hide-to-tray, autostart option and notification permission UX
- [x] S01-T02: Rust event reducer and session state machine with migration-managed SQLite (WAL)
- [x] S01-T03: Authenticated Unix domain socket / Windows named pipe for hook ingestion
- [x] S01-T04: React dashboard with Zustand view state showing sessions, unknown/stale states and empty state
- [x] S01-T05: Restart, duplicate, reorder and stale-state tests; migration rollback test
- [x] S01-T06: Harden agent event input (Sprint 00 security review): cap total payload bytes at the IPC transport before parsing; restrict `metadata` to scalar values with value `maxLength` and a `propertyNames` pattern; give `capabilities` items a `maxLength`/pattern; decide `providerSessionId` `minLength: 1`; reject or neutralise control and bidi characters in displayed strings. Update schema, fixtures and both validators together

## Acceptance criteria

- [x] S01-A01: Closing the window does not stop monitoring
- [x] S01-A02: Session state survives restart; invalid or unauthenticated IPC input is rejected
- [x] S01-A03: Unknown and stale agent status shown truthfully
- [ ] S01-A04: Tray and SQLite behavior tested on the primary dev OS

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 01
Completed tasks (commits): not committed yet (awaiting approval)

Commands run and results (Windows 11 Enterprise 10.0.26200, x64, Node 24.14.1, pnpm 12.6.0, Rust 1.99.1):
- `pnpm check`: all 11 steps pass, after the security-review fixes.
- Tests: 41 Rust (agent-core 16 unit + 2 shared-fixture suites; local-store 10;
  agent-ipc 10 protocol/token + 3 over the real named pipe), 92 TS (79 schema fixture cases,
  13 desktop view/store tests). TS and Rust agree on every agent-event fixture, including the
  new null, zero-width/RLM/BOM and line-separator cases.
- `cargo clippy -p agent-ipc --all-targets --target x86_64-unknown-linux-gnu` and
  `--target aarch64-apple-darwin`: clean (Unix socket code compiles; not run).
- App smoke test (debug build, real named pipe, `examples/send_event`, DOM read via WebView2
  DevTools):
  - Events stored, a duplicate reported as duplicate, an invalid event rejected (`invalid_event`).
  - Wrong, empty, truncated and uppercased tokens all rejected (`unauthorized`).
  - A future-dated event (year 9999) followed by a current one: state moved to "Needs you".
  - Window closed (WM_CLOSE): process still running, still accepting events (A01).
  - Restart: open sessions shown as Unknown, "Not confirmed since Lullshift started", with
    the last known state; a terminal session stays Completed; a new event restores the live
    state (A02, A03).
  - A second launch exits at once; one instance keeps running.
  - The pipe name carries a fresh random nonce each launch.

Tests not run and why:
- Tray icon clicks (left click to show, "Show Lullshift", "Quit Lullshift"): not automated, and
  no manual check yet. S01-A04 stays open until someone clicks them on Windows.
- Turning start-at-sign-in on: not exercised (it writes a Windows startup entry). The off state
  and status read were checked in the UI.
- Live 10-minute stale wait: covered by reducer and store tests, not by waiting in the app.
- macOS and Linux runtime: no machines. Unix IPC code compiles and lints for both targets.

Security/privacy review: security-qa-reviewer agent, read-only, on the full change set.
- Fixed now: predictable Windows pipe name let another local user squat it and receive
  forwarders' tokens (random per-launch name, test added); a future-dated event froze session
  state (timestamps capped at receipt time, tests added); Rust accepted explicit null for
  optional fields unlike the schema (rejected, fixtures added); zero-width, RLM, BOM, line
  separator and similar invisible characters were allowed in displayed ids (blocked in schema
  and Rust, fixtures added); the accept loop could end silently while the UI said "live" (now
  retries with backoff, dashboard shows "stopped"); `dashboard` ran on the UI thread under the
  store lock (now off-thread; UI refreshes debounced).
- Smoke-test hygiene: an earlier script run wrote stray files into apps/desktop, one named
  with the IPC token. Deleted before any commit; the token was rotated.
- Deferred with owners: owner-only DACL for the Windows pipe (ADR-009 residual risk, S02-T06);
  event log retention (S04-T06).
- Checked, no issue: token redaction and constant-time compare, no input echo in replies,
  64 KiB cap before parsing, read timeout, rate limit after auth, endpoint.json has no secret,
  Unix 0700/0600, downgrade refusal, per-step migration transactions with backup, parameterized
  SQL, fixed error codes in the UI, capability grants only what the UI uses.

Remaining defects and risks:
- Default pipe DACL: other local users who learn the name can hold connection slots (ADR-009).
- Event log has no retention yet (S04-T06).
- Smoke test left "smoke"-provider sessions in the developer's local database.
- The webview may enable autostart (granted on purpose for the setting; mitigated by the CSP).

Acceptance checks passed/failed:
- S01-A01 passed (window close keeps monitoring).
- S01-A02 passed (state survives restart; unauthenticated and invalid IPC input rejected).
- S01-A03 passed (unknown after restart verified live; stale verified by tests).
- S01-A04 open: SQLite behavior tested; tray menu clicks need one manual check on Windows.

Recommended next action: click-check the tray on Windows, commit Sprint 01, then start Sprint 02
(Claude Code adapter and forwarder).
```
