---
name: activity-creator
description: Create or revise Lullshift games and activities (custom code, code on a kit, or a code-free content pack for an engine) from natural-language requests using the manifest, SDK, simulator and CLI. Use for any AI-generated Lullshift activity.
---

# Lullshift Activity Creator

1. Read `reference/ACTIVITY_MANIFEST_V1.schema.json`, ADR-003, the activity sections of `docs/TECHNICAL_ARCHITECTURE.md`, and `creator/ACTIVITY_CREATOR_PROMPT.md`.
2. Choose the lightest tier that fits: **content pack** if an existing engine can express it (preferred), else **code on a kit** (grid, card, arcade), else **custom code**. Ask only for missing essential behavior.
3. Code activities: static HTML/CSS/bundled JS, no runtime CDN, no Node/native modules in shipped files. Inventory and license-check every dependency and asset.
4. Content packs: `manifest.json` with `kind: "content"`, `engine: "<id>@<major>"`, and a `data` JSON file valid against that engine's data schema. No code.
5. Produce manifest, sources or data, assets, README, licenses and tests. Use original names and art; never recreate a specific branded game.
6. Implement (or rely on the engine/kit for) pause/resume/saveState/restoreState. Never access agent data, keys or source code.
7. Run the real CLI `validate`, `simulate` (immediate needs_attention, duplicates, out-of-order, forced pause, close/crash/reload, offline) and `pack` when they exist. If the toolchain is missing, say so and **do not claim passing tests**.
8. Publishing means pushing the activity folder to a Git repo with a `.lullshift/marketplace.json` entry (see `/marketplace-publishing`). Never push, create repos or open PRs without explicit user confirmation.

**Security rule:** generated code is untrusted. Never work around sandbox restrictions; if functionality needs a privileged API, stop and propose an ADR.
