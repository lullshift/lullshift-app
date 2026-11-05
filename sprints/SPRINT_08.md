# Sprint 08 - Git Marketplaces

**Goal:** Users add public or private Git repos as marketplaces and install activities pinned to commits.

**Depends on:** Sprints 05 and 06/07 (for real content).

## Work items

- [ ] S08-T01: `.lullshift/marketplace.json` for this repo; built-in marketplace bundled into the app at the release tag, removable never, disable-able per activity
- [ ] S08-T02: Marketplace manager: add/remove/list/refresh via hardened system `git` in an app-owned cache (no hooks, no submodules, no LFS, restricted protocols, timeouts)
- [ ] S08-T03: Install/update/uninstall with commit pinning, strict validation, install review screen (source, commit, kind, permissions, unverified notice) and explicit updates showing permission changes
- [ ] S08-T04: Private repo support through the user's existing Git credentials; clear errors when access fails
- [ ] S08-T05: Malicious-repo fixture suite; third-party code activities enabled only on OSes where Sprint 03 sandbox proof passed
- [ ] S08-T06: Close marketplace schema gaps found in the Sprint 00 security review, with fixtures: `git` URLs with embedded credentials (`user:token@`), SSH option injection (`ssh://-o...`, `git0 0o...`), shorthand owner starting with `-`, `..`/`.git` repo segments, loopback/LAN hosts, control characters; a `ref` pattern (no leading `-`, `..`, `:`, `^`, `~`, `*`, whitespace) plus `git check-ref-format` at runtime; segment-based `relativePath` (no `.`/empty/`.git` segments, no leading `-`, no Windows reserved names); enforce the reserved `lullshift` name. At runtime always pass `--`/`--end-of-options` before URLs and refs and set `GIT_ALLOW_PROTOCOL=https:ssh`

## Acceptance criteria

- [ ] S08-A01: Install from a public and a private test repo works; no credential is ever stored or logged by Lullshift
- [ ] S08-A02: Malicious-repo fixtures are rejected
- [ ] S08-A03: Updates never happen silently; a new permission requires explicit approval
- [ ] S08-A04: Built-in activities work fully offline on first launch

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 08
Completed tasks (commits):
Commands run and results:
Tests not run and why:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next action:
```
