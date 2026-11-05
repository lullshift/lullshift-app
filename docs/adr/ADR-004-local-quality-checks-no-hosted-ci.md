# ADR-004: Local quality checks instead of hosted CI (for now)

- **Status:** Accepted
- **Date:** 2026-11-05
- **Owner:** Product owner
- **Revisit:** when credentials/permissions for a hosted CI service are available

## Context

The plan assumed GitHub Actions for build, test, lint, schema validation and secret scanning on Windows, macOS and Linux. The product owner does not currently have the credentials/permissions needed to set up hosted CI.

## Decision

1. **No hosted CI for now.** Do not add `.github/workflows/` or any other hosted pipeline configuration.
2. **One local check entry point** runs everything CI would have run: format check, lint, typecheck, unit/integration tests, build, schema and fixture validation (including that invalid fixtures fail), and a secret scan. The exact runner (e.g. a `package.json` script, `just`, or a cross-platform script) is chosen in Sprint 00 and must work on Windows, macOS and Linux.
3. **Git hooks are opt-in.** The repo provides an installable pre-commit hook (fast checks + secret scan) and a pre-push hook (full check). Contributors enable them explicitly; nothing installs hooks automatically.
4. **Evidence is recorded by hand.** Sprint closeouts and release gates report the exact local commands, exit codes and the OS they ran on.
5. **Cross-platform coverage is manual.** Checks on OSes other than the developer's machine are run on available machines or VMs and recorded; untested OSes are reported as blocked, never assumed to pass.
6. **Keep the check script CI-ready.** Hosted CI can later simply call the same entry point, so adding it should not require changing any checks.

## Consequences

- Regressions are caught only when someone runs the checks; the pre-push hook and sprint closeout evidence are the safeguards.
- Windows/macOS/Linux confidence depends on manual runs, which matters most for the activity sandbox (per-OS proof) and release builds.
- Release artifacts (Sprint 11) are built and signed locally; signing keys stay outside the repo.
