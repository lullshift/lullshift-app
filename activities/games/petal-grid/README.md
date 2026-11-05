# Petal Grid

A quiet flower-bed logic puzzle. Plant each kind of petal exactly once in every row and every column. Some petals are already in place. A new bed each day, or as many as you like.

An original game bundled with Lullshift, built on the grid kit (ADR-019).

## How to play

1. Move to an empty square and type a number to plant that petal. Planted petals that came with the puzzle are fixed.
2. If a petal repeats in its row or column, both squares are softly marked and the game says so. Nothing is lost; change either one.
3. Type 0, Backspace or Delete to clear a square you planted.
4. Today's bed is the same for everyone today. New bed deals a fresh one. Size switches between 4 by 4 and the larger 6 by 6.
5. Undo takes back your last change. Clear removes everything you planted.

## Files

- `logic.js`: the rules, with no DOM access; unit-tested in `apps/desktop/src/activity/games/petal-grid.test.ts`.
- `main.js`, `index.html`, `style.css`: the board, controls and bridge wiring.
- `lullshift-grid-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.

## Checks

From the repository root:

```bash
pnpm lullshift validate activities/games/petal-grid
pnpm lullshift simulate activities/games/petal-grid
```

## License and assets

Apache-2.0. All code, text and art (including `preview.svg`) are original to this repository; no third-party assets.
