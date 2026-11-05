# Sprint 09 - Companion (BYOK)

**Goal:** A separate conversational workspace using the user's own AI provider key.

**Depends on:** Sprints 01 and 04.

## Work items

- [ ] S09-T01: Provider-neutral BYOK settings; keys in OS keychain only
- [ ] S09-T02: Companion threads stored locally; explicit context scopes (none / session summary / selected notes)
- [ ] S09-T03: Network failure, rate limit and budget handling
- [ ] S09-T04: Tests proving keys never reach SQLite, logs or error traces

## Acceptance criteria

- [ ] S09-A01: BYOK key never appears in SQLite, logs or UI errors
- [ ] S09-A02: Context sharing requires an explicit scope choice
- [ ] S09-A03: Companion never gains agent execution privileges

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 09
Completed tasks (commits):
Commands run and results:
Tests not run and why:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next action:
```
