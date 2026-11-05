# Tea Sort

Eight cups on a shelf, six teas. Pour the top layers between cups until every cup holds a single tea. Colours also differ by pattern and letter, so you never need to rely on colour alone. No timer. A puzzle each day, or a new one whenever you like.

An original game bundled with Lullshift, built on the grid kit (ADR-019).

## How to play

1. Pick up a cup, then choose the cup to pour into. Only the top run of one tea moves, and only as much as fits.
2. You can pour onto an empty cup, or onto a cup whose top layer is the same tea.
3. Each tea has its own colour, pattern and first letter.
4. Undo has no limit. Restart tangles the same puzzle again. Today's puzzle is the same for everyone today and remembers your best. New puzzle deals a fresh one.

## Files

- `logic.js`: the rules and puzzle generator, with no DOM access; unit-tested in `apps/desktop/src/activity/games/tea-sort.test.ts`.
- `main.js`, `index.html`, `style.css`: the shelf, controls and bridge wiring.
- `lullshift-grid-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.
- `manifest.json` and `preview.svg`: the activity manifest and its preview picture.

## Checks

From the repository root:

```bash
pnpm lullshift validate activities/games/tea-sort
pnpm lullshift simulate activities/games/tea-sort
```

## License and assets

Apache-2.0. All art is drawn in code, CSS or inline SVG in this repository, and `preview.svg` is the only image file. No third-party assets.
