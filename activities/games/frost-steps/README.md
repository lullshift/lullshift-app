# Frost Steps

A quiet sliding puzzle on a frozen pond. The walker slides in a direction until a rock or the edge stops it; the warm lantern stops it too. Reach the lantern. A puzzle each day that remembers your best, or a new one whenever you like.

An original game bundled with Lullshift, built on the grid kit (ADR-019).

## How to play

1. The walker is the blue circle, the lantern the orange square, rocks are grey stones.
2. Slide with the arrow keys, W A S D, or the buttons under the board.
3. Reach the lantern. The game then shows the best possible number of moves.
4. Today's puzzle is the same for everyone on a given local date. New puzzle deals a fresh one. Undo takes back slides.

## Puzzles

Puzzles come from a seed: the generator scatters rocks, then picks a lantern square and verifies it with an exact breadth-first search over stopping squares, with the lantern in place. Each size has a minimum solution length (6 by 6: 6 moves, 7 by 7: 8, 8 by 8: 10, 9 by 9: 12), so none is trivial and all are solvable. "Best possible" is that search's result, recomputed when a saved puzzle is loaded.

## Files

- `logic.js`: the rules, solver and generator, with no DOM access; tested in `apps/desktop/src/activity/games/frost-steps.test.ts`.
- `main.js`, `index.html`, `style.css`: the pond, controls and bridge wiring.
- `lullshift-grid-kit-v1.js` and `lullshift-bridge-v1.js`: copies of the SDK files; don't edit them here.

## Checks

```bash
pnpm lullshift validate activities/games/frost-steps
pnpm lullshift simulate activities/games/frost-steps
```

## License and assets

Apache-2.0. All code, text and art (including `preview.svg`) are original to this repository; no third-party assets.
