# Pebble Merge

A sliding merge puzzle on a 4 by 4 river tray. Slide the pebbles, join two of a size into the next size, and see how large a stone you can make. Nothing to lose, and no score to chase. A tray each day, or a new one whenever you like.

An original game bundled with Lullshift, built on the grid kit (ADR-019).

## How to play

1. Press an arrow key to slide every pebble that way until it meets the edge or another pebble.
2. When two pebbles of the same size meet, they join into the next size up. Each pair joins once per slide.
3. After each slide that moves something, one new small pebble arrives in an empty spot.
4. Undo takes back your last slide, one step only. Today's tray is the same for everyone today and remembers the largest pebble you made. New tray deals a fresh one.

## Files

- `logic.js`: the sliding and merging rules, with no DOM access; unit-tested in `apps/desktop/src/activity/games/pebble-merge.test.ts`.
- `main.js`, `index.html`, `style.css`: the tray, controls and bridge wiring.
- `lullshift-grid-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.
- `manifest.json` and `preview.svg`: the activity manifest and its preview picture.

## Checks

From the repository root:

```bash
pnpm lullshift validate activities/games/pebble-merge
pnpm lullshift simulate activities/games/pebble-merge
```

## License and assets

Apache-2.0. All art is drawn in code, CSS or inline SVG in this repository, and `preview.svg` is the only image file. No third-party assets.
