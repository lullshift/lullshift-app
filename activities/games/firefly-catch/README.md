# Firefly Catch

A dusk meadow, a glass jar and slow glowing fireflies. Slide the jar to catch them for a calm round of 60, 90 or 180 seconds, or with no clock. Missed ones just fly away.

An original game bundled with Lullshift, built on the arcade kit (ADR-019).

## How to play

1. Move the jar left and right under a drifting firefly.
2. A firefly that drifts into the jar mouth is caught.
3. When the round time (60, 90 or 180 seconds, set with Length) is up, the round ends and shows how many you caught. Pausing stops the clock. Untimed rounds never end and keep no best score.
4. Press Play again for a new round. Your best is remembered for each timed length.

## Files

- `logic.js`: the rules, with no DOM access; unit-tested in `apps/desktop/src/activity/games/firefly-catch.test.ts`.
- `main.js`, `index.html`, `style.css`: the board, controls and bridge wiring.
- `lullshift-arcade-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.

## Checks

From the repository root:

```bash
pnpm lullshift validate activities/games/firefly-catch
pnpm lullshift simulate activities/games/firefly-catch
```

## License and assets

Apache-2.0. All code, text and art (including `preview.svg`) are original to this repository; no third-party assets.
