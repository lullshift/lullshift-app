# Lantern Pond

A 5 by 5 pond of paper lanterns. Pressing a lantern flips it and the lanterns directly above, below, left and right of it. Light every lantern. No timer and no wrong answer. A new pond each day, or as many fresh ones as you like.

An original game bundled with Lullshift, built on the grid kit (ADR-019).

## How to play

1. Press any lantern. It and its up to four neighbours switch between lit and unlit.
2. Keep pressing until all 25 lanterns are lit.
3. Today's pond is the same for everyone today and remembers your best. New pond deals a fresh one.
4. Undo takes back your last press (Ctrl+Z works too). Restart puts the pond back to how it began.

## Files

- `logic.js`: the rules and puzzle generator, with no DOM access; unit-tested in `apps/desktop/src/activity/games/lantern-pond.test.ts`.
- `main.js`, `index.html`, `style.css`: the pond, controls and bridge wiring. The grid model, roving focus and keyboard navigation come from the grid kit.
- `lullshift-grid-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.
- `manifest.json` and `preview.svg`: the activity manifest and its preview picture.

## Saved state

`board`, `history`, `moves`, `seed`, `mode` and `solved` are unchanged from 0.1.0, so older saves restore. `cursor` is new and optional.

## Checks

From the repository root:

```bash
pnpm lullshift validate activities/games/lantern-pond
pnpm lullshift simulate activities/games/lantern-pond
```

## License and assets

Apache-2.0. All art is drawn in code, CSS or inline SVG in this repository, and `preview.svg` is the only image file. No third-party assets.
