# ADR-004: Local quality checks instead of hosted CI (for now)

- **Status:** Accepted
- **Date:** 2026-11-05
- **Owner:** Product owner
- **Revisit:** when credentials/permissions for a hosted CI service are available

## Context

The plan assumed GitHub Actions for build, test, lint, schema validation and secret scanning on Windows, macOS and Linux. The product owner does not currently have the credentials/permissions needed to set up hosted CI.

## Decision

1. **No hosted CI for now.** (One manual-only release workflow is allowed, see the 2026-11-05 amendment below.) Do not add `.github/workflows/` or any other hosted pipeline configuration.
2. **One local check entry point** runs everything CI would have run: format check, lint, typecheck, unit/integration tests, build, schema and fixture validation (including that invalid fixtures fail), and a secret scan. The exact runner (e.g. a `package.json` script, `just`, or a cross-platform script) is chosen in Sprint 00 and must work on Windows, macOS and Linux.
3. **Git hooks are opt-in.** The repo provides an installable pre-commit hook (fast checks + secret scan) and a pre-push hook (full check). Contributors enable them explicitly; nothing installs hooks automatically.
4. **Evidence is recorded by hand.** Sprint closeouts and release gates report the exact local commands, exit codes and the OS they ran on.
5. **Cross-platform coverage is manual.** Checks on OSes other than the developer's machine are run on available machines or VMs and recorded; untested OSes are reported as blocked, never assumed to pass.
6. **Keep the check script CI-ready.** Hosted CI can later simply call the same entry point, so adding it should not require changing any checks.

## Consequences

- Regressions are caught only when someone runs the checks; the pre-push hook and sprint closeout evidence are the safeguards.
- Windows/macOS/Linux confidence depends on manual runs, which matters most for the activity sandbox (per-OS proof) and release builds.
- Release artifacts (Sprint 11) are built and signed locally; signing keys stay outside the repo.


## Amendment 2026-11-05: manual release workflow (S11-T01)

Product owner decision. Point 1 of the Decision gets one exception: **a single GitHub Actions workflow, `.github/workflows/release.yml`, is allowed, and it runs only on manual `workflow_dispatch`.** It never runs on push, pull request, merge or a schedule, which protects the free-plan quota. Quality checks stay local (points 2 to 6 are unchanged); this workflow does not run tests.

- **What it does:** builds installers on Windows (NSIS), macOS (universal DMG) and Linux (deb, AppImage), writes `SHA256SUMS`, adds a build-provenance attestation and attaches everything to a **draft** GitHub Release. The owner reviews and publishes by hand. Source and user data are never uploaded.
- **Hardening:** every action is pinned to a full commit SHA; the default token is read-only and only the release job has `contents: write`, `id-token: write` and `attestations: write`; build jobs never see the release token; `tauri-apps/tauri-action` is not used. A test (`apps/desktop/src-tauri/tests/release_config.rs`) fails if another trigger, an unpinned action or a second workflow appears.
- **Signing:** no Windows or Apple certificates exist. Installers are unsigned (macOS ad-hoc), and the SmartScreen/Gatekeeper warnings are documented in `docs/RELEASING.md`. This replaces the earlier consequence that release artifacts are built and signed locally; signing keys, if added later, live in repository secrets or on the owner's machine, never in the repository.
- **Updater:** Tauri's updater needs a minisign key pair that the owner has not generated. The workflow reads `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` from repository secrets; no private key is generated or committed. Until a public key is in `tauri.conf.json`, the updater plugin is not a dependency, `createUpdaterArtifacts` is false and no update check can happen. When enabled, checks are manual (user-initiated) only, never at startup or in the background. Key custody and the update flow get their own ADR-018 at that point (reserved in `docs/TECHNICAL_ARCHITECTURE.md`).
- **Revisit:** if a full hosted CI is added later, it should call the same local entry point (point 6).
