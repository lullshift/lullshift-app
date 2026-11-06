# Lullshift Activity SDK

Everything you need to build a Lullshift activity: the bridge client, the host protocol types, and the manifest, help and summary formats. Version 0.2.0. This guide is for people and for AI tools.

Generated code is untrusted. Follow the sandbox rules below. Never ask for more permission than the activity needs.

## Where it lives

The SDK is in this repository. Publishing it to npm as `@lullshift/activity-sdk` is approved (decision 29) but hasn't happened yet. After the first release, `npm install @lullshift/activity-sdk` and `npx @lullshift/cli` work without a clone; until then, use a clone. In the npm package, the paths below are relative to the package: `bridge/` as here, and `dist/` (compiled JavaScript and types) instead of `src/`.

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

ADR-016. A content pack is data only, with no code. It names an engine and a data file in its manifest:

```json
"kind": "content",
"engine": "lullshift.cards@1",
"data": "deck.json"
```

**Engines.** An engine is an `app` activity with one more file, `engine.json`, naming its id, major version and data schema (`data.schema.json`, JSON Schema 2020-12). A pack names it as `id@major`. The bundled engines live in `activities/engines/`; each has fixtures in `tests/fixtures/engines/<id>@<major>/{valid,invalid}/`.

**`init.content`.** The engine receives the pack's data, already checked, as `content` in `onInit`:

```js
LullshiftBridge.start({
  onInit({ restore, content }) {
    // content: the pack's data (an object valid against data.schema.json); null for other activities
  },
});
```

Still treat `content` as untrusted text: render it with `textContent`, never `innerHTML`, and keep the engine's own bounds checks.

**How the app runs a pack.** Packs appear in Activities as their own cards, with the pack's name, description, preview and help; engines are never listed. The app serves the engine's files at the pack's own address, so each pack has its own saved state, storage and time played. Before starting, the app resolves `id@major` to a bundled engine and checks the data against the engine's schema again. If either fails, the player sees "This pack's data doesn't match its engine" (or that the engine is missing) and the engine never starts. A pack may use `activity.storage` only if both the pack and its engine declare it.

**Rules for data schemas.** Objects set `"additionalProperties": false`, strings have `maxLength`, lists have `maxItems`. The app's check understands a subset of JSON Schema: `type`, `enum`, `const`, `properties`, `required`, `additionalProperties`, `min/maxProperties`, `items`, `min/maxItems`, `uniqueItems`, `min/maxLength`, `pattern`, `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf`, `allOf`, `anyOf`, `oneOf`, `not`, `if`/`then`/`else`, `$ref` to `#/$defs/<name>`, and annotations such as `title` and `description`. Patterns can't use `\b`, lookaround or backreferences. `$schema`, `$id` (a `urn:` name) and `$defs` go only at the top, a schema can't refer to itself in a loop, and it may have at most 2,000 parts once its references are expanded. The app checks data with the `jsonschema` crate behind these rules (ADR-016). `node tests/activities/check-engine-compat.mjs` checks all of this.

**What a pack holds (v1).** `manifest.json`, the data file (at most 256 KiB), a preview image (at most 256 KiB), notes (`.md`, `.txt`) and, since Sprint 12, up to 20 still PNG or WebP pictures listed in the manifest's `images` (each `images/<name>.png|webp`, at most 512 KiB and 2048 × 2048 pixels, with no animation or metadata). No code and no other media: `validate` refuses anything else.

**Showing a pack's pictures in an engine.** The app serves a pack's listed pictures at the pack's own address, so an engine shows one with a relative URL: `img.src = "images/hexagon.png"` (the path from the data, which the app has already checked is listed). Never build the URL from anything else, always set `alt` from the data, and handle `error` by showing the alt text instead. An engine must not have an `images/` folder of its own; the app never serves it for a pack.

**Compatibility.** A change that would make any existing valid pack invalid, or change how it plays in a way that matters, needs a new major, which ships beside the old one. No major is removed within v1.

Reference files:

- engine: `activities/engines/calm-cards`
- pack: `activities/content/calm-cards/desk-stretches`

Create a pack with `pnpm lullshift create my-pack --kind content`. Validate it; problems name the file, the place in the data and what is wrong (`deck.json: cards[3].front is longer than 200 characters`):

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
