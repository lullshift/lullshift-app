---
name: activity-creator
description: Create or revise Lullshift games and activities (custom code, code on a kit, or a code-free content pack for an engine) from natural-language requests using the manifest, SDK, simulator and CLI. Every activity needs help, a paused-view summary, a preview image and a one-row controls layout. Use for any AI-generated Lullshift activity.
license: Apache-2.0
compatibility: Needs a clone of github.com/lullshift/lullshift-app with pnpm installed, and a shell to run the lullshift CLI. Works with Claude Code, Codex, Cursor and OpenCode.
---

# Lullshift Activity Creator

This skill is shared by every supported AI tool. Claude Code finds it through `.claude/skills/activity-creator`; Codex, Cursor and OpenCode read it from `.agents/skills/activity-creator`.

## 0. Find the toolchain

The `lullshift` CLI is not published to npm; it runs from a clone of `lullshift-app`.

- If the current repository contains `tools/lullshift-cli`, you are in the clone: run commands from its root as `pnpm lullshift <command>`.
- Otherwise (for example, in the creator's own marketplace repository), ask for the path to their clone and run `pnpm --dir <clone> lullshift <command>`. Folder arguments resolve from where you run the command. Read the reference files below from the clone.
- If there is no clone or `pnpm install` has not been run, say so and stop: you can't create or check an activity without the CLI.

## 1. Read first

`packages/activity-sdk/README.md`, `reference/ACTIVITY_MANIFEST_V1.schema.json`, `packages/activity-kits/README.md` (for kits), ADR-003 and ADR-012 in `docs/adr/`, and `creator/ACTIVITY_CREATOR_PROMPT.md`. For content packs also read the engine's `data.schema.json` and `docs/CONTENT_STYLE_GUIDE.md`.

## 2. Choose the lightest tier

A **content pack** if an existing engine in `activities/engines/` can express it (preferred), else **code on a kit** (grid, card, arcade) if one fits, else **custom code**. Ask only for missing essential behaviour.

## 3. Scaffold with the real CLI

- `pnpm lullshift create <folder> --name "Name" --author "You" --id <reverse.dns.id>`; add `--kind content --engine <id>@<major>` for a pack. Without `--id` the id is `dev.local.<name>`.
- `category` is one of `arcade`, `break`, `productivity` or `interactive` (games are usually `arcade`).
- Never hand-write `lullshift-bridge-v1.js`; `create` copies it from `packages/activity-sdk/bridge/`.
- For a grid, card or real-time game add `--kit grid|card|arcade`. `create` copies `lullshift-<kit>-kit-v1.js` and loads it after the bridge. Never edit the kit copy.

## 4. Rules

- **Code activities:** static HTML, CSS and plain JavaScript as classic scripts. No inline script or style, no modules, no CDN, no network, no storage APIs, clipboard or Tauri IPC. Inventory and licence-check every asset.
- **Content packs:** `kind: "content"`, `engine`, and a `data` JSON file valid against the engine's `data.schema.json`. No code, and no files beyond `manifest.json`, the data file, a preview image and `.md`/`.txt` notes (ADR-016). Validate failures name the file and data path (`deck.json: cards[3].front is longer than 200 characters`); fix each one.
- **Originality:** original names and art. Generic mechanics are fine; never recreate a specific branded game (its name, characters, art, levels or look).

## 5. Always meet these three requirements

- **Help is mandatory.** Fill `help` in `manifest.json`: `summary`, `goal`, `howToPlay` steps, `controls` by key, `tips`. Plain text only, in sync with play. The `validate` warning "no help" is a failure.
- **Layout.** Controls sit on one row when they fit; buttons never break mid-label (`white-space: nowrap`); wrap only on very narrow windows. Use the `.controls` CSS from the template.
- **Paused view.** Call `LullshiftBridge.summary([...])` with 1 to 4 stats (label and value at most 24 characters, or numbers) after each meaningful change. Add a `preview` image (original art, at most 256 KiB). The host shows time played itself.

## 6. Saving and access

`LullshiftBridge.autosave(state)` after each meaningful change, `saveState()`, and a validated restore in `onInit`. All progress goes in one JSON state of at most 256 KiB. Support keyboard play, visible focus, reduced motion and light and dark themes.

## 7. Run the real checks

- `pnpm lullshift validate <folder>` (packs: add `--engines <engine folder>` if the engine isn't bundled)
- `pnpm lullshift simulate <folder>`
- `pnpm lullshift pack <folder>` only after both pass.

Fix every error and every warning, then run them again. **Never claim success if validate or simulate were not run, or if either failed.** If the toolchain or a browser is missing, report it as a blocker. Report the real output, not what you expect it to say.

## 8. Finish

- Document tests and the source of every asset in the activity's README or `LICENSES.md`.
- Publishing means pushing the folder to a Git repository with a `.lullshift/marketplace.json` entry; use the `marketplace-publishing` skill. Never push, create repositories or open pull requests without the user's explicit confirmation.

**Security rule:** generated code is untrusted, whichever tool wrote it. Never work around sandbox restrictions or ask for broader permissions than needed. If something needs a privileged API, stop and explain why instead of building a workaround.
