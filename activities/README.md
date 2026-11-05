# Bundled activities

The built-in marketplace content (ADR-002, ADR-003), listed in `.lullshift/marketplace.json` from Sprint 08.

| Folder | Contents | Sprint |
|---|---|---|
| `break/` | Break activities, starting with Step Away | 03 |
| `games/` | Code activities, mostly on the grid, card and arcade kits. Lantern Pond is the reference game | 03, 06, 10 |
| `lib/` | Shared files copied into activities, such as the bridge client `lullshift-bridge-v1.js` (ADR-012) | 03 |
| `engines/` | First-party engines that render content packs | 07 |
| `content/` | Code-free content packs, grouped by engine | 07, 10 |

Every item must pass the activity gates in `sprints/DEFINITION_OF_DONE.md`.
