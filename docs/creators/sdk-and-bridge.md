# SDK and bridge contract

This page condenses the contract every code activity follows. The full guide, with layout and help examples, is [packages/activity-sdk/README.md](../../packages/activity-sdk/README.md) (SDK version 0.2.0). Engines are code activities too and follow the same contract; content packs don't contain code, so the engine does this for them.

The SDK lives in this repository, in `packages/activity-sdk`. Publishing it to npm as `@lullshift/activity-sdk` is approved (decision 29), but it isn't on npm yet. Until then, use a clone. **Only after the first release** (check that the npmjs.com page links to `github.com/lullshift/lullshift-app` and shows a provenance badge), `npm install @lullshift/activity-sdk` gives you the bridge script (`bridge/lullshift-bridge-v1.js`), its types and the host protocol, for tools and TypeScript. Activities themselves don't need it installed: `lullshift create` copies the bridge into the activity folder.

## Files in an activity folder

- `manifest.json` (`"kind": "app"`, `"entrypoint": "index.html"`, `"runtime": "isolated-web-v1"`, and `"interruptibility": { "pause": true, "resume": true, "persistState": true }`)
- `index.html`, `style.css`, `main.js`
- `lullshift-bridge-v1.js`: an identical copy of `packages/activity-sdk/bridge/lullshift-bridge-v1.js`. `validate` checks it is unchanged
- `lullshift-<kit>-kit-v1.js` if you use a kit
- a preview image

No build step is required. Load scripts as classic scripts, bridge first:

```html
<script src="lullshift-bridge-v1.js"></script>
<script src="main.js"></script>
```

## Lifecycle

The bridge adds one global, `LullshiftBridge`.

```js
LullshiftBridge.start({
  onInit({ restore, reducedMotion, theme, content }) {
    // restore: last saved state, or null on a fresh start
    // reducedMotion: true when the player's system asks for less motion
    // theme: "light" or "dark"
    // content: a content pack's data (engines only), otherwise null
  },
  saveState() {
    return state; // the whole state; any JSON value except null
  },
  onPause() {}, // optional hint
  onResume() {}, // optional hint
});
```

| Step | Who | What happens |
|---|---|---|
| Ready | Activity | Call `start(handlers)` once. The host then sends `init` |
| Initialise and restore | Host, then activity | `onInit` runs once. If `restore` is not null, rebuild your screen from it |
| Save | Host asks, activity answers | `saveState()` returns your whole state |
| Autosave | Activity | Call `LullshiftBridge.autosave(state)` after every meaningful change |
| Pause and resume | Host | The host stops your page when an agent needs the user, when the view is hidden, or when the player pauses. `onPause` and `onResume` are hints only |
| Close or crash | Host | The page can be unloaded at any moment, with or without a reply. Anything not saved is lost |

**The host owns interruption.** It freezes the activity without waiting for it to agree. Don't rely on `onPause` to stay correct; rely on `autosave`.

## Saving state

- Keep all progress in one JSON object of at most 256 KiB. Small is better.
- Include a `version` number so you can change the shape later.
- Treat `restore` as untrusted. Check types and ranges; if anything looks wrong, start fresh.
- Restore must be deterministic: save, restore, save again gives the same state. Store the seed or the generated board, not something you re-roll on load. `simulate` checks this.
- Don't put secrets or anything personal in state.

## Paused-view summary

While paused, the host shows time played and your latest summary. Call this after each meaningful change:

```js
LullshiftBridge.summary([
  { label: "Moves", value: 12 },
  { label: "Progress", value: "4 of 10" },
]);
```

One to four items. Each `label` is plain text up to 24 characters; each `value` is plain text up to 24 characters or a finite number. Don't report time played; the host does that.

## Storage permission

`LullshiftBridge.storage.get(key)`, `.set(key, value)` and `.remove(key)` return promises of strings. They work only if the manifest declares `"permissions": ["activity.storage"]`. Use them for small things that outlive a reset, such as a best score. Prefer the single state object for everything else.

`activity.storage` is the only permission in manifest schema v1. A content pack can use it only if both the pack and its engine declare it. Declare nothing you don't use; marketplace reviews show permissions to the user, and updates highlight added ones.

## Keyboard, reduced motion and theme

- Everything works with the keyboard, with a visible `:focus-visible` outline and text labels on controls. `simulate` counts focusable elements.
- When `reducedMotion` is true: no animations or transitions. Move things in steps.
- Follow `theme` with CSS variables so light and dark both read well.
- Keep controls on one row when they fit, and never let a button label break mid-word (`white-space: nowrap`).

## Help and preview

Every activity needs `help` in `manifest.json`: `summary`, `howToPlay` (required), plus `goal`, `controls` and `tips`. Plain text only. `validate` warns when it is missing; treat that as a failure. Add `"preview"` pointing at an original image of at most 256 KiB.

## Manifest essentials

From `reference/ACTIVITY_MANIFEST_V1.schema.json`:

| Field | Notes |
|---|---|
| `schemaVersion` | `1` |
| `kind` | `app` or `content` |
| `id` | Reverse-DNS, such as `dev.you.my-game`. Without `--id`, `create` uses `dev.local.<name>`; change it before publishing |
| `version` | Semver |
| `name`, `description`, `author` | Plain text. `author` is self-declared and unverified |
| `license` | SPDX expression for your code and content |
| `category` | `arcade`, `break`, `productivity` or `interactive` |
| `minHostVersion`, `sdkVersion` | Semver |
| `permissions` | `[]` or `["activity.storage"]` |
| `offline` | Must be `true` |

Unknown fields are refused.

## Reference activities

- `activities/games/lantern-pond`: reference game
- `activities/break/step-away`: break activity
- `tools/lullshift-cli/templates/app`: the starter `create` copies
