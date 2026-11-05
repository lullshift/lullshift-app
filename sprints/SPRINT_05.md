# Sprint 05 - Activity SDK and Creator CLI

**Goal:** Creators can build, simulate, validate and pack activities locally with the published SDK.

**Depends on:** Sprint 03.

## Work items

- [ ] S05-T01: Versioned TypeScript `activity-sdk` (lifecycle, storage, reduced-motion and theme signals)
- [ ] S05-T02: Manifest v1 with `kind: app | content`; engine reference and data-schema validation for content packs
- [ ] S05-T03: CLI `create`, `dev`, `simulate`, `validate`, `pack` (deterministic `.lullpack` for sideloading); `marketplace init` scaffolds an index
- [ ] S05-T04: Event-sequence simulator (needs_attention, duplicates, out-of-order, forced pause, close, crash, resume, offline)
- [ ] S05-T05: Update `/activity-creator` skill and `creator/` prompts; starter templates for custom, kit-based and content activities
- [ ] S05-T06: Tighten manifest strings (Sprint 00 security review): segment-based `data` path pattern (no `.`, `..`, empty or `.git` segments, no leading `-`), `maxLength`; patterns and `maxLength` for `minHostVersion`/`sdkVersion`; reject bidi/control characters in `name`/`description`/`author`. Share one Ajv factory between the runtime validator and the fixture script

- [ ] S05-T07 (product owner, 2026-11-05): how-to-play help for every activity. The manifest gets an optional structured `help` section (plain text only: summary, goal, how to play, controls by key, tips), and the host renders it in a "How to play" panel. The panel is reachable from the activity card, the running activity's title bar and the paused view. Bundled activities ship full help; `validate` warns when help is missing. The help format is part of the Activity SDK (types, docs, starter templates) and of the AI/LLM creator material (`/activity-creator` skill and `creator/` prompts), so generated activities always include how-to-play help
- [ ] S05-T08 (product owner, 2026-11-05): activity layout polish. Lantern Pond's controls sit on one row; add layout guidance to the SDK docs and starter templates (controls in one row when they fit, no stray wrapping)
- [ ] S05-T09 (product owner, 2026-11-05): useful paused view instead of an empty one. The host shows time played (tracked by the host), stats the activity reports through the bridge (an optional, validated `summary` of a few label/value pairs, such as moves, best or lit), and the activity's preview image from an optional manifest `preview` file when there are no stats. Activity cards show the preview too
- [ ] S05-T10 (product owner, 2026-11-05): disconnecting a tool ends tracking of its open sessions at once. Its Working and Needs you sessions leave the live columns; History keeps their last state, marked "Not tracked: disconnected". A status line says how many sessions stopped being tracked. New events after a reconnect bring a session back. The mascot and notifications ignore disconnected sessions
- [ ] S05-T11 (product owner, 2026-11-05): session cards show the task name, as Claude Code does. The title comes from Claude Code's transcript file, read with fallbacks in this order: a name the user gives the session, then the agent's own title, then the first line of the captured intent, then tool name plus short id. Reading the title is opt-in under the capture consent and amends ADR-013. Only title lines are read, from a bounded tail of the file named by the hook's `transcript_path`. The forwarder reads it, not the app, and only the title is sent. The title is cut to 120 characters and redacted. A missing file, an unknown format or a parse error means falling back silently, and fixture tests cover those cases. The user can rename a session at any time

## Acceptance criteria

- [ ] S05-A01: Sample code activity and sample content pack pass `validate` and `simulate`
- [ ] S05-A02: Invalid manifests, unknown permissions and code inside content packs are rejected
- [ ] S05-A03: Identical input produces an identical `.lullpack` hash
- [ ] S05-A04: An activity generated from the creator prompt passes without host changes
- [ ] S05-A05: Every bundled activity has help reachable by keyboard from its card, while running and while paused; the paused view shows time played plus stats or a preview, never an empty card
- [ ] S05-A06: After a disconnect no session from that tool shows as Working or Needs you; cards show the agent's title when there is one, and the fallbacks hold with a missing, empty or unknown-format transcript

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 05
Completed tasks (commits):
Commands run and results:
Tests not run and why:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next action:
```
