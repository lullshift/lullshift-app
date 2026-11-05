# Moss Paths

A quiet garden of stream tiles. Turn each tile until water from the spring reaches every bed. A garden each day that remembers your best, or a new one whenever you like.

An original game bundled with Lullshift, built on the grid kit (ADR-019).

## How to play

1. Find the spring, the tile with the ring in the middle.
2. Turn a tile a quarter turn so its stream lines up with its neighbours. Water only passes where two tiles open toward each other.
3. Watch the Watered count grow until every tile is blue.
4. Today's garden is the same for everyone today and remembers your best. New garden deals a fresh one. Restart tangles the same garden again, and Undo takes back your last turn.

## Files

- `logic.js`: the rules, with no DOM access; unit-tested in `apps/desktop/src/activity/games/moss-paths.test.ts`.
- `main.js`, `index.html`, `style.css`: the board, controls and bridge wiring.
- `lullshift-grid-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.

## Checks

From the repository root:

```bash
pnpm lullshift validate activities/games/moss-paths
pnpm lullshift simulate activities/games/moss-paths
```

## License and assets

Apache-2.0. All code, text and art (including `preview.svg`) are original to this repository; no third-party assets.
