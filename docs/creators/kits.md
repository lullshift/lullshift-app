# Kits

Kits are shared game code: seeded puzzles, undo, keyboard navigation, cards and piles, and a game loop that pauses properly. They are optional. Kit version 1.0.0, Apache-2.0 (ADR-019).

The full API is in [packages/activity-kits/README.md](../../packages/activity-kits/README.md). This page is a short orientation.

## What a kit is, and isn't

- A kit is ordinary code inside your activity's sandbox. It adds no permissions and no manifest fields.
- The host doesn't trust an activity more because it uses a kit.
- Kits never save. Your game saves through the bridge: put the kit's `snapshot()` in your state, call `LullshiftBridge.autosave(state)` after each change, and pass the saved part to `restore()` on init. `restore()` returns `false` for anything malformed; start fresh then.

## Using one

```bash
pnpm lullshift create my-game --kit grid --name "My Game" --id dev.you.my-game
```

`create` copies `lullshift-grid-kit-v1.js` into the folder and loads it after the bridge, before `main.js`. In `main.js`:

```js
var K = window.LullshiftKits.grid;
```

Never edit the copy. `validate` warns when it differs from the SDK's. A breaking change will ship as a `-v2` file beside it, so your copy keeps working.

## The three kits

| Kit | Use it for | Main pieces |
|---|---|---|
| `grid` | Number placement, picture logic, tile matching, sliding, flood fill and other grid puzzles | `Grid` (cells, cursor, selection, undo and redo), `generateGrid`, `scatter`, and `bindGrid`, which renders an accessible `role="grid"` with one tab stop that follows the cursor |
| `card` | Patience and other card games | `buildDeck`, `customDeck`, `shuffleDeck`, `Table` (piles, your `canMove` rules, pick-up-and-place, undo) and `bindTable`, which renders labelled piles of card buttons |
| `arcade` | Real-time games | `Loop` (fixed steps, stops while paused or hidden), `ResumeGate` (a 3, 2, 1 countdown, or a "press Enter" prompt with reduced motion) and `runArcade`, which wires both to the bridge and autosaves every 2 seconds of play and on every pause |

Every kit includes the core: a seeded random generator (`createRng`), `dailySeed` for a daily puzzle from the player's local date, an undo `History`, and `keyAction`, which maps keys to actions such as `up`, `activate`, `cancel`, `undo` and `redo` on any keyboard layout.

## Games that use them

Most bundled games in `activities/games/` use a kit. Lantern Pond (`activities/games/lantern-pond`) is the reference game.

## Changing a kit

Kit sources are in `packages/activity-kits/src/`. After changing them:

```bash
pnpm --filter @lullshift/activity-kits test
pnpm kits:build
pnpm kits:check
```

`pnpm kits:check` is part of `pnpm check` and fails when `dist/` is out of date.
