# Sprint 12 - Owner review changes

**Goal:** Build the changes the product owner asked for when reviewing `DECISIONS_FOR_REVIEW.md` on 2026-11-06, before the first release (0.1.0).

**Depends on:** Sprints 00-11. Sprint 11's owner steps (the workflow run, manual checks and release approval) stay open. The release follows this sprint.

## Work items

Agents (decisions 1, 22, 23):

- [x] S12-T01: OpenCode adapter. Research OpenCode's documented hook or plugin API, then build an observe-only adapter like the Codex and Cursor ones. Include consent, a backup, idempotent install, clean Disconnect, uninstall cleanup, a row in the capability matrix and a security review.
- [x] S12-T02: Cursor mapping. `sessionEnd` with reason `error` maps to `failed`, and the error message is never read. Register `postToolUseFailure`, `subagentStop` and `preCompact` as `working` signals.
- [x] S12-T03: Hook command paths are escaped for each shell (PowerShell, POSIX sh) instead of being limited to an allowlist. Control characters still refuse install. Add property tests and a security review.
- [x] S12-T04: Extend the task-text opt-in (off by default, stored locally) to Codex, Cursor and OpenCode prompts. Needs a privacy review.

Release and uninstall (decision 26):

- [x] S12-T05: AppImage hooks point at `$APPIMAGE`, so they survive restarts. A reinstall over an existing Windows install keeps the agent hooks.
- [x] S12-T06: Record the update-check choice in ADR-018: once the updater exists, an opt-in setting (off by default) checks once a day, and the manual button stays. No updater code is added until the signing key exists.

Desktop UI (decisions 2, 25):

- [x] S12-T07: The welcome offers "Connect an agent" (opens Integrations), and hides it when every supported agent is already connected. Add a "Skip to main content" link and `aria-describedby` on consent dialogs. Show activity-supplied paused-view stats in their own group, headed by the activity's name.

Games and content (decisions 6, 8, 14, 24):

- [x] S12-T08: Rename Tea Sort to "Teacup Tidy". Thread Picture, Brook Links and Island Bridges get a "Size" button in the main controls row.
- [x] S12-T09: Game changes:
  - Port Lantern Pond onto a kit.
  - Rebuild Tide Pairs on the card kit's table binding.
  - Ridge Walk: a King wraps to an Ace and an Ace to a King, with no winnable-deal check.
  - Fog Garden: three sizes and a safe first reveal.
  - Moss Paths: 4x4, 6x6 and 8x8, and crossing tiles turn.
  - Firefly Catch: rounds of 60, 90 or 180 seconds, or untimed.
  - Kite Drift: a Pace button (slow, normal, brisk).
- [x] S12-T10: Listening steps can be done by hearing or by touch and sight. Ban U+2028 and U+2029 in every engine schema in one pass.

Packs and the pack checker (decisions 9, 13):

- [x] S12-T11: Packs may include checked PNG/WebP images, with size and count limits and decode checks. Amend ADR-016 and get a security review.
- [x] S12-T12: Replace the custom Rust pack-data checker with an established JSON Schema crate. Audit the dependency and run the shared Ajv/Rust corpus. The CSP stays as it is.

Sandbox and marketplaces (decisions 15, 17, 18):

- [x] S12-T13: Close the WebRTC gap in the activity sandbox on Windows, macOS and Linux. Third-party code activities stay blocked until the sandbox proof passes on each OS.
- [x] S12-T14: Marketplace changes, recorded in ADR-015, with a security review:
  - Private hosts are allowed after an explicit "on your own network" confirmation, and a resolved-address check catches public names that point at private addresses.
  - Same-host redirects are followed.
  - An explicit "change address" flow.
  - Installed files are re-checked on each load.
  - Before deleting saved progress that belongs to another source, the user is asked.
  - The schema, SDK and CLI hidden-character checks are aligned with the host.

Companion (decisions 19, 20):

- [x] S12-T15: Add streaming replies, with each chunk cleaned and budgets counted for partial replies. Remember the last context choice; the preview is still shown before every send. Cancel frees the thread at once, discards a late reply and still counts its usage.
- [x] S12-T16: Add three read-only Companion tools, each previewed and approved per call: Session overview, Suggest a break (a card the user clicks) and Activity help lookup. Amend ADR-020 and get a security review.

## Acceptance criteria

- [x] S12-A01: Each change above has tests. `pnpm check` passes in full.
- [x] S12-A02: Security reviews for T01, T03, T11, T13, T14 and T16 have no open high or medium findings.
- [x] S12-A03: ADR-015, ADR-016, ADR-018 and ADR-020 are amended, and the capability matrix and user and creator docs are updated.
- [x] S12-A04: New manual checks are logged in `docs/MANUAL_TESTS.md` as M12-x.

## Guardrails

- No accounts, hosted services or default telemetry. Network use stays user-initiated.
- Companion tools never act on agents, files, the shell or the web.
- Never weaken the sandbox or the Git hardening to make content work.
- Hooks stay observe-only, with explicit consent, backups, idempotent install and clean uninstall.

## Closeout

```text
Sprint: 12 (closed; everything the owner can't check by hand is done)
Completed tasks (commits): about 60 local commits from c523d31 to HEAD on main, made by
  subagents in separate worktrees and merged here, then the security review fixes. Local only,
  not pushed. Agent worktrees and branches removed after merging.
  T01 experimental OpenCode 2.x adapter (ADR-022). T02 Cursor sessionEnd error -> failed, plus
  3 non-blocking hooks. T03 per-shell path escaping. T04 task-text opt-in for Codex, Cursor and
  OpenCode. T05 AppImage path and Windows reinstall restore (reworked after review M1).
  T06 ADR-018 (proposed). T07 Connect an agent, skip link, aria-describedby, grouped stats.
  T08/T09 Teacup Tidy, Size buttons, Lantern Pond on the grid kit, Tide Pairs on the card kit
  table, game rule changes. T10 listening steps, U+2028/2029 ban. T11 checked PNG/WebP in packs
  (new crate pack-media). T12 jsonschema 0.60.2. T13 WebRTC off in the webview (new crate
  macos-webrtc). T14 marketplace private hosts, resolve check, same-host moves, change address,
  re-check on load, saved-progress question, aligned hidden characters. T15/T16 Companion
  streaming, remembered scope, cancel, three read-only lookups (off by default).
Commands run and results:
  pnpm check (full, Windows 11): all 14 steps pass (desktop 1201 TS tests, protocol 303, kits
    84, SDK 134, CLI 57; cargo test all pass incl. desktop lib 192, marketplace 139; 103
    bundled activities validate and simulate). The first full run failed lint only because
    ESLint scanned agent worktrees; .claude/** is now ignored.
  Windows sandbox proof (debug build, run-denial-tests.mjs --attention): 48/48, 0 known gaps.
  Agents also ran: makensis compile of the real installer.nsi with the new hooks.nsh; real
    sh, PowerShell 5.1 and 7 quoting tests; a native-exe argv test; node runs of the OpenCode
    plugin against a fake context; cargo check of macos-webrtc for aarch64-apple-darwin.
Tests not run and why:
  M12-1 to M12-9 (real agents, installers, screen readers, macOS/Linux sandbox proof, live
  Companion providers, real marketplaces): the owner runs all manual checks together. No real
  OpenCode 2.x was available. The real installer was compiled, not run.
Security/privacy review: three security-qa-reviewer passes (adapters; pictures, schema,
  WebRTC and Companion; marketplace). High: one (desktop test build broken by a merge),
  fixed. Mediums fixed: Windows reinstall left hooks after uninstall-then-cancel; cp1252
  quote risk on Windows paths (fail closed); final Companion round sent tool blocks without
  tools; PNG/WebP trailing payloads; DNS rebinding via index ssh sources and failed lookups;
  change address re-owned kept progress. Lows fixed: entry recognition, plugin prompt cutting
  and retries, overview copy and retention, duplicate tool ids, cancel race, unsafe confined
  to macos-webrtc with forbid restored, Windows WebRTC state honest and gate needs runtime
  proof, dev-only proxy bypass moved to tauri.dev.conf.json, [] and [^] regex, redirect probe
  pinned, IPv6 forms, transient read errors no longer mark damaged, engine schemas aligned.
Remaining defects and risks:
  OpenCode adapter built from source, not docs (experimental). Some Windows profile paths
  (e.g. Cyrillic В) can't connect Codex/Cursor. WebP VP8 trailing bytes undetectable. ssh
  marketplaces typed by the user can still be DNS-rebound; no pin behind a proxy. macOS and
  Linux WebRTC proof not run; third-party code stays blocked everywhere. Choices for the
  owner: DECISIONS_FOR_REVIEW.md item 27.
Acceptance checks passed/failed:
  A01 pass (full gate). A02 pass: no open high or medium findings after fixes. A03 pass:
  ADR-012, 013, 015, 016, 018 (proposed), 020, 021, 022 updated; capability matrix, user and
  creator docs updated. A04 pass: M12-1 to M12-9 logged.
Recommended next action (owner): review decision 27; run the manual checks (M01-M12); run
  the Release workflow once; then approve the 0.1.0 release.
```

## Follow-up after the owner's review of decisions 27 to 29 (2026-11-06)

Built after the sprint closed, at the owner's request ("Build them now", the bundled Git fallback, and npm publishing):

- [x] Decision 27.1: Windows hook paths with letters such as В, Ñ and Ó are accepted (ADR-021).
- [x] Decision 27.5: third-party code activities install on every platform after a risk warning and a consent tick, at each install and each update that changes the code (ADR-012, ADR-015 amendments). Migrations 0014 and 0015.
- [x] Decision 27.6: refused Companion lookups show a notice (ADR-020). Migration 0013.
- [x] Decision 27.8: in Moss Paths, turning a crossing is not a move (0.2.1).
- [x] Decision 28: built-in Git (gitoxide) for public https marketplaces when Git isn't installed (ADR-023).
- [x] Decision 29: the creator CLI, SDK and protocol packages can be published to npm through a manual, owner-approved workflow (ADR-017, ADR-004 amendments). Nothing is published.
- [x] Security reviews of 27.5, 28 and 29 (security-qa-reviewer). Every high and medium finding is fixed with tests:
  - 27.5: scheme flood (async handler, budget, small-file cache), cross-activity loads (per-launch keys, second-load freeze), storage rows (key cap, row cost, write budget), native IPC probe; lows: consent for every installed app row, more tests, dialog order and focus, "address changed" note.
  - 28: tree-bomb traversal (visit budget, depth, deadline), proxy bypass (built-in Git refuses when a proxy is set); lows: alloc limit and threads, NTFS `.git` spellings, rejected-pack cleanup, honest 403 text.
  - 29: publish job split from build (no credential near build tools; the checked tarballs are what is published, hash-verified), docs name the real controls and lead with the clone until `@lullshift` is registered; lows: guard pinned to this repo, integrity check on skip, stronger tarball checker, INIT_CWD, pnpm pinned by hash.
- [x] Fixed a clock race in the `repair_hooks` tests (they failed when a second ticked between reading the clock and writing the marker).
- [x] Manual checks M12-10 to M12-14 logged; M12-1 updated for 27.1.

```text
Commands run and results (Windows 11, after all merges, HEAD before this note):
  pnpm check (full): all steps passed. protocol 306, kits 84, SDK 134, desktop 1233, CLI 61
    TS tests; 41 cargo test suites ok, 0 failed (includes migrations 13-15 upgrade and
    rollback, marketplace suite on both Git backends); bundled activities validate and
    simulate; npm package checks; desktop build.
  Windows sandbox proof (debug build, run-denial-tests.mjs --attention): 59/59, 0 known gaps.
  Agents also ran: actionlint on both workflows; the npm tarballs installed outside the repo
    with create/validate/simulate/pack (Node 24); corepack accepting the pnpm hash.
Not run: macOS and Linux sandbox proof; Node 22; the publish workflow itself; real agents on
  Windows paths with В/Ñ/Ó; live Companion providers. All are in M12-10 to M12-14.
Open for the owner: decisions 30 (PRD and CLAUDE.md wording after 27.5) and 31 (npm package
  versions); the 29 assumption that the published CLI supports Node 22.12+; register the
  `lullshift` npm organization before the repo and docs are public.
Remaining risks: macOS/Linux sandbox unverified (third-party code warns "not verified");
  the Windows IPC barrier relies on wry behaviour, re-run the proof after upgrades; the
  built-in Git can't see a proxy set only in Windows settings (installed Git can't either);
  a compromised build tool could still alter what is packed (caught only by the checks).
```
