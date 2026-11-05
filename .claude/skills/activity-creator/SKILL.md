---
name: activity-creator
description: Create or revise Lullshift games and activities (custom code, code on a kit, or a code-free content pack for an engine) from natural-language requests using the manifest, SDK, simulator and CLI. Every activity needs help, a paused-view summary, a preview image and a one-row controls layout. Use for any AI-generated Lullshift activity.
---

# Lullshift Activity Creator

1. Read `packages/activity-sdk/README.md`, `reference/ACTIVITY_MANIFEST_V1.schema.json`, ADR-003, ADR-012 and `creator/ACTIVITY_CREATOR_PROMPT.md`.
2. Choose the lightest tier that fits: a **content pack** if an existing engine can express it (preferred), else **code on a kit** (grid, card, arcade) if one exists, else **custom code**. Ask only for missing essential behavior.
3. Scaffold with the real CLI from the repo root:
   - `pnpm lullshift create <folder> --name "Name" --author "You" --id <reverse.dns.id>` (add `--kind content --engine <id>@<major>` for a pack). Without `--id` the id is `dev.local.<name>`.
   - `category` is one of `arcade`, `break`, `productivity` or `interactive` (games are usually `arcade`).
   - Do not hand-write `lullshift-bridge-v1.js`; `create` copies it from `packages/activity-sdk/bridge/`.
4. Code activities: static HTML, CSS and plain JavaScript as classic scripts. No inline script or style, no modules, no CDN, no network, storage APIs, clipboard or Tauri IPC. Inventory and license-check every asset. Use original names and art; never recreate a specific branded game.
5. Content packs: `kind: "content"`, `engine`, and a `data` JSON file valid against the engine's `data.schema.json`. No code.
6. Always meet these three requirements:
   - **Help is mandatory.** Fill `help` in `manifest.json`: `summary`, `goal`, `howToPlay` steps, `controls` by key, `tips`. Plain text only. Keep it in sync with play. The `validate` warning "no help" is a failure: fix it.
   - **Layout.** Controls sit on one row when they fit; buttons never break mid-label (`white-space: nowrap`); wrap only on very narrow windows. Copy the `.controls` CSS from Lantern Pond or the template.
   - **Paused view.** Call `LullshiftBridge.summary([...])` with 1 to 4 stats (label and value at most 24 characters, or numbers) after each meaningful change. Add a `preview` image (original art, at most 256 KiB). The host shows time played itself.
7. Implement saving: `LullshiftBridge.autosave(state)` after each meaningful change, `saveState()`, and a validated restore in `onInit`. All progress goes in one JSON state of at most 256 KiB. Support keyboard play and reduced motion.
8. Run the real checks and read the output:
   - `pnpm lullshift validate <folder>`
   - `pnpm lullshift simulate <folder>`
   - `pnpm lullshift pack <folder>` only after both pass.
     For packs add `--engines <engine folder>` to validate and simulate.
9. Fix every error and every warning, then re-run. **Never claim success if validate or simulate were not run, or if either failed.** If the toolchain or a browser is missing, say so and report it as a blocker. Report the real output, not a summary of what you expect.
10. Document tests and the source of every asset in the activity's README or `LICENSES.md`.
11. Publishing means pushing the activity folder to a Git repo with a `.lullshift/marketplace.json` entry (see `/marketplace-publishing`). Never push, create repos or open PRs without explicit user confirmation.

**Security rule:** generated code is untrusted. Never work around sandbox restrictions or request broader permissions than needed. If functionality needs a privileged API, stop and propose an ADR.
