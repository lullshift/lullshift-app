# Lullshift Activity SDK

Everything you need to build a Lullshift activity: the bridge client, the host protocol types, and the manifest, help and summary formats. Version 0.2.0. This guide is for people and for AI tools.

Generated code is untrusted. Follow the sandbox rules below. Never ask for more permission than the activity needs.

## Where it lives

The SDK is in this repository for now. Publishing to npm waits for approval, so do not run `npm install @lullshift/activity-sdk`.

| Path | What it is |
|---|---|
| `packages/activity-sdk/bridge/lullshift-bridge-v1.js` | The bridge client. Copy it into your activity folder. |
| `packages/activity-sdk/bridge/lullshift-bridge-v1.d.ts` | Types for the bridge, if you write TypeScript. |
| `packages/activity-sdk/src/` | Manifest, help and summary types and the host protocol. |
| `reference/ACTIVITY_MANIFEST_V1.schema.json` | The manifest schema. |

`pnpm lullshift create` copies the bridge for you.

Games can also use a kit from `packages/activity-kits` (grid, card or arcade): seeded puzzles, undo, keyboard navigation, and pause and resume done once and tested. Add `--kit grid|card|arcade` to `create`. See the [kits README](../activity-kits/README.md).

## Quick start

```bash
pnpm lullshift create my-activity --name "My Activity" --author "You" --id dev.you.my-activity
pnpm lullshift dev my-activity
pnpm lullshift validate my-activity
pnpm lullshift simulate my-activity
pnpm lullshift pack my-activity
```

`create` writes a small working activity. `dev` opens it in a host-like window. `validate` checks files and the manifest. `simulate` runs the interruption scenarios in a browser already on your computer (Edge, Chrome or Chromium; use `--browser <path>` or `LULLSHIFT_BROWSER` to pick one). `pack` writes a `.lullpack` file after validation passes. Run `pnpm lullshift --help` for all options.

In `manifest.json`, `category` is one of `arcade`, `break`, `productivity` or `interactive` (games are usually `arcade`). Without `--id`, `create` uses `dev.local.<name>`; use your own reverse-DNS id before you publish.

Exit codes: 0 ok, 1 problems found, 2 usage or environment error.

An activity folder needs these files. No build step is required.

- `manifest.json`
- `index.html`
- `style.css`
- `main.js`
- `lullshift-bridge-v1.js` (an identical copy from the SDK)
- `lullshift-<kit>-kit-v1.js` if you use a kit (an identical copy, loaded after the bridge)
- a preview image

## The bridge

Load it as a classic script, before your own code:

```html
<script src="lullshift-bridge-v1.js"></script>
<script src="main.js"></script>
```

It adds one global, `LullshiftBridge`.

```js
LullshiftBridge.start({
  onInit({ restore, reducedMotion, theme, content }) {
    // restore: the last saved state, or null on a fresh start
    // reducedMotion: true if the player asked for less motion
    // theme: "light" or "dark"
    // content: pack data for an engine, otherwise null
  },
  saveState() {
    return { /* any JSON value except null */ };
  },
  onPause() {}, // optional hint
  onResume() {}, // optional hint
});
```

- `start(handlers)` tells the host you are ready. Call it once. The host then sends `init`.
- `onInit` runs once. Build your screen from `restore` if it is not null.
- `saveState()` is called when the host asks. Return your whole state.
- `autosave(state)` sends your state to the host now. Call it after every meaningful change. The host can unload the page at any moment, with or without a reply, so unsaved progress is lost.
- `summary(items)` sets the stats shown while the activity is paused (see below).
- `storage.get(key)`, `storage.set(key, value)` and `storage.remove(key)` return promises. They need the `activity.storage` permission. Values are strings. Prefer the single state object; use storage only for small things that outlive a reset, such as a best score.

The host owns pause and resume. It stops your page when an agent needs the user. `onPause` and `onResume` are hints only. Do not rely on them to stay correct.

## Sandbox rules

The activity runs in an isolated frame (ADR-012). These rules are enforced, and `validate` warns when it sees code that breaks them.

- Classic scripts only. No `type="module"`, no `import`.
- No inline script and no inline style. Put code in `main.js` and styles in `style.css`. No `style="..."` attributes.
- No network: no `fetch`, XHR, WebSocket, remote fonts, images or CDN scripts. Everything ships in the folder.
- No `localStorage`, cookies, IndexedDB, clipboard, file access, navigation, popups or Tauri APIs.
- Keyboard access for everything. Visible focus. Text labels on controls.
- Honour `reducedMotion`: no animation or transitions when it is true.
- Honour `theme`. Use CSS variables so light and dark both read well.
- Works offline.

If something seems to need a privileged API, stop. Do not work around the sandbox.

## Manifest

Start from `tools/lullshift-cli/templates/app/manifest.json`. The schema is `reference/ACTIVITY_MANIFEST_V1.schema.json`. Declare only the permissions you use; today that is `activity.storage` or none.

## Help (required)

Every activity needs a `help` section in `manifest.json`. `validate` warns when it is missing. Treat that warning as a failure and fix it before you call the activity done.

Help is plain text only: no HTML, no Markdown. Keep it in sync with how the activity actually plays. If you change the controls, change the help.

Fields:

- `summary`: one or two sentences about what this is.
- `goal`: what the player is trying to do.
- `howToPlay`: short steps, in order.
- `controls`: a list of `{ "keys": "...", "action": "..." }`.
- `tips`: optional short hints.

Complete example:

```json
"help": {
  "summary": "Stack pebbles until the cairn is ten high.",
  "goal": "Ten pebbles on the cairn.",
  "howToPlay": [
    "Press Add a pebble to place one on top.",
    "Press Take one off if you change your mind.",
    "At ten, the cairn is done. Start again whenever you like."
  ],
  "controls": [
    { "keys": "Tab", "action": "Move between the buttons" },
    { "keys": "Enter / Space", "action": "Press the focused button" },
    { "keys": "+ / -", "action": "Add or take off a pebble" }
  ],
  "tips": ["You can stop at any time. Your cairn is saved."]
}
```

## Paused view: summary and preview

While an activity is paused, the host shows its latest summary. The host shows time played by itself; do not report it.

Call `LullshiftBridge.summary([...])` after each meaningful change:

```js
LullshiftBridge.summary([
  { label: "Moves", value: 12 },
  { label: "Best", value: 9 },
  { label: "Progress", value: "4 of 10" },
]);
```

Rules: one to four items. Each `label` is plain text of at most 24 characters. Each `value` is plain text of at most 24 characters, or a finite number.

Also add a `preview` image in `manifest.json`, for example `"preview": "preview.svg"`. Formats: png, jpg, jpeg, webp or svg, at most 256 KiB. Use original art. Copying a branded game's look is not allowed. `validate` warns when it is missing.

## Layout

Keep controls on one row when they fit. Buttons must never break in the middle of a label. Wrap only on very narrow windows. This is the pattern used by Lantern Pond and the template:

```css
.controls {
  display: flex;
  flex-wrap: nowrap;
  gap: 8px;
}
.controls button {
  flex: 1 1 auto;
  white-space: nowrap;
  min-height: 44px;
}
@media (max-width: 340px) {
  .controls {
    flex-wrap: wrap;
  }
}
```

Give buttons a visible `:focus-visible` outline, and turn off transitions when reduced motion is on.

## State design

- Keep all progress in one JSON state object, 256 KiB at most. Small is better.
- Include a `version` number in it so you can change the shape later.
- Validate restored state before you use it. Check types and ranges. If it does not look right, start fresh. Never trust it blindly.
- Restore must be deterministic: save, restore, save again must give the same state. `simulate` checks this. Store the seed or the generated board, not a value you re-roll on load.
- Call `autosave` after each meaningful change, not only on a timer.
- Do not put secrets or anything personal in state.

## Content packs and engines

Provisional (ADR-016). A content pack is data only, with no code. It names an engine and a data file in its manifest:

```json
"kind": "content",
"engine": "lullshift.cards@1",
"data": "deck.json"
```

An engine is an activity that receives the pack data as `content` in `onInit` (`init.content`). The engine validates the data against its own `data.schema.json`. Data must be at most 256 KiB. Reference files:

- engine: `activities/engines/calm-cards`
- pack: `activities/content/calm-cards/desk-stretches`

Create a pack with `pnpm lullshift create my-pack --kind content`. Validate it with the engine folder:

```bash
pnpm lullshift validate activities/content/calm-cards/desk-stretches --engines activities/engines/calm-cards
```

Packs still need `help` and a preview image.

## What the simulator checks

`pnpm lullshift simulate <folder>` loads the activity in a browser and runs these scenarios: save, restore (same state after save, restore, save), working/completed/failed/cancelled events, `needs_attention` with duplicate and out-of-order events, resume, `needs_attention` before ready, close, crash and recover, reduced motion and dark theme, paused-view stats, uncaught errors, and offline (no network requests). Add `--json` for machine-readable output.

Report the real output. If the simulator could not run, for example because no browser was found, say so. Do not claim the activity passes.

## Reference activities

- `activities/games/lantern-pond` (game)
- `activities/break/step-away` (break)
- `activities/engines/calm-cards` (engine) and `activities/content/calm-cards/desk-stretches` (pack)
- `tools/lullshift-cli/templates/app` (starter)
