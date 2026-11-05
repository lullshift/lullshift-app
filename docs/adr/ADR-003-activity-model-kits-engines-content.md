# ADR-003: Activity model — code activities, kits, engines and content packs; v1 content target

- **Status:** Accepted
- **Date:** 2026-11-05
- **Owner:** Product owner
- **Supersedes:** Handoff v1.1 "two bundled games and one Break activity" scope

## Context

The product owner wants the first release to ship with about **20 games and 100 activities**. Building each as an independent app would multiply the hardest-to-test parts (pause/resume, state persistence, accessibility, reduced motion, interruption simulation). Independent creators must still be free to build anything within the sandbox.

## Decision

### Three ways to build (creator's choice)

| Tier | What the author ships | Runs code? |
|---|---|---|
| **Code activity (custom)** | Any static web app (HTML/CSS/bundled JS, any framework) implementing the SDK lifecycle | Yes, in sandbox |
| **Code activity on a kit** | Game rules built on an optional first-party kit npm package (grid, card, real-time arcade) | Yes, in sandbox |
| **Content pack** | Only a JSON data file (plus optional static media) for a named engine | **No** |

- Kits and engines are **optional conveniences**, never required.
- All code activities follow the same contract: the sandbox, the SDK lifecycle bridge (`initialize/start/pause/resume/saveState/restoreState/dispose`), a valid manifest and declared asset licenses. The host enforces pause/freeze on `needs_attention` without relying on the activity's cooperation.

### Definitions

- **Kit:** a first-party TypeScript library that game code is built on (e.g. grid kit: cells, selection, keyboard navigation, undo, seeded generators; card kit: deck, moves, undo; arcade kit: game loop, forced pause, resume countdown). Kits are compiled into the activity bundle.
- **Engine:** a first-party code activity that renders a family of experiences from data (e.g. breathing, movement routine, quiz, brain teaser, guided pause, reflection prompts, creative toy, typing drills, away prompts). An engine is versioned and referenced by `id@major`.
- **Content pack:** a manifest of `kind: "content"` that names an engine and supplies data validated against that engine's data schema. Content packs contain no executable code and therefore carry a much lower install risk.

### Manifest change

`reference/ACTIVITY_MANIFEST_V1.schema.json` gains `kind: "app" | "content"`. `app` requires `entrypoint` and `runtime`; `content` requires `engine` and `data` and forbids executable files.

### v1 bundled content target

- **20 games**, original names and art on generic mechanics; puzzle/turn-based games prioritised (interruption-friendly), real-time games use forced pause + resume countdown.
- **~100 activities**, mostly content packs on ~8–9 engines (breathing, movement, guided pause, quiz, brain teaser, reflection, creative toy, typing, away prompts).
- All bundled items live in this repo under `activities/` and are listed in the built-in marketplace index (ADR-002).
- **The numbers are a release target, not a release blocker.** Core agent awareness and the activity platform gate the release; unfinished content ships in a later app release.
- Quizzes and teasers are **play, not curriculum** (no courses, progress tracks or certification — "Academy" stays out of scope).
- No recreation of specific existing branded games (names, characters, art, level designs, look).

### Not SDK activities

Context Keeper, Next Move and Companion need task context and/or AI access, so they are **host features**, not sandboxed activities.

## Consequences

- Test surface scales with kits + engines (~12) instead of with item count (~120).
- Content creation (including AI-assisted) becomes mostly JSON authoring, reviewed by schema validation.
- Engine data schemas become public, versioned contracts; breaking changes require a new engine major version and the old one must keep running installed packs.
