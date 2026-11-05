# Sprint 01 - Desktop Shell, State and Local IPC

**Goal:** The app runs from the tray, persists agent sessions locally and accepts events over authenticated local IPC.

**Depends on:** Sprint 00.

## Work items

- [ ] S01-T01: Tauri tray lifecycle, hide-to-tray, autostart option and notification permission UX
- [ ] S01-T02: Rust event reducer and session state machine with migration-managed SQLite (WAL)
- [ ] S01-T03: Authenticated Unix domain socket / Windows named pipe for hook ingestion
- [ ] S01-T04: React dashboard with Zustand view state showing sessions, unknown/stale states and empty state
- [ ] S01-T05: Restart, duplicate, reorder and stale-state tests; migration rollback test

## Acceptance criteria

- [ ] S01-A01: Closing the window does not stop monitoring
- [ ] S01-A02: Session state survives restart; invalid or unauthenticated IPC input is rejected
- [ ] S01-A03: Unknown and stale agent status shown truthfully
- [ ] S01-A04: Tray and SQLite behavior tested on the primary dev OS

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 01
Completed tasks (commits):
Commands run and results:
Tests not run and why:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next action:
```
