# Sprint 05 - Activity SDK and Creator CLI

**Goal:** Creators can build, simulate, validate and pack activities locally with the published SDK.

**Depends on:** Sprint 03.

## Work items

- [x] S05-T01: Versioned TypeScript `activity-sdk` (lifecycle, storage, reduced-motion and theme signals)
- [x] S05-T02: Manifest v1 with `kind: app | content`; engine reference and data-schema validation for content packs
- [x] S05-T03: CLI `create`, `dev`, `simulate`, `validate`, `pack` (deterministic `.lullpack` for sideloading); `marketplace init` scaffolds an index
- [x] S05-T04: Event-sequence simulator (needs_attention, duplicates, out-of-order, forced pause, close, crash, resume, offline)
- [x] S05-T05: Update `/activity-creator` skill and `creator/` prompts; starter templates for custom, kit-based and content activities (Note: templates cover custom code and content packs; the kit-based template follows the kits in Sprint 06.)
- [x] S05-T06: Tighten manifest strings (Sprint 00 security review): segment-based `data` path pattern (no `.`, `..`, empty or `.git` segments, no leading `-`), `maxLength`; patterns and `maxLength` for `minHostVersion`/`sdkVersion`; reject bidi/control characters in `name`/`description`/`author`. Share one Ajv factory between the runtime validator and the fixture script

- [x] S05-T07 (product owner, 2026-11-05): how-to-play help for every activity. The manifest gets an optional structured `help` section (plain text only: summary, goal, how to play, controls by key, tips), and the host renders it in a "How to play" panel. The panel is reachable from the activity card, the running activity's title bar and the paused view. Bundled activities ship full help; `validate` warns when help is missing. The help format is part of the Activity SDK (types, docs, starter templates) and of the AI/LLM creator material (`/activity-creator` skill and `creator/` prompts), so generated activities always include how-to-play help
- [x] S05-T08 (product owner, 2026-11-05): activity layout polish. Lantern Pond's controls sit on one row; add layout guidance to the SDK docs and starter templates (controls in one row when they fit, no stray wrapping)
- [x] S05-T09 (product owner, 2026-11-05): useful paused view instead of an empty one. The host shows time played (tracked by the host), stats the activity reports through the bridge (an optional, validated `summary` of a few label/value pairs, such as moves, best or lit), and the activity's preview image from an optional manifest `preview` file when there are no stats. Activity cards show the preview too
- [x] S05-T10 (product owner, 2026-11-05): disconnecting a tool ends tracking of its open sessions at once. Its Working and Needs you sessions leave the live columns; History keeps their last state, marked "Not tracked: disconnected". A status line says how many sessions stopped being tracked. New events after a reconnect bring a session back. The mascot and notifications ignore disconnected sessions
- [x] S05-T11 (product owner, 2026-11-05): session cards show the task name, as Claude Code does. The title comes from Claude Code's transcript file, read with fallbacks in this order: a name the user gives the session, then the agent's own title, then the first line of the captured intent, then tool name plus short id. Reading the title is opt-in under the capture consent and amends ADR-013. Only title lines are read, from a bounded tail of the file named by the hook's `transcript_path`. The forwarder reads it, not the app, and only the title is sent. The title is cut to 120 characters and redacted. A missing file, an unknown format or a parse error means falling back silently, and fixture tests cover those cases. The user can rename a session at any time

## Acceptance criteria

- [x] S05-A01: Sample code activity and sample content pack pass `validate` and `simulate`
- [x] S05-A02: Invalid manifests, unknown permissions and code inside content packs are rejected
- [x] S05-A03: Identical input produces an identical `.lullpack` hash
- [x] S05-A04: An activity generated from the creator prompt passes without host changes
- [x] S05-A05: Every bundled activity has help reachable by keyboard from its card, while running and while paused; the paused view shows time played plus stats or a preview, never an empty card
- [x] S05-A06: After a disconnect no session from that tool shows as Working or Needs you; cards show the agent's title when there is one, and the fallbacks hold with a missing, empty or unknown-format transcript

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 05
Completed tasks (commits): T01-T11 and A01-A06; uncommitted at closeout, commits pending approval.
Commands run and results:
  pnpm check (full): secret scan, schema fixtures, generated types, typecheck, lint, format,
    unit tests (ts), bundled activities (validate, simulate), cargo fmt, clippy, test, desktop
    build: all steps passed (re-run after the security fixes).
  SDK 18 tests, CLI 32 tests (incl. simulator negative cases, server hardening), desktop TS
    tests, Rust workspace tests: pass.
  pnpm tauri build --debug --no-bundle, then the app driven over CDP: cards show previews and
    How to play; help dialog renders; Lantern Pond controls on one row; paused view shows
    played time plus Lanterns lit and Moves; help fallback for an activity without help.
  A04: Tide Sort built by an agent from creator docs only; validate and all 15 simulate
    scenarios pass after one manifest fix (category). Docs now list the categories, --id and
    the bridge copy; enum errors list the allowed values.
Tests not run and why:
  Disconnect and transcript titles against real Claude Code hooks: would change the user's
    hook settings; covered by unit/integration tests, manual check M05-2.
  macOS/Linux: simulator and the title reader's Unix branch not run (Windows only); M05-3.
  Screen reader pass of help and the paused view: M05-1.
Security/privacy review: security-qa-reviewer on the whole Sprint 05 diff. No critical or high.
  Fixed: M1 browser left running when CDP setup failed; M2 served/packed files re-checked at
  read time (link swap, size); L1 title reader refuses UNC/device paths and opens without
  following links or blocking; L2 dev servers check the Host header and survive bad escapes;
  L3 activity text stripped of control characters before printing; L5 the store rejects bidi
  and invisible characters in titles (shared check in agent-core).
Remaining defects and risks:
  L4 (low): an activity summary label can mimic a host label such as "Played"; spoofing only.
  activity_preview reads compiled-in files today; when Sprint 07 reads installed packs from
    disk it needs the same no-follow and re-check hardening as the CLI.
  Claude Code's transcript format is internal; titles fall back silently if it changes.
  The test run cleared the user's saved Step Away state ("Start over" pressed by a script).
Acceptance checks passed/failed: A01-A04 passed; A05 passed in the app except the screen
  reader step (M05-1); A06 passed in tests, real-hook check pending (M05-2).
Recommended next action: commit Sprint 05 in logical commits; then Sprint 06 (kits and games).
```
