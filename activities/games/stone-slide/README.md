# Stone Slide

Smooth river stones sit in a frame with one gap. Slide them, one at a time, until the numbers run in order. A fresh scramble each day, or as many new ones as you like.

An original game bundled with Lullshift, built on the grid kit (ADR-019).

## How to play

1. Press a stone that touches the empty space (not diagonally). It slides into the space.
2. Keep sliding until the numbers run in order and the space is in the bottom right corner.
3. Every scramble is made by sliding stones from the solved frame, so it can always be solved.
4. Today's stones is the same scramble for everyone today. New stones deals a fresh one. Size switches between 4 by 4 and a smaller 3 by 3.
5. Undo takes back your last slide. Restart puts the stones back to how they began.

## Files

- `logic.js`: the rules, with no DOM access; unit-tested in `apps/desktop/src/activity/games/stone-slide.test.ts`.
- `main.js`, `index.html`, `style.css`: the board, controls and bridge wiring.
- `lullshift-grid-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.

## Checks

From the repository root:

```bash
pnpm lullshift validate activities/games/stone-slide
pnpm lullshift simulate activities/games/stone-slide
```

## License and assets

Apache-2.0. All code, text and art (including `preview.svg`) are original to this repository; no third-party assets.
