# Brook Links

A quiet garden of paired stones. Draw a stream between each matching pair without crossing. Covering every cell is optional and earns a full garden bonus. A garden each day that remembers your best, or a new one whenever you like.

An original game bundled with Lullshift, built on the grid kit (ADR-019).

## How to play

1. Find two stones with the same letter and colour. Each pair is one stream.
2. Start on a stone and draw toward its partner, one cell at a time. A stream cannot pass through another stream or another colour's stone.
3. Streams that run out of room can be pulled back and redrawn.
4. Today's garden is the same for everyone today and remembers your best. New garden deals a fresh one. Restart clears the streams, and Undo takes back your last stroke. Pick a size with Size.

## Files

- `logic.js`: the stream rules and garden generator, with no DOM access; unit-tested in `apps/desktop/src/activity/games/brook-links.test.ts`.
- `main.js`, `index.html`, `style.css`: the garden, controls and bridge wiring.
- `lullshift-grid-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.
- `manifest.json` and `preview.svg`: the activity manifest and its preview picture.

## Checks

From the repository root:

```bash
pnpm lullshift validate activities/games/brook-links
pnpm lullshift simulate activities/games/brook-links
```

## License and assets

Apache-2.0. All art is drawn in code, CSS or inline SVG in this repository, and `preview.svg` is the only image file. No third-party assets.
