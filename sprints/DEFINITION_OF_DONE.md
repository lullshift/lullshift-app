# Definition of Done and Release Gates

A sprint is complete only when observed behavior meets the documented acceptance criteria, the relevant test suites have actually been executed, and risks are documented. Generated code alone is never "done".

## Mandatory gates

- [ ] The local check command (ADR-004) passes; unit, integration, type, lint, format and secret-scan results recorded with OS.
- [ ] User stories tested on the development OS; release gates include Windows, macOS and Linux smoke results (or clearly flagged blocked platforms).
- [ ] External APIs (Tauri, agent hooks, AI providers) validated against current official documentation.
- [ ] State changes tested with duplicate, reordered, missing and malicious inputs.
- [ ] No account, sign-in or network dependency introduced for any local feature.
- [ ] Secrets (BYOK keys) only in the OS keychain; never in SQLite, logs or error traces.
- [ ] Privacy: no implicit context capture, no prompt/source upload, no default telemetry, no employee metrics.
- [ ] Agent hooks installed/uninstalled reversibly, with consent and backups.
- [ ] Migration and rollback paths tested for persistent changes.
- [ ] Keyboard, screen-reader labels, reduced motion and error/offline states inspected.
- [ ] Documentation, ADRs and release notes updated.

## Activity gates (every game, engine, kit and content pack)

- [ ] Manifest validates against `reference/ACTIVITY_MANIFEST_V1.schema.json`; content data validates against its engine's data schema.
- [ ] Simulator sequences pass: immediate `needs_attention`, duplicate/out-of-order events, forced pause, close, crash/reload, resume, offline.
- [ ] State restored exactly after pause and after restart.
- [ ] Keyboard-only play, screen-reader labels, reduced-motion mode.
- [ ] Original name and art; asset licenses declared.

## Marketplace and sandbox gates

- [ ] Sandbox denial tests (Tauri IPC, network, filesystem, clipboard, cross-activity storage, navigation) pass on each target OS before third-party code activities are enabled there.
- [ ] Host freezes/pauses activities on `needs_attention` without activity cooperation.
- [ ] Malicious-repo fixtures rejected: path traversal, escaping symlinks, oversized files, Git hooks, submodules, invalid index/manifest, executables in content packs.
- [ ] Installs pinned to commit SHA; updates explicit and show permission changes.
- [ ] Lullshift never prompts for, stores or logs Git credentials.

## Required QA evidence

Record the exact command and exit code, environment, fixture identifiers, failed/blocked checks and manual steps. Report platform gaps honestly.

Every check that needs a person, an account or another OS goes into `docs/MANUAL_TESTS.md` with exact steps and expected results when the sprint closes. The product owner runs them all together before release (S11-T02).
