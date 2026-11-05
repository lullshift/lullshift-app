# Thread Picture

A picture-by-numbers puzzle. Number clues tell you which cells to thread; fill them and a small picture appears. Each puzzle has one answer, reached by logic alone. A puzzle each day that remembers your best, or a new one whenever you like.

An original game bundled with Lullshift, built on the grid kit (ADR-019).

## How to play

1. Read a clue. For example, 3 1 means a run of three filled cells, then a gap, then one filled cell.
2. Fill cells you know belong to the picture. Mark cells you know are empty so you can stop thinking about them.
3. Work line by line: a clue that nearly fills its line settles many cells at once, and each settled cell helps the crossing lines.
4. Today's puzzle is the same for everyone today and remembers your best. New puzzle deals a fresh one. Restart clears the board, and Undo takes back your last move. Pick 5 by 5 or 10 by 10 with Size.

## Files

- `logic.js`: the clues, checking and puzzle generator, with no DOM access; unit-tested in `apps/desktop/src/activity/games/thread-picture.test.ts`.
- `main.js`, `index.html`, `style.css`: the grid, controls and bridge wiring.
- `lullshift-grid-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.
- `manifest.json` and `preview.svg`: the activity manifest and its preview picture.

## Checks

From the repository root:

```bash
pnpm lullshift validate activities/games/thread-picture
pnpm lullshift simulate activities/games/thread-picture
```

## License and assets

Apache-2.0. All art is drawn in code, CSS or inline SVG in this repository, and `preview.svg` is the only image file. No third-party assets.
