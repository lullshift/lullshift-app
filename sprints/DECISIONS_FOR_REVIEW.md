# Decisions and assumptions for product-owner review

Logged while the sprints ran unattended (from 2026-11-05 evening). Each item states what was decided and why, and what changes if you decide otherwise. I'll walk through them with you one by one.

| # | Sprint | Topic | Status |
|---|---|---|---|
| 1 | 05 | OpenCode adapter | open |
| 2 | 05 | Low-risk summary-label spoofing (L4) left unfixed | open |
| 3 | 05 | Sprint 05 commit split | open |
| 4 | 06 | How kits ship (ADR-019) | open |
| 5 | 06 | ADR number for kits | open |
| 6 | 06 | The first eight games | open |
| 7 | 06 | Built-in marketplace lists engines | open |

## 1. OpenCode adapter (Sprint 05)

You asked about Cursor, Codex and OpenCode. Cursor and Codex are in Sprint 10. OpenCode is not in the roadmap or PRD. **Assumption:** nothing changed; I did not add it. **Question:** add an OpenCode adapter to Sprint 10 next to Cursor and Codex?

## 2. Summary labels can mimic host labels (Sprint 05, security review L4)

An activity's paused-view stat label can say "Played" or "In total", like the host's own stats. It is look-alike text only, with no access gained. **Decision:** left as is and recorded as a low risk. **Alternative:** reserve host label words, or draw activity stats in a visually separate group.

## 3. Sprint 05 commits (Sprint 05)

You approved committing. I made five commits by behavior, staging individual hunks where files mixed two features. Only the final tree was fully checked (`pnpm check` all green). The intermediate commits were not built one by one. **Question:** fine, or do you want every commit to build on its own in future?

## 4. How kits ship (Sprint 06, ADR-019)

The architecture says kits are TypeScript libraries "compiled into the activity bundle", but bundled activities have no build step and the sandbox blocks module scripts. **Decision:** kits are written in TypeScript; a deterministic generator emits one classic-script file per kit (`lullshift-grid-kit-v1.js` and so on), committed and checked for drift by `pnpm check`. Activities copy that file in, like the bridge. **Alternative:** introduce a bundler (for example esbuild) and a build step for activities.

## 5. ADR number for kits (Sprint 06)

`TECHNICAL_ARCHITECTURE.md` reserved ADR-018 for app update signing (Sprint 11) and ADR-014/015 for Git and marketplaces (Sprint 08). **Decision:** kits use ADR-019 so the reserved numbers stay as planned.

## 6. The first eight games (Sprint 06)

Sprint 06 needs at least eight original games across the kits, puzzle and turn-based first. **Decision:** generic mechanics with original names and art, no branded look:

| Game | Kit | Mechanic |
|---|---|---|
| Moss Paths | grid | rotate tiles so water reaches every garden bed |
| Fog Garden | grid | reveal tiles using neighbour-count hints (deduction) |
| Stone Slide | grid | sliding-tile picture puzzle |
| Petal Grid | grid | Latin-square logic puzzle, daily seed |
| Ridge Walk | card | patience: play a card one higher or lower onto the pile |
| Tide Pairs | card | memory pairs |
| Firefly Catch | arcade | move a jar to catch slow fireflies |
| Kite Drift | arcade | one-button glide through gentle gaps |

Lantern Pond stays as written (no kit) for now. **Alternatives:** different games, or porting Lantern Pond onto the grid kit.

## 7. Engines in the built-in marketplace (Sprint 06)

`.lullshift/marketplace.json` lists every bundled folder, including the Calm Cards engine, so content packs can name an installed engine. The app does not show engines as cards (ADR-016). **Question:** keep engines listed, or mark them differently once the marketplace UI exists (Sprint 08)?
