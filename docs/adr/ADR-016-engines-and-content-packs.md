# ADR-016: Engines, data schemas and content packs

- **Status:** Provisional (Sprint 05). To be completed in Sprint 07, when the first engines ship.
- **Date:** 2026-11-05
- **Owner:** Implementation lead
- **Related:** ADR-003 (activity model), ADR-012 (bridge), ADR-017 (CLI)

## Context

Content packs (ADR-003) are code-free data for a named engine. Sprint 05 must validate and simulate a sample content pack (S05-A01). That needs the smallest engine contract: where engines live, how a pack's data is checked, and how an engine receives it. Retirement policy and the full set of engines are Sprint 07 work.

## Decision (provisional)

- **An engine is a code activity with one more file.**
  - Its folder holds a normal `kind: "app"` manifest and its files, plus `engine.json`:
    ```json
    { "schemaVersion": 1, "id": "lullshift.cards", "major": 1, "dataSchema": "data.schema.json" }
    ```
  - `id` uses the manifest id pattern. `major` is a positive integer. `dataSchema` is a JSON Schema (2020-12) file inside the engine folder.
  - A pack names the engine as `id@major`.
- **Where engines are found.** First-party engines live in `activities/engines/<name>/`. The CLI searches there, plus any folders given with `--engines`. The host bundles them like other activities.
- **Data is checked twice.** The CLI validates a pack's data file against the engine's data schema with the shared Ajv setup, and the host checks it again before running the pack.
  - A data schema must be strict: `additionalProperties: false` on objects, `maxLength` on strings, `maxItems` on arrays.
  - Data files are at most 256 KiB.
- **The engine receives the data in `init`.** The bridge `init` message gains `content` (ADR-012), which holds the pack's validated data for engines and is `null` for every other activity.
  - Engines never read files, so `connect-src 'none'` stays.
  - Pack media (images, audio) is out of scope until Sprint 07.
- **Saves belong to the pack.** Snapshots and storage are keyed by the pack id, not the engine id, so two packs on one engine keep separate progress.
- **Compatibility.** A change that would make any valid existing pack invalid, or render it differently in a way that matters, needs a new major. The old major keeps running installed packs (ADR-003).

## Sprint 05 scope

- One small first-party engine, `lullshift.cards@1` (Calm Cards): a deck of short cards read one at a time. It exists to prove the contract and is written to the same standard as any bundled activity.
- One sample pack for it.
- The desktop host does not list or run content packs until Sprint 07. The CLI's `validate` and `simulate` cover them now.

## Open for Sprint 07

- Engine retirement and how many majors are kept.
- Pack media and its limits.
- How engines are listed in the marketplace index.
