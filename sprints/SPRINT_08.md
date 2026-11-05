# Sprint 08 - Git Marketplaces

**Goal:** Users add public or private Git repos as marketplaces and install activities pinned to commits.

**Depends on:** Sprints 05 and 06/07 (for real content).

## Work items

- [x] S08-T01: `.lullshift/marketplace.json` for this repo; built-in marketplace bundled into the app at the release tag, removable never, disable-able per activity
- [x] S08-T02: Marketplace manager: add/remove/list/refresh via hardened system `git` in an app-owned cache (no hooks, no submodules, no LFS, restricted protocols, timeouts)
- [x] S08-T03: Install/update/uninstall with commit pinning, strict validation, install review screen (source, commit, kind, permissions, unverified notice) and explicit updates showing permission changes
- [x] S08-T04: Private repo support through the user's existing Git credentials; clear errors when access fails
- [x] S08-T05: Malicious-repo fixture suite; third-party code activities enabled only on OSes where Sprint 03 sandbox proof passed
- [x] S08-T06: Close marketplace schema gaps found in the Sprint 00 security review, with fixtures: `git` URLs with embedded credentials (`user:token@`), SSH option injection (`ssh://-o...`, `git0 0o...`), shorthand owner starting with `-`, `..`/`.git` repo segments, loopback/LAN hosts, control characters; a `ref` pattern (no leading `-`, `..`, `:`, `^`, `~`, `*`, whitespace) plus `git check-ref-format` at runtime; segment-based `relativePath` (no `.`/empty/`.git` segments, no leading `-`, no Windows reserved names); enforce the reserved `lullshift` name. At runtime always pass `--`/`--end-of-options` before URLs and refs and set `GIT_ALLOW_PROTOCOL=https:ssh`

## Acceptance criteria

- [ ] S08-A01: Install from a public and a private test repo works; no credential is ever stored or logged by Lullshift *(installs from local test repositories and credential refusal are tested; real public/private hosts are manual test M08-1)*
- [x] S08-A02: Malicious-repo fixtures are rejected
- [x] S08-A03: Updates never happen silently; a new permission requires explicit approval
- [ ] S08-A04: Built-in activities work fully offline on first launch *(built-ins are compiled in and nothing fetches on launch, confirmed by review and in the debug app; a fresh-profile offline launch is manual test M08-3)*

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 08
Completed tasks (commits): local commits "Add the hardened Git marketplace crate...", "Store
  marketplaces and installed activities...", "Add marketplaces to the app..." and this
  closeout. Local only, not pushed.
Commands run and results:
  pnpm check (full, Windows 11, Node 24, git 2.53, Edge headless): 12 of 14 steps passed on the
    first run; secret scan flagged 5 deliberately fake credentials in refusal tests (now
    allowed line by line) and format flagged capabilities/default.json; both re-run and pass.
  cargo test --workspace: marketplace 97 (incl. real malicious git repos), desktop 85,
    local-store 24 marketplace/migration tests plus the earlier suites; all pass.
  TS tests: agent-protocol 177, desktop 699 (29 marketplace), kits 82, SDK 18, CLI 43: pass.
  Schema fixtures: 53 new invalid, 3 valid and 3 runtime-invalid marketplace fixtures, shared
    by Ajv and Rust. check-bundled: all 70 activities pass; built-in index still valid.
  Debug app over CDP: Marketplaces tab lists the built-in; localhost and credential URLs are
    refused with clear text; a missing repo says "wasn't found"; disabling a built-in removes
    it from the Library and re-enabling brings it back.
Tests not run and why:
  Installs from a real public and private Git host (M08-1): no test repository online and
    publishing one needs the owner. Update flow on a real host (M08-2). Fresh-profile offline
    launch (M08-3). macOS and Linux, including the Unix-only tests (M08-4).
Security/privacy review: security-qa-reviewer, no critical or high. Fixed M1 (re-adding a
  removed name from another URL could swap sources; now refused, and updates flag a source
  change), M2 (fetch size cap 200 MiB with staging), L1 redirects off, L2 trace/TLS env
  scrubbed, L3 fsck on, L4 serve only verified bytes, L5 per-activity install lock, L6 wider
  invisible-character check, L7 saved state from another source cleared (migration 0007, with
  upgrade and rollback tests). Each with a regression test.
Remaining defects and risks:
  The fetch cap and timeouts are tested with local repositories only.
  The local-host rule is text-only; a DNS name can still resolve to a local address (git
    redirects are off, so only the first hop matters).
  Third-party code activities are blocked on every OS (decision 15).
  Product choices made while the owner was away: DECISIONS_FOR_REVIEW.md items 15-18.
Acceptance checks passed/failed: A02 malicious-repo fixtures rejected (passed); A03 updates
  are explicit and show added permissions and source changes (passed); A01 and A04 are
  implemented and covered by automated tests but need the manual checks above.
Recommended next action: create a small public test marketplace and run M08-1/M08-2; review
  decisions 15-18; then Sprint 09.
```
