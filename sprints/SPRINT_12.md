# Sprint 12 - Owner review changes

**Goal:** Build the changes the product owner asked for when reviewing `DECISIONS_FOR_REVIEW.md` on 2026-11-06, before the first release (0.1.0).

**Depends on:** Sprints 00-11. Sprint 11's owner steps (the workflow run, manual checks and release approval) stay open. The release follows this sprint.

## Work items

Agents (decisions 1, 22, 23):

- [ ] S12-T01: OpenCode adapter. Research OpenCode's documented hook or plugin API, then build an observe-only adapter like the Codex and Cursor ones. Include consent, a backup, idempotent install, clean Disconnect, uninstall cleanup, a row in the capability matrix and a security review.
- [ ] S12-T02: Cursor mapping. `sessionEnd` with reason `error` maps to `failed`, and the error message is never read. Register `postToolUseFailure`, `subagentStop` and `preCompact` as `working` signals.
- [ ] S12-T03: Hook command paths are escaped for each shell (PowerShell, POSIX sh) instead of being limited to an allowlist. Control characters still refuse install. Add property tests and a security review.
- [ ] S12-T04: Extend the task-text opt-in (off by default, stored locally) to Codex, Cursor and OpenCode prompts. Needs a privacy review.

Release and uninstall (decision 26):

- [ ] S12-T05: AppImage hooks point at `$APPIMAGE`, so they survive restarts. A reinstall over an existing Windows install keeps the agent hooks.
- [ ] S12-T06: Record the update-check choice in ADR-018: once the updater exists, an opt-in setting (off by default) checks once a day, and the manual button stays. No updater code is added until the signing key exists.

Desktop UI (decisions 2, 25):

- [ ] S12-T07: The welcome offers "Connect an agent" (opens Integrations), and hides it when every supported agent is already connected. Add a "Skip to main content" link and `aria-describedby` on consent dialogs. Show activity-supplied paused-view stats in their own group, headed by the activity's name.

Games and content (decisions 6, 8, 14, 24):

- [ ] S12-T08: Rename Tea Sort to "Teacup Tidy". Thread Picture, Brook Links and Island Bridges get a "Size" button in the main controls row.
- [ ] S12-T09: Game changes:
  - Port Lantern Pond onto a kit.
  - Rebuild Tide Pairs on the card kit's table binding.
  - Ridge Walk: a King wraps to an Ace and an Ace to a King, with no winnable-deal check.
  - Fog Garden: three sizes and a safe first reveal.
  - Moss Paths: 4x4, 6x6 and 8x8, and crossing tiles turn.
  - Firefly Catch: rounds of 60, 90 or 180 seconds, or untimed.
  - Kite Drift: a Pace button (slow, normal, brisk).
- [ ] S12-T10: Listening steps can be done by hearing or by touch and sight. Ban U+2028 and U+2029 in every engine schema in one pass.

Packs and the pack checker (decisions 9, 13):

- [ ] S12-T11: Packs may include checked PNG/WebP images, with size and count limits and decode checks. Amend ADR-016 and get a security review.
- [ ] S12-T12: Replace the custom Rust pack-data checker with an established JSON Schema crate. Audit the dependency and run the shared Ajv/Rust corpus. The CSP stays as it is.

Sandbox and marketplaces (decisions 15, 17, 18):

- [ ] S12-T13: Close the WebRTC gap in the activity sandbox on Windows, macOS and Linux. Third-party code activities stay blocked until the sandbox proof passes on each OS.
- [ ] S12-T14: Marketplace changes, recorded in ADR-015, with a security review:
  - Private hosts are allowed after an explicit "on your own network" confirmation, and a resolved-address check catches public names that point at private addresses.
  - Same-host redirects are followed.
  - An explicit "change address" flow.
  - Installed files are re-checked on each load.
  - Before deleting saved progress that belongs to another source, the user is asked.
  - The schema, SDK and CLI hidden-character checks are aligned with the host.

Companion (decisions 19, 20):

- [ ] S12-T15: Add streaming replies, with each chunk cleaned and budgets counted for partial replies. Remember the last context choice; the preview is still shown before every send. Cancel frees the thread at once, discards a late reply and still counts its usage.
- [ ] S12-T16: Add three read-only Companion tools, each previewed and approved per call: Session overview, Suggest a break (a card the user clicks) and Activity help lookup. Amend ADR-020 and get a security review.

## Acceptance criteria

- [ ] S12-A01: Each change above has tests. `pnpm check` passes in full.
- [ ] S12-A02: Security reviews for T01, T03, T11, T13, T14 and T16 have no open high or medium findings.
- [ ] S12-A03: ADR-015, ADR-016, ADR-018 and ADR-020 are amended, and the capability matrix and user and creator docs are updated.
- [ ] S12-A04: New manual checks are logged in `docs/MANUAL_TESTS.md` as M12-x.

## Guardrails

- No accounts, hosted services or default telemetry. Network use stays user-initiated.
- Companion tools never act on agents, files, the shell or the web.
- Never weaken the sandbox or the Git hardening to make content work.
- Hooks stay observe-only, with explicit consent, backups, idempotent install and clean uninstall.

## Closeout

```text
Sprint: 12 (not started)
```
