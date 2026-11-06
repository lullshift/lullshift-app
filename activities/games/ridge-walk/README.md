# Ridge Walk

A quiet patience game. Seven ridges of cards and a path that climbs and falls one step at a time. Walk the path to clear every ridge.

An original game bundled with Lullshift. It uses the card kit's deck and shuffle helpers (ADR-019) and draws its own layout.

## How to play

1. Only the top card of each ridge can be played. The cards under it are waiting.
2. Play a top card if its rank is exactly one above or one below the card on the path. A King connects to an Ace and an Ace to a King, so the ranks go round in a circle.
3. Stuck? Flip card turns the next stock card onto the path. You may flip whenever you like.
4. If the stock runs out and nothing fits, the path ends there. Try again replays the same deal, New deal gives a fresh one.
5. Undo takes back your last play or flip, as many times as you like.

## Files

- `logic.js`: the rules, with no DOM access; unit-tested in `apps/desktop/src/activity/games/ridge-walk.test.ts`.
- `main.js`, `index.html`, `style.css`: the table, controls and bridge wiring.
- `lullshift-card-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.

## Checks

From the repository root:

```bash
pnpm lullshift validate activities/games/ridge-walk
pnpm lullshift simulate activities/games/ridge-walk
```

## License and assets

Apache-2.0. All code, text and art (including `preview.svg`) are original to this repository; no third-party assets.
