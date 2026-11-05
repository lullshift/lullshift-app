# Sprint 11 - v1.0 Release

**Goal:** A signed, installable, documented open-source v1 on Windows, macOS and Linux.

**Depends on:** Sprints 00-10 gates (content may be below target per ADR-003).

## Work items

- [ ] S11-T01: Installers, signed updater and release provenance. Product owner decision (2026-11-05): build installers with a GitHub Actions workflow that runs only on manual `workflow_dispatch` (never on push, merge or PR, to protect the free-plan quota) and attaches artifacts to a draft GitHub Release; amend ADR-004 accordingly. Windows/Apple code-signing certificates are not available yet: plan for unsigned installers with documented SmartScreen/Gatekeeper warnings unless that changes
- [ ] S11-T02: Full threat-case, privacy, migration and cross-platform QA, including every check in `docs/MANUAL_TESTS.md` (product owner runs them together)
- [ ] S11-T03: Accessibility audit and onboarding polish (hook consent, first activity)
- [ ] S11-T04: User docs, creator docs (SDK, kits, engines, marketplace repos) and contribution guide
- [ ] S11-T05: Release notes and feedback channels (GitHub issues/discussions); release only with explicit human approval
- [ ] S11-T06 (Sprint 02 review follow-up): the OS uninstaller removes Lullshift's Claude Code hooks (same logic as Disconnect) so Claude Code is not left calling a missing executable (ADR-010); check how Claude Code behaves when the hook executable is missing

## Acceptance criteria

- [ ] S11-A01: Smoke tests on Windows/macOS/Linux, or blocked platforms clearly flagged
- [ ] S11-A02: Signed update validation passes
- [ ] S11-A03: No account, server or telemetry dependency anywhere in the product
- [ ] S11-A04: Release checklist signed off by the product owner

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 11
Completed tasks (commits):
Commands run and results:
Tests not run and why:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next action:
```
