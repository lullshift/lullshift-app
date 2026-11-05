# Universal Lullshift Activity Generator Prompt

Use this prompt with Claude Code, Codex, Cursor or another coding LLM. The official Claude Code skill is `.claude/skills/activity-creator/SKILL.md`. Give the tool `packages/activity-sdk/README.md` and `reference/ACTIVITY_MANIFEST_V1.schema.json`.

> You are building a third-party **Lullshift activity**: a small static web page that runs in a sandbox. Follow `packages/activity-sdk/README.md` and `reference/ACTIVITY_MANIFEST_V1.schema.json`.
>
> Activity idea: [DESCRIBE GAME/ACTIVITY].
>
> Start with `pnpm lullshift create <folder> --name "<Name>" --author "<You>" --id <reverse.dns.id>`, then change the files. `create` copies the bridge for you. For a grid, card or real-time game add `--kit grid|card|arcade`: it copies a kit (`window.LullshiftKits.<kit>`, see `packages/activity-kits/README.md`) with seeded puzzles, undo, keyboard navigation and pause handling. Without `--id` the id is `dev.local.<name>`. `category` is one of `arcade`, `break`, `productivity` or `interactive` (games are usually `arcade`).
>
> Rules:
>
> - Plain HTML, CSS and JavaScript. Classic scripts only. No modules, no inline script or style, no CDN, no network, no localStorage, clipboard, file access or Tauri APIs. Everything works offline.
> - Rounds are short and interruption-friendly. Keep all progress in one JSON state (at most 256 KiB). Call `LullshiftBridge.autosave(state)` after every meaningful change. Validate restored state in `onInit` and start fresh if it looks wrong.
> - Keyboard play, visible focus, reduced motion, light and dark theme.
> - Declare only `activity.storage`, and only if you use it.
> - **Help is mandatory.** Fill `help` in `manifest.json`: `summary`, `goal`, `howToPlay` steps, `controls` by key, `tips`. Plain text only. Keep it in sync with play. The `validate` warning "no help" is a failure: fix it.
> - **Layout.** Controls on one row when they fit; buttons never break mid-label; wrap only on very narrow windows. Use the `.controls` CSS from the template.
> - **Paused view.** After each meaningful change call `LullshiftBridge.summary([...])` with 1 to 4 stats (label and value at most 24 characters, or numbers), such as moves, best, progress. Provide a `preview` image (png, jpg, jpeg, webp or svg, at most 256 KiB) in original art. The host shows time played itself.
> - Original names and art. Do not copy a branded game. List every third-party asset and its license.
>
> Then run `pnpm lullshift validate <folder>` and `pnpm lullshift simulate <folder>`. Fix all errors and warnings. Report the real output. If a command could not run, say so as a blocker. Never claim the activity passes unless both ran and passed. Do not publish or push without explicit user consent.

## Minimal output checklist

- [ ] `manifest.json` with `help` and `preview`
- [ ] `index.html`
- [ ] `style.css`
- [ ] `main.js`
- [ ] `lullshift-bridge-v1.js` (`create` copies it; never edit it)
- [ ] `lullshift-<kit>-kit-v1.js` if you used `--kit` (never edit it)
- [ ] a preview image (original art)
- [ ] README or `LICENSES.md` with asset sources and tests run
- [ ] `pnpm lullshift validate <folder>` output, with no errors and no warnings
- [ ] `pnpm lullshift simulate <folder>` output, all scenarios passed

No `src/` folder or build step is needed. Make the `.lullpack` with `pnpm lullshift pack <folder>` only after both checks pass.

## Suggested ideas

- Puzzle: an original grid or logic game; no protected branding.
- Break: a calm timed breathing or stretching prompt.
- Decision card: a simple choice canvas with local state only.
- Content pack: cards for an existing engine (`activities/engines/calm-cards`).
