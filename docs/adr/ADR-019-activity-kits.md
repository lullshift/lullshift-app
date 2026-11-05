# ADR-019: Activity kits: source, distribution and scope

- **Status:** Accepted (Sprint 06), pending product-owner review (see `sprints/DECISIONS_FOR_REVIEW.md`)
- **Date:** 2026-11-05
- **Owner:** Implementation lead
- **Related:** ADR-003 (activity model), ADR-012 (sandbox and bridge), ADR-017 (CLI and SDK packaging), TECHNICAL_ARCHITECTURE §6.5

## Context

ADR-003 defines kits as first-party TypeScript libraries "compiled into the activity bundle": grid (cells, selection, keyboard navigation, undo, seeded generators, daily seed), card (deck, moves, keyboard play, undo) and arcade (game loop, forced pause, resume countdown, reduced motion). Sprint 06 builds them and at least eight games on them.

Two constraints shape how they ship:

- Bundled activities and the creator templates use **no build step** (ADR-017): plain files the host serves as they are.
- The activity sandbox **blocks module scripts and inline script** (ADR-012). An activity can only load classic scripts from its own folder.

## Decision

### One TypeScript source, one generated classic script per kit

- Kits live in `packages/activity-kits/{core,grid,card,arcade}` as erasable-syntax TypeScript ES modules. Tests import these modules directly (Vitest on Node).
- A deterministic generator, `packages/activity-kits/scripts/build-kits.ts` (Node type stripping, no bundler dependency), turns each kit into **one classic script**, `dist/lullshift-<kit>-kit-v1.js`. That file inlines the small shared `core` (seeded random numbers, undo history, key mapping) and attaches the kit to `window.LullshiftKits.<kit>`. The generated files are committed. `pnpm check` fails when they are out of date, the same way it does for generated types.
- The source rules that keep the generator simple and safe: no imports except from `core`, top-level `export function|const|class` only, no `eval`, no network, storage, clipboard or Tauri APIs. Kits never touch persistence directly; games save through the bridge.

### How activities use a kit

- An activity copies the kit file into its folder and loads it after the bridge and before its own scripts. `lullshift create --kit grid|card|arcade` does the copy. `lullshift validate` warns when a kit file differs from the SDK's copy, as it does for the bridge.
- Creators who use a bundler may import the TypeScript modules instead. The host never loads kit code itself and never trusts an activity because it uses a kit: a kit is ordinary code inside the sandbox.
- The kit file's major version is in its name (`-v1`). A breaking change ships as `-v2` beside it; activities keep the copy they were built with.

### Scope of each kit

- **core:** seeded PRNG and daily seed (from the local date), undo/redo history with a size cap, a key-to-action map that respects common keyboard layouts.
- **grid:** a rectangular grid model, cursor and selection, arrow/Home/End/PageUp/PageDown navigation with wrapping options, roving-tabindex DOM binding with labelled cells, seeded generators (shuffle, fill), snapshot and restore of plain JSON state.
- **card:** deck building, seeded shuffle, piles, move validation hooks, focus moving between piles and cards by keyboard, undo, snapshot and restore.
- **arcade:** a fixed-step game loop on `requestAnimationFrame`, forced pause (the loop stops and time does not advance while paused or hidden), a resume countdown (skipped to a static "Ready" prompt in reduced-motion mode), and a reduced-motion flag games must honour.

No kit adds manifest fields or permissions; the manifest schema is unchanged.

## Consequences

- One tested implementation of the hard parts (keyboard navigation, undo, seeded generation, pause and resume) per kit, shared by every game on it.
- Committed generated files must be regenerated after any kit change; the check catches drift.
- DOM behaviour (focus, roving tabindex) is tested through the simulator on real games, because the workspace has no DOM test library. Kit logic is unit-tested on Node.
