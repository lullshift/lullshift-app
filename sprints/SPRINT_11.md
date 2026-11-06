# Sprint 11 - v1.0 Release

**Goal:** A signed, installable, documented open-source v1 on Windows, macOS and Linux.

**Depends on:** Sprints 00-10 gates (content may be below target per ADR-003).

## Work items

- [ ] S11-T01: Installers, signed updater and release provenance. Product owner decision (2026-11-05): build installers with a GitHub Actions workflow that runs only on manual `workflow_dispatch` (never on push, merge or PR, to protect the free-plan quota) and attaches artifacts to a draft GitHub Release; amend ADR-004 accordingly. Windows/Apple code-signing certificates are not available yet: plan for unsigned installers with documented SmartScreen/Gatekeeper warnings unless that changes
- [ ] S11-T02: Full threat-case, privacy, migration and cross-platform QA, including every check in `docs/MANUAL_TESTS.md` (product owner runs them together)
- [ ] S11-T03: Accessibility audit and onboarding polish (hook consent, first activity)
- [x] S11-T04: User docs, creator docs (SDK, kits, engines, marketplace repos) and contribution guide
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
Sprint: 11 (in progress: everything that can be done without the owner is done)
Completed tasks (commits): local commits "Accessibility fixes and a first-launch welcome",
  "Add a manual release workflow and remove agent hooks on uninstall", "Add user and creator
  guides, contribution and security docs" and this status update. Local only, not pushed,
  nothing tagged or released.
Commands run and results:
  pnpm check (full, Windows 11): all 14 steps pass (desktop 1064 TS tests, protocol 177,
    kits 82, SDK 18, CLI 43; cargo test all pass incl. desktop lib 125 and release_config 4;
    102 bundled activities validate and simulate).
  actionlint 1.7.7 on .github/workflows/release.yml: 0 errors. Action SHA pins checked
    against their tags with gh api.
  pnpm tauri build --bundles nsis (release profile): built
    target/release/bundle/nsis/Lullshift_0.1.0_x64-setup.exe (5.81 MiB). Not run. An
    earlier attempt failed at link (0xc0000142) while other builds were running; the clean
    retry passed.
  prerm exercised in Git Bash with stubbed getent/dpkg/runuser; hooks.nsh compiled with
    makensis; uninstall mode tested in temp dirs only (8 tests).
  Debug app over CDP: the first-launch welcome shows, Skip focused first, wording matches
    the installer; left undismissed for M11-1.
  Creator tutorial run end to end outside the repo (create, validate, simulate, pack).
Tests not run and why:
  The release workflow (never dispatched; owner runs it, M11-5). Installing and uninstalling
    on real Windows, Linux and macOS (M11-5, M11-6). Signed update (no key yet, M11-7).
    Screen reader, keyboard-only, zoom (M11-1 to M11-3). Network capture (M11-4). All
    earlier manual checks M01-M10 (S11-T02: the owner runs them together).
Security/privacy review: (1) no-account/telemetry audit: pass on code and lockfiles; every
  outbound host listed in docs/user/privacy.md; self-navigation already covered by the
  sandbox runner; DNS prefetch moved into M11-4. (2) release and uninstall review: M1 fixed
  (draft job only from main; provenance check pins workflow and ref). Lows fixed: empty
  database file left untouched, signing secret not passed until the updater exists, 30 s
  limit per user in prerm. Documented: cancelled Windows uninstall disconnects agents;
  Linux per-user env overrides in profiles are not seen. Known, accepted: WebRTC is not
  blocked in activities (third-party code activities stay disabled, ADR-012).
Remaining defects and risks:
  Workflow details unverified until first run (macOS universal target, Linux packages,
    bundle paths). Write access to the repo is the release trust boundary until the
    `release` environment with a reviewer exists.
  Product choices made while the owner was away: DECISIONS_FOR_REVIEW.md items 25-26.
Acceptance checks passed/failed:
  A01 smoke tests: Windows release installer builds; no platform smoke-tested yet (blocked
    on M11-5). A02 signed update: blocked, no key (owner step, RELEASING.md section 3).
  A03 no account/server/telemetry: passes on evidence; tick after M11-4. A04: owner
    sign-off pending.
  T01, T06: code complete, waiting for the workflow run and M11-5/M11-6. T03: done in code,
    waiting for M11-1 to M11-3. T05: notes drafted in docs/RELEASE_NOTES.md and issue
    templates added; owner turns on Discussions and private vulnerability reporting and
    approves the release.
Recommended next action (owner): review decisions 21-26; run the Release workflow once;
  run docs/MANUAL_TESTS.md; then decide the version and approve the release.
```
