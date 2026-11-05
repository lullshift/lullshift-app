# Bundled activities

The built-in marketplace content (ADR-002, ADR-003), listed in `.lullshift/marketplace.json` from Sprint 08.

| Folder | Contents | Sprint |
|---|---|---|
| `break/` | Break activities, starting with Step Away | 03 |
| `games/` | Code activities, mostly on the grid, card and arcade kits. Lantern Pond is the reference game | 03, 06, 10 |
| `engines/` | First-party engines that render content packs (ADR-016). Calm Cards (`lullshift.cards@1`) is provisional | 05, 07 |
| `content/` | Code-free content packs, grouped by engine. Desk Stretches is the provisional sample pack (ADR-016) | 05, 07, 10 |

The bridge client `lullshift-bridge-v1.js` (ADR-012) is copied into every code activity. Its canonical copy lives in `packages/activity-sdk/bridge/`; a test checks that every copy is identical.

Every item must pass the activity gates in `sprints/DEFINITION_OF_DONE.md`.
