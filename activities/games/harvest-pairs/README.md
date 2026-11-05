# Harvest Pairs

A calm card puzzle. Clear a stacked triangle of cards by pairing two that add up to 13. The four suits are original: Leaves, Acorns, Berries and Shells. A deal each day, or a new one whenever you like.

An original game bundled with Lullshift, built on the card kit (ADR-019). The suit art is drawn with SVG shapes in `main.js`; the cards are laid out and navigated by the game itself because the kit's table binding has no stacked-triangle layout.

## How to play

1. Free cards (no card overlapping them from the row below) can be used.
2. Pair two free cards that add up to 13. Ace is 1, Jack 11, Queen 12. A King goes alone.
3. Draw from the draw pile onto the waste pile; its top card can pair with a free card. Turn the waste pile over when the draw pile is empty, as often as you like.
4. Clear the triangle to win. Undo has no limit.

## Deals

A deal is a seeded shuffle. The game tries up to eight shuffles per seed and keeps the first one that the solver in `logic.js` can clear (depth-first search over an exact state space, 60000-node budget). If none is proven clearable the first shuffle is used and the game says it could not be checked. After each pairing the solver runs again (20000 nodes) and the game says honestly when no clearing path remains.

## Files

- `logic.js`: the rules, the solver and the deals, with no DOM access; tested in `apps/desktop/src/activity/games/harvest-pairs.test.ts`.
- `main.js`, `index.html`, `style.css`: the board, controls and bridge wiring.
- `lullshift-card-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.

## Checks

```bash
pnpm lullshift validate activities/games/harvest-pairs
pnpm lullshift simulate activities/games/harvest-pairs
```

## License and assets

Apache-2.0. All code, text and art (including `preview.svg`) are original to this repository; no third-party assets.
