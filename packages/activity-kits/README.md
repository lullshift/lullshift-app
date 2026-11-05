# Lullshift Activity Kits

Shared code for games: seeded puzzles, undo, keyboard navigation, cards and piles, and a game loop that pauses properly. Kit version 1.0.0 (ADR-019). Apache-2.0.

A kit is ordinary code inside your activity's sandbox. It adds no permissions and no manifest fields, and the host does not trust an activity more because it uses one.

## Files

| Path | What it is |
|---|---|
| `dist/lullshift-grid-kit-v1.js` | Grid kit as one classic script. Copy it into your activity. |
| `dist/lullshift-card-kit-v1.js` | Card kit. |
| `dist/lullshift-arcade-kit-v1.js` | Arcade kit. |
| `dist/lullshift-<kit>-kit-v1.d.ts` | Types for `window.LullshiftKits.<kit>`. |
| `src/core`, `src/grid`, `src/card`, `src/arcade` | The TypeScript sources. Import these if you use a bundler. |
| `scripts/build-kits.ts` | Builds `dist/` from `src/`. |

Each kit file includes the core (random numbers, undo history, key mapping, JSON checks). Load one kit per activity; loading two is allowed but carries two copies of the core.

## Using a kit

```bash
pnpm lullshift create my-game --kit grid --name "My Game" --id dev.you.my-game
```

`create` copies the kit and loads it after the bridge, before your code:

```html
<script src="lullshift-bridge-v1.js"></script>
<script src="lullshift-grid-kit-v1.js"></script>
<script src="main.js"></script>
```

In `main.js`:

```js
var K = window.LullshiftKits.grid;
```

Never edit the copy. `lullshift validate` warns when it differs from the SDK's. A breaking change will ship as `-v2` beside it; your copy keeps working.

## Rules every kit follows

- No network, no storage, no clipboard, no Tauri, no `eval`, no HTML parsing. The build refuses source that mentions them.
- Kits never save. Your game saves through the bridge: put the kit's `snapshot()` into your state and call `LullshiftBridge.autosave(state)` after each change. On `onInit`, pass the saved part to `restore()`. It returns `false` for anything malformed; start fresh then.
- Snapshots are plain JSON and include undo history, so undo survives an interruption.
- Text reaches the page through `textContent` only.

## Core (in every kit)

```ts
createRng(seed: number): Rng
  // rng.next() in [0, 1), rng.int(min, max) inclusive, rng.pick(items), rng.shuffle(items)
  // rng.state(): save it; createRng(state) continues the same sequence
dailySeed(date?: Date | "YYYY-MM-DD", salt?: string): number   // from the player's local date
randomSeed(): number
hashString(text: string): number      // FNV-1a, 32-bit
localDateKey(date: Date): string      // "2026-11-05"
new History<T>(limit = 100)           // record(before), undo(current), redo(current),
                                      // canUndo(), canRedo(), snapshot(), restore(state, isEntry)
keyAction(event, { wasd?: boolean }): KeyAction | null
isJson(value), cloneJson(value), isRecord(value), isIntIn(value, min, max)
```

The random generator is mulberry32: one 32-bit number of state, fast, fine for games, not for secrets. `dailySeed` gives everyone the same puzzle on the same local day; add a `salt` (your game's name) so two games don't share one.

`keyAction` returns one of `up`, `down`, `left`, `right`, `home`, `end`, `first` (Ctrl+Home), `last` (Ctrl+End), `pageUp`, `pageDown`, `activate` (Enter, Space), `cancel` (Escape), `undo` (Ctrl/Cmd+Z), `redo` (Ctrl/Cmd+Y, Ctrl/Cmd+Shift+Z), or `null`. Named keys work on every layout. WASD is off by default; when on, it uses key positions, so it is ZQSD on AZERTY.

## Grid kit

```ts
new Grid<C>(width, height, fill: C | ((x, y) => C), { wrap?: boolean, historyLimit?: number })
grid.cells, grid.size, grid.cursor, grid.get(x, y), grid.set(x, y, v), grid.index(x, y),
grid.point(i), grid.inBounds(x, y), grid.neighbours(x, y, diagonal?)
grid.edit(change): boolean            // records undo when cells changed
grid.undo(), grid.redo(), grid.canUndo(), grid.canRedo(), grid.clearHistory()
grid.navigate(action): boolean        // arrows, Home/End (row), Ctrl+Home/End (corners),
                                      // PageUp/PageDown (column top and bottom)
grid.setCursor(x, y), grid.moveCursor(dx, dy)
grid.select(x, y, on?), grid.toggleSelect(x, y), grid.isSelected(x, y), grid.selection(),
grid.clearSelection()
grid.shuffle(rng)
grid.snapshot(): GridState<C>, grid.restore(state, isCell?): boolean
generateGrid(width, height, rng, (rng, x, y) => C, options?): Grid<C>
scatter(grid, rng, count, where?): Point[]   // distinct cells
bindGrid(container, grid, options): { render(), focus(), destroy() }
```

`bindGrid` options: `name` (the grid's accessible name), `label(cell, x, y)` (required, read per cell), and optional `text`, `className`, `pressed` (sets `aria-pressed`), `selectable` (sets `aria-selected`), `onActivate(x, y)`, `onMove(x, y)`, `onAction(action, event)`, `onChange()`, `keys`.

It renders `role="grid"` with rows and gridcells, each holding a button. One button is in the tab order and follows the cursor. Enter, Space and clicks call `onActivate`. Ctrl+Z and Ctrl+Y undo and redo unless `onAction` handles them first. Call `render()` after you change the model.

```js
var K = window.LullshiftKits.grid;
var grid = new K.Grid(5, 5, false);
var view = K.bindGrid(document.getElementById("board"), grid, {
  name: "Pond, 5 by 5",
  label: function (lit, x, y) {
    return "Row " + (y + 1) + ", column " + (x + 1) + ", " + (lit ? "lit" : "unlit");
  },
  pressed: function (lit) { return lit; },
  onActivate: function (x, y) {
    grid.edit(function (g) { g.set(x, y, !g.get(x, y)); });
    view.render();
    save();
  },
  onChange: save,
});
function save() { LullshiftBridge.autosave({ version: 1, grid: grid.snapshot() }); }
```

## Card kit

```ts
buildDeck({ suits?, ranks?, copies? }): Card[]    // default 52 cards, face down
customDeck([{ suit, rank, id?, name? }]): Card[]  // your own set; ids must be unique
shuffleDeck(cards, rng): Card[]
cardLabel(card), rankName(rank), suitColor(suit), isCard(value)
new Table(piles: { id, name, cards? }[], { canMove?(move, table), revealAfterMove?, historyLimit? })
table.piles, table.pile(id), table.top(id)
table.canMove(from, to, count = 1), table.move(from, to, count = 1): boolean
table.deal(from, to, count, faceUp): number       // ignores rules
table.edit(change): boolean
table.undo(), table.redo(), table.canUndo(), table.canRedo()
table.focus, table.navigate(action)               // left/right piles, up/down cards
table.selection, table.activate(): "selected" | "moved" | "rejected" | "cleared" | "none"
table.snapshot(): TableState, table.restore(state): boolean
bindTable(container, table, options): { render(), focus(), destroy() }
```

Your rules go in `canMove`. The table already checks that the cards exist, are face up and change piles. `activate()` is pick-up-and-place: the first press picks up the focused card and those on top of it, the second press tries to put them on the focused pile.

`bindTable` renders each pile as a labelled group ("Foundation 1, 3 cards") of card buttons. An empty pile shows an "Empty ..." button so it can be a target. Picked-up cards are announced as "picked up". Escape drops the selection. Options: `name`, and optional `label`, `text`, `className`, `onActivate(pileId, cardIndex)` (return true to handle it yourself), `onResult(result)`, `onAction`, `onChange`, `keys`.

## Arcade kit

```ts
new Loop({ update(dt), render?(alpha), stepMs?, maxStepsPerFrame?, frames?, visibility?,
           onHidden?, onVisible? })
loop.resume(), loop.pause(), loop.stop(), loop.paused, loop.elapsed, loop.steps,
loop.snapshot(), loop.restore(state)
new ResumeGate({ show(text), done(), reducedMotion?, seconds?, timers? })
gate.start(), gate.confirm(), gate.cancel(), gate.active
runArcade({ bridge, onInit(info), update, render?, save(), prompt(text), onPausedChange?,
            autosaveSeconds?, countdownSeconds?, ... }): ArcadeSession
session.pause(), session.resume(), session.confirm(), session.saveNow(),
session.paused, session.reducedMotion, session.loop, session.gate
READY_PROMPT, PAUSED_PROMPT
```

The loop calls `update` in fixed steps (default 1/60 s) on `requestAnimationFrame`. While paused or hidden, no frames run and game time does not move. Hiding the page pauses it, and it stays paused until resumed, so the player never comes back to a lost game.

`runArcade` does the wiring:

- It calls `LullshiftBridge.start`. On init it passes you `restore` and `reducedMotion`, then starts the resume gate.
- The gate counts 3, 2, 1 through `prompt(text)`. With reduced motion there is no countdown: it shows "Ready. Press Enter or Space to play." and waits.
- When the host pauses, the loop stops at once. `saveState` returns `save()`. On resume the gate runs again.
- It autosaves every 2 seconds of play and on every pause.

Send Enter and Space to `session.confirm()`, and your pause key to `session.pause()`. Show `prompt` text in an `aria-live` element. When `session.reducedMotion` is true, skip shakes, flashes and smooth scrolling; move things in steps.

```js
var A = window.LullshiftKits.arcade;
var state = fresh();
var session = A.runArcade({
  bridge: LullshiftBridge,
  onInit: function (info) { state = valid(info.restore) ? info.restore : fresh(); },
  update: function (dt) { state.x += state.vx * dt; },
  render: draw,
  save: function () { return state; },
  prompt: function (text) { document.getElementById("prompt").textContent = text; },
});
document.addEventListener("keydown", function (e) {
  var a = A.keyAction(e);
  if (a === "activate" && session.confirm()) e.preventDefault();
  else if (a === "cancel") session.pause();
});
```

## Tests and building

```bash
pnpm --filter @lullshift/activity-kits test    # unit tests, and dist matches a fresh build
pnpm kits:build                                # regenerate dist/ after changing src/
pnpm kits:check                                # fails when dist/ is out of date (part of pnpm check)
```

The DOM bindings are tested against a small fake DOM. Real focus and screen reader behaviour is checked with `lullshift simulate` and by hand on the games that use the kits.

Source rules, enforced by the build: imports only from the kit's own files and `core`; top-level `export function`, `export const` or `export class` only; no repeated top-level names across files; nothing from the blocked list above.
