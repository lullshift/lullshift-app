# Sea Glass

An 8 by 8 beach of sea-glass shapes. Swap neighbours to line up three or more, in a calm round of 25 swaps. Colours also differ by shape. No timer. A board each day, or a new one whenever you like.

An original game bundled with Lullshift, built on the grid kit (ADR-019).

## How to play

1. Move to a piece and press Enter to pick it up.
2. Press an arrow key to swap it with the neighbour in that direction. Only swaps that make a line of three or more are allowed.
3. Cleared pieces score points, and chains of new lines score a little more each step.
4. If no swap is left, the glass is gently reshuffled. When the 25 swaps are used, the round is complete.
5. Today's board is the same for everyone today. New board deals a fresh one, and Restart replays the same one.

## Files

- `logic.js`: the board, swap and chain rules, with no DOM access; unit-tested in `apps/desktop/src/activity/games/sea-glass.test.ts`.
- `main.js`, `index.html`, `style.css`: the beach, controls and bridge wiring.
- `lullshift-grid-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.
- `manifest.json` and `preview.svg`: the activity manifest and its preview picture.

## Checks

From the repository root:

```bash
pnpm lullshift validate activities/games/sea-glass
pnpm lullshift simulate activities/games/sea-glass
```

## License and assets

Apache-2.0. All art is drawn in code, CSS or inline SVG in this repository, and `preview.svg` is the only image file. No third-party assets.
