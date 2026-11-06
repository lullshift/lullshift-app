# Kite Drift

A one-button glide. Hold to lift a kite, let go to sink, and drift through the wide gaps between soft cloud banks. A bump never ends the game.

An original game bundled with Lullshift, built on the arcade kit (ADR-019).

## How to play

1. Hold Space, the Up arrow or the big on-screen button to lift the kite.
2. Release to let it sink slowly. Speed is capped so it always stays gentle. Pace sets how fast the sky scrolls: slow, normal or brisk.
3. Steer through the wide gap in each cloud bank. Each gap you pass adds to your streak.
4. If you touch a cloud the kite is bumped to the middle of the gap and your streak starts over. Flying simply goes on.

## Files

- `logic.js`: the rules, with no DOM access; unit-tested in `apps/desktop/src/activity/games/kite-drift.test.ts`.
- `main.js`, `index.html`, `style.css`: the board, controls and bridge wiring.
- `lullshift-arcade-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.

## Checks

From the repository root:

```bash
pnpm lullshift validate activities/games/kite-drift
pnpm lullshift simulate activities/games/kite-drift
```

## License and assets

Apache-2.0. All code, text and art (including `preview.svg`) are original to this repository; no third-party assets.
