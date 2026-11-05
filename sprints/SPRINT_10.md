# Sprint 10 - More Adapters and Content Completion

**Goal:** Broader agent support and the full v1 content set.

**Depends on:** Sprints 02, 06, 07, 08.

## Work items

- [x] S10-T01: Research and implement Codex and Cursor adapters only where documented lifecycle APIs exist; capability matrix
- [x] S10-T02: Games to 20 and activities to ~100 (release target, not blocker)
- [x] S10-T03: Daily seeded puzzles for grid games
- [x] S10-T04: Content review pass: originality, tone, accessibility, licenses

## Acceptance criteria

- [x] S10-A01: Adapter capability gaps documented; no unsupported claims
- [x] S10-A02: All bundled items pass activity gates
- [x] S10-A03: Content count reported honestly against the 20/100 target

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 10
Completed tasks (commits): local commits for the Codex and Cursor adapters, the eleven new
  games, the new content packs and marketplace entries, and this closeout. Local only, not
  pushed.
Commands run and results:
  pnpm check (full, Windows 11): 13 of 14 steps passed on the first run; format failed on
    capabilities/default.json (again), fixed with prettier and re-checked: pass. Bundled
    activities (validate + simulate) 102 of 102 pass; cargo test all pass (agent-adapters
    39 unit + 6/15/17/14 integration, desktop lib 117); TS tests: desktop 1047, protocol 177,
    kits 82, SDK 18, CLI 43.
  After the content and adapter review fixes: prettier --check, validate/simulate on the
    edited games and their unit tests: pass.
  Debug app over CDP: all 11 new games and all new packs listed under Activities; Tea Sort,
    Harvest Pairs and Island Bridges opened and rendered in the host (Tea Sort's sixth tea
    pattern was broken in the real host and fixed; see risks).
Tests not run and why:
  Real Codex and Cursor sessions on an installed release build (M10-1, M10-2): needs the
    agents and the owner's machine. Screen reader, keyboard-only and reduced-motion pass over
    the new content (M10-3). Clock Patience auto-play against a real host pause (M10-4).
    macOS and Linux.
Security/privacy review: security-qa-reviewer on the Codex and Cursor adapters: no critical,
  high or medium. Low fixes with regression tests: duplicate-key and over-64-bit numbers
  refused (file untouched), 1 MiB cap and regular-file check before reading (no FIFO hang),
  relative CODEX_HOME ignored, "Hooks turned off" only when Lullshift's hooks are present,
  Codex SessionEnd timeout 3 s. Content review (S10-T04): originality, facts (all 20 quiz
  answers and 17 teasers checked), tone, accessibility, sandbox; five pack text fixes;
  medium fixes: Tea Sort letters and pattern, Island Bridges finished-island cue, six
  game READMEs completed.
Remaining defects and risks:
  Low, accepted (match the Claude Code adapter): uninstall has no Rust-side confirmation;
    backups of the user's hook files are kept. The Codex config.toml reader misses inline
    tables and quoted keys (status only). Small race between the last re-read and rename.
  Codex details not re-checked against the docs: the features key, Interrupt, async on
    PermissionRequest, Windows shell (M10-1 covers them).
  Release builds on Windows are GUI programs; hooks through PowerShell must be checked on an
    installed build (noted in M10-1/M10-2).
  Harvest Pairs and Island Bridges need a scroll at the default window size.
  Size control row inconsistent across grid games; "Tea Sort" name not checked against
    stores. Product choices made while the owner was away: DECISIONS_FOR_REVIEW.md 21-24.
Acceptance checks passed/failed:
  A01: docs/ADAPTER_CAPABILITIES.md lists every gap ("not available"/"partial"), nothing
    guessed. A02: all 102 bundled items pass validate and simulate. A03 (honest count):
    20 of 20 games; 92 playable activities (20 games, 71 content packs, 1 break) against
    the ~100 target, plus 10 engines that run the packs. Target not reached; not a blocker.
  T03: all 11 grid-kit games have a daily seeded puzzle (Fog Garden's added this sprint).
  Manual checks M10-1 to M10-4 pending.
Recommended next action: review decisions 21-24; run M10-1 and M10-2 on a release build;
  then Sprint 11 (release), which needs owner approval for signing and publishing.
```
