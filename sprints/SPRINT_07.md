# Sprint 07 - Engines and First Content

**Goal:** Data-driven engines let most activities ship as code-free content packs.

**Depends on:** Sprint 05.

## Work items

- [ ] S07-T01: Engines with public versioned data schemas: breathing, movement routine, guided pause, quiz, brain teaser, reflection, creative toy, typing drills, away prompts
- [ ] S07-T02: Engine data-schema compatibility tests (old packs keep working within a major version)
- [ ] S07-T03: At least 40 content packs across engines, original text and assets with licenses
- [ ] S07-T04: Content style guide (tone, no medical claims, play not curriculum)
- [ ] S07-T05: Accessibility review of every engine

## Acceptance criteria

- [ ] S07-A01: Each engine passes the activity gates
- [ ] S07-A02: At least 40 content packs validate against their engine schemas
- [ ] S07-A03: Breaking a pack's data fails validation with a clear message

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 07
Completed tasks (commits):
Commands run and results:
Tests not run and why:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next action:
```
