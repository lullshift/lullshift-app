# Breeze Bounce

A light seed floats and bounces on the breeze. Move a leaf to keep it aloft and release soft dandelion clocks. No lives: a missed seed drifts back in from the top.

An original game bundled with Lullshift, built on the arcade kit (ADR-019).

## How to play

1. Slide the leaf left and right under the seed.
2. Every dandelion the seed touches is released.
3. When all are released the round ends and shows your time and best time. Pausing stops the clock.
4. Press Play again for a new seeded layout.

## Files

- `logic.js`: the rules, with no DOM access; unit-tested in `apps/desktop/src/activity/games/breeze-bounce.test.ts`.
- `main.js`, `index.html`, `style.css`: the board, controls and bridge wiring.
- `lullshift-arcade-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.

## Checks

From the repository root:

```bash
pnpm lullshift validate activities/games/breeze-bounce
pnpm lullshift simulate activities/games/breeze-bounce
```

## License and assets

Apache-2.0. All code, text and art (including `preview.svg`) are original to this repository; no third-party assets.
