# Clock Patience

A calm game of chance. Thirteen card piles sit like a clock face, Kings in the centre. Turn the top card of the current pile, place it under the pile for its rank, and carry on from there. A deal each day, or a new one any time.

An original game bundled with Lullshift, built on the card kit (ADR-019) for its seeded random numbers and daily seed. The suit art (Leaves, Acorns, Berries, Shells) is drawn with SVG shapes in `main.js`.

## How to play

1. Start at the King pile in the centre and turn its top card.
2. Place it face up under the pile for its rank and continue from that pile.
3. The game ends when the fourth King is turned. You win if every pile is complete by then.
4. Turn plays step by step on its own (about one card every 0.7 seconds) until you press Pause. One card turns a single card.

The result is reported honestly, for example "2 piles left this time, with 6 cards still face down." It is pure chance: the deal decides everything, about one deal in thirteen is a win.

## Files

- `logic.js`: the rules, with no DOM access; tested in `apps/desktop/src/activity/games/clock-patience.test.ts`.
- `main.js`, `index.html`, `style.css`: the clock, controls and bridge wiring. Saved state is only the seed and the number of turns, and restoring replays them.
- `lullshift-card-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.

## Checks

```bash
pnpm lullshift validate activities/games/clock-patience
pnpm lullshift simulate activities/games/clock-patience
```

## License and assets

Apache-2.0. All code, text and art (including `preview.svg`) are original to this repository; no third-party assets.
