# Fog Garden

A quiet garden under mist. Clear the fog tile by tile and use the numbers to work out where the brambles hide. No timer, and a fresh start is always one press away.

An original game bundled with Lullshift, built on the grid kit (ADR-019).

## How to play

1. Reveal any tile to begin. The first tile is always safe, and so are the tiles around it.
2. A number tells you how many brambles are in the tiles around it. A blank tile has none, so its neighbours clear on their own.
3. Use the numbers to work out which tiles are safe and which hide brambles. Mark the ones you are sure of so you do not reveal them by accident.
4. If you reveal a bramble the fog simply closes in. Try again keeps the same field, New field gives a different one, and Today's field is the same for everyone today.

## Files

- `logic.js`: the rules, with no DOM access; unit-tested in `apps/desktop/src/activity/games/fog-garden.test.ts`.
- `main.js`, `index.html`, `style.css`: the board, controls and bridge wiring.
- `lullshift-grid-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.

## Checks

From the repository root:

```bash
pnpm lullshift validate activities/games/fog-garden
pnpm lullshift simulate activities/games/fog-garden
```

## License and assets

Apache-2.0. All code, text and art (including `preview.svg`) are original to this repository; no third-party assets.
