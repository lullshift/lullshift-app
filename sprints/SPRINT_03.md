# Sprint 03 - Notifications and Activity Host

**Goal:** Attention-aware notifications and a sandboxed activity host that the host itself can pause, with Break and one reference game.

**Depends on:** Sprint 02.

## Work items

- [ ] S03-T01: Attention-priority native notifications, quiet hours and preferences
- [ ] S03-T02: Activity host: isolated WebView/origin, CSP, no Tauri IPC/network/filesystem, nonce-bound lifecycle bridge, namespaced storage quota
- [ ] S03-T03: Host-enforced pause/freeze on `needs_attention`, snapshot persistence and crash recovery
- [ ] S03-T04: Break activity (away mode, gentle return) and one original reference puzzle game built on the bridge
- [ ] S03-T05: Sandbox denial test suite; record per-OS results in an ADR (activity sandbox strategy)
- [ ] S03-T06: Give activities their own origin with an explicit `frame-src` in the app CSP (never `'self'`) (Sprint 00 security review)

## Acceptance criteria

- [ ] S03-A01: Agent attention pauses the running activity without its cooperation and without losing progress
- [ ] S03-A02: Turn-stop is never shown as task completion
- [ ] S03-A03: Sandbox denial tests pass on the dev OS; other OS results recorded or flagged blocked
- [ ] S03-A04: Keyboard and screen-reader smoke checks documented

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 03
Completed tasks (commits):
Commands run and results:
Tests not run and why:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next action:
```
