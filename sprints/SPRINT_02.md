# Sprint 02 - Claude Code Adapter

**Goal:** Real Claude Code sessions are observed through hooks that are installed and removed safely.

**Depends on:** Sprint 01.

## Work items

- [ ] S02-T01: Confirm current Claude Code hook payloads and event semantics from official docs; write capability matrix
- [ ] S02-T02: Hook forwarder and consent-based, idempotent installer with backups and clean uninstall
- [ ] S02-T03: Map hook events to normalized session state, including ambiguous turn-stop vs task completion
- [ ] S02-T04: Handle simultaneous sessions and missing/replayed events
- [ ] S02-T05: Adapter fixtures plus install/upgrade/uninstall tests
- [ ] S02-T06 (Sprint 01 review follow-up): decide on an owner-only DACL for the Windows pipe (ADR-009 residual risk); forwarder reads `endpoint.json` on every send since the pipe name changes per launch, and fails fast when the app isn't running

## Acceptance criteria

- [ ] S02-A01: Real Claude Code smoke test shows working / needs_attention / turn events
- [ ] S02-A02: No duplicated attention notifications on replay
- [ ] S02-A03: Uninstall restores previous hook configuration exactly
- [ ] S02-A04: Capability matrix documented

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 02
Completed tasks (commits):
Commands run and results:
Tests not run and why:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next action:
```
