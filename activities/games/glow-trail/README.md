# Glow Trail

A firefly leaves a short glowing trail on a grid. Steer it to dew drops to make the trail longer. Bumping never ends the game; the trail just eases back.

An original game bundled with Lullshift, built on the arcade kit (ADR-019).

## How to play

1. Steer with the arrow keys; the firefly keeps gliding.
2. Each dew drop makes the trail one cell longer.
3. Touching your trail or the edge shortens the trail to its starting length.
4. After 2 minutes of play the round shows your longest trail and best. Slower gives a gentler pace.

## Files

- `logic.js`: the rules, with no DOM access; unit-tested in `apps/desktop/src/activity/games/glow-trail.test.ts`.
- `main.js`, `index.html`, `style.css`: the board, controls and bridge wiring.
- `lullshift-arcade-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.

## Checks

From the repository root:

```bash
pnpm lullshift validate activities/games/glow-trail
pnpm lullshift simulate activities/games/glow-trail
```

## License and assets

Apache-2.0. All code, text and art (including `preview.svg`) are original to this repository; no third-party assets.
