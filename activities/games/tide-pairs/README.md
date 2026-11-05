# Tide Pairs

A gentle memory game of sea creatures and shells. Turn two cards at a time and find every matching pair. A new deal each day, or as many as you like.

An original game bundled with Lullshift. It uses the card kit's deck and shuffle helpers (ADR-019) and draws its own layout.

## How to play

1. Turn a card, then turn a second card. That is one turn.
2. If the two match, they stay face up. If not, they turn back after a short pause. Pressing another card turns them back at once.
3. With reduced motion on, an unmatched pair stays showing until you press any card.
4. Daily deal is the same for everyone today. New deal gives a fresh one. Bigger switches to 12 pairs, and Smaller goes back to 8.
5. Your fewest turns is remembered for each board size.

## Files

- `logic.js`: the rules, with no DOM access; unit-tested in `apps/desktop/src/activity/games/tide-pairs.test.ts`.
- `main.js`, `index.html`, `style.css`: the table, controls and bridge wiring.
- `lullshift-card-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.

## Checks

From the repository root:

```bash
pnpm lullshift validate activities/games/tide-pairs
pnpm lullshift simulate activities/games/tide-pairs
```

## License and assets

Apache-2.0. All code, text and art (including `preview.svg`) are original to this repository; no third-party assets.
