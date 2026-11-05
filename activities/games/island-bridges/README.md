# Island Bridges

A quiet sea of numbered islands. Join them with bridges until every island has its number and the whole archipelago is connected. A sea each day that remembers your best, or a new one whenever you like.

An original game bundled with Lullshift, built on the grid kit (ADR-019).

## How to play

1. Select an island, then add bridges toward the next island up, down, left or right of it.
2. A bridge runs in a straight line between two islands with no island in between. At most two bridges join a pair.
3. Bridges cannot cross each other, and an island never takes more bridges than its number. Finished islands turn green and get a double ring.
4. You win when every island has its number and only one group is left.
5. Today's sea is the same for everyone today and remembers your best. New sea deals a fresh one. Restart clears the bridges, and Undo takes back your last move. Pick 7 by 7 or 9 by 9 with Size.

## Files

- `logic.js`: the bridge rules and sea generator, with no DOM access; unit-tested in `apps/desktop/src/activity/games/island-bridges.test.ts`.
- `main.js`, `index.html`, `style.css`: the sea, controls and bridge wiring.
- `lullshift-grid-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.
- `manifest.json` and `preview.svg`: the activity manifest and its preview picture.

## Checks

From the repository root:

```bash
pnpm lullshift validate activities/games/island-bridges
pnpm lullshift simulate activities/games/island-bridges
```

## License and assets

Apache-2.0. All art is drawn in code, CSS or inline SVG in this repository, and `preview.svg` is the only image file. No third-party assets.
