# ADR-016: Engines, data schemas and content packs

- **Status:** Accepted (Sprint 07). Provisional in Sprint 05.
- **Date:** 2026-11-05
- **Owner:** Implementation lead
- **Related:** ADR-002 (marketplaces), ADR-003 (activity model), ADR-012 (sandbox and bridge), ADR-017 (CLI)

## Context

Content packs (ADR-003) are code-free data for a named engine. Sprint 05 set the smallest engine contract so the CLI could validate and simulate a sample pack. Sprint 07 ships the first set of engines and about fifty packs, and the desktop app runs packs. That needs the remaining decisions: how the app identifies, serves and checks a pack, how long engine majors are kept, what files a pack may carry, and how engines appear in a marketplace index.

## Decision

### Engines

- **An engine is a code activity with one more file.** Its folder holds a normal `kind: "app"` manifest and its files, plus `engine.json`:
  ```json
  { "schemaVersion": 1, "id": "lullshift.cards", "major": 1, "dataSchema": "data.schema.json" }
  ```
  - `id` uses the manifest id pattern; `major` is a positive integer; `dataSchema` is a JSON Schema 2020-12 file beside `engine.json` (no sub-folder).
  - A pack names the engine as `id@major`.
- **Data schemas are strict.** Objects that define a shape set `additionalProperties: false`, strings have `maxLength`, arrays have `maxItems`.
- **Data schemas use the host's keyword subset.** The app checks pack data in Rust (below; since Sprint 12 with the `jsonschema` crate behind a fail-closed gate, see the amendment). It accepts `type`, `enum`, `const`, `properties`, `required`, `additionalProperties`, `minProperties`, `maxProperties`, `items`, `minItems`, `maxItems`, `uniqueItems`, `minLength`, `maxLength`, `pattern`, `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf`, `allOf`, `anyOf`, `oneOf`, `not`, `if`/`then`/`else`, `$ref` to `#` or `#/$defs/<name>`, and annotations. Patterns keep their JavaScript `u`-flag meaning; `\b`, lookaround and backreferences are refused. An engine whose schema uses anything else, refers to itself in a loop or unfolds to more than 2,000 parts can't run packs in the app.
- **Where engines are found.** First-party engines live in `activities/engines/<name>/`. The CLI searches there plus any `--engines` folders. The app bundles them like other activities.

### Packs in the app

- **Identity is the pack.** A pack is listed as its own activity card with the name, description, category, preview and help from the pack's manifest. Its URL is `lullact://localhost/<pack id>/index.html` (`http://lullact.localhost/<pack id>/…` on Windows), so two packs never share an address. Snapshots, storage, time played and the paused-view summary are all keyed by the pack id.
- **The pack's path serves the engine's files.** The scheme handler maps `/<pack id>/<file>` to the engine's folder. The pack's own files (data, preview, notes) are never served; the preview reaches the UI only through `activity_preview` as a `data:` image.
- **Engines are hidden.** They are not listed, have no URL of their own, and every activity command (`activity_info`, snapshots, storage, play time, `activity_content`) refuses an engine id.
- **The host re-validates before starting.** `activity_content(id)` resolves `id@major` to exactly one bundled engine with a usable schema, reads the data (at most 256 KiB), parses it and validates it. Results are cached for the app's life (bundled files can't change while it runs). The UI calls it before creating the iframe; on failure it shows a calm card ("This pack's data doesn't match its engine, so Lullshift didn't start it. Nothing was lost.") with the plain-text reason, and never creates the iframe. The scheme handler independently refuses to serve engine files under a pack id whose check failed.
- **Why Rust, not Ajv in the UI.** Ajv compiles schemas with `new Function`; the app CSP (`script-src 'self'`, no `unsafe-eval`) forbids that and must not be loosened. A precompiled Ajv build would need regenerating for every engine. Rust tests run every engine fixture and the shared Ajv/Rust corpus through the host's check, so it can't silently disagree with Ajv. (Sprint 07 used a hand-written subset validator; Sprint 12 replaced it with an established crate, see the amendment.)
- **The engine receives the data in `init`.** `init.content` (ADR-012) holds the pack's checked data for packs and is `null` for every other activity. Engines still treat it as untrusted text. `connect-src 'none'` and the rest of the activity CSP are unchanged.
- **Permissions.** A pack may use `activity.storage` only if both the pack (what the user is shown) and its engine (the code that would use it) declare it.

### Messages creators see

- `lullshift validate`, `pnpm check` (`tests/activities/check-engine-compat.mjs`) and the app word schema problems the same way: file, data path, plain message, e.g. `deck.json: cards[3].front is longer than 200 characters`. The TypeScript formatter is `friendlyErrors` in `@lullshift/agent-protocol`; the app's wording lives in `data_schema.rs`.

### Pack media (v1)

- **v1 packs are data plus a preview.** A pack holds `manifest.json`, its data file, an optional preview image (PNG, JPEG, WebP or SVG, at most 256 KiB) and notes (`.md`, `.txt`). Nothing else; `validate` refuses other files. The app serves only the engine's files, so pack media would never reach the player anyway.
- Pack images or audio need a later ADR covering how an engine asks for a pack file, the allowed types and sizes, and how the scheme handler serves them without widening the sandbox.

### Compatibility and retirement

- A change that would make any valid existing pack invalid, or render it differently in a way that matters, needs a new major.
- **A new major ships beside the old one** as a separate engine folder (for example `activities/engines/quiz-v2/` with `"major": 2`). Each major keeps its own fixtures in `tests/fixtures/engines/<id>@<major>/`.
- **No major is removed within v1.** The app keeps every major that any bundled or installed pack uses. Retiring a major after v1 needs an ADR and a release that first stops listing packs on it.
- Every engine has `valid/` and `invalid/` fixtures; the compat check requires valid fixtures to pass and invalid ones to fail, and every bundled pack to pass its engine. Old fixtures are never edited to fit a schema change: if one stops passing, the change needs a new major.

### Marketplace index

- Engines are listed in `.lullshift/marketplace.json` like any activity (by manifest id and source path), so a pack's `id@major` can resolve to an engine that a marketplace provides and so installs can pin both. The app recognises an engine by its `engine.json` and never shows it as a card. No schema change to `MARKETPLACE_V1` is needed.
- Installing third-party packs and engines is ADR-002 work (not in Sprint 07). When it lands, a pack installs only if its `id@major` resolves to a bundled or installed engine, and the host re-validates the data at install and before every start.

## Amendments (Sprint 12)

### Plain text refuses U+2028 and U+2029 (S12-T10, decision 14)

- Every engine's shared plain-text pattern now also refuses the line separator U+2028 and the paragraph separator U+2029, changed in all ten engine schemas in one pass. The full set is: U+0000-001F, U+007F-009F, U+00AD, U+061C, U+180E, U+200B-200F, U+2028, U+2029, U+202A-202E, U+2060-2064, U+2066-206F and U+FEFF. The pattern is written with JSON escapes in every schema, so the characters are visible in review.
- **Why no new major.** This tightens the rules, which "Compatibility and retirement" would normally treat as a new major. The owner chose a one-pass ban instead (decision 14): the characters render as invisible line breaks and no bundled pack uses them. An installed third-party pack whose text contains one stops running after the update and is listed with the "can't run on its engine" problem until its author fixes it; nothing is deleted. Older fixtures are unchanged and still pass; each engine has two new invalid fixtures (`line-separator.json`, `paragraph-separator.json`), and the shared Ajv/Rust corpus has plain-text cases for both characters.

### The host checks pack data with the `jsonschema` crate (S12-T12, decision 13)

**Decision.** The custom validator in `crates/marketplace/src/data_schema.rs` is replaced by [`jsonschema`](https://crates.io/crates/jsonschema) **0.60.2**, pinned exactly (`=0.60.2`) with `default-features = false`. The module keeps three layers of its own around it, because engine schemas are only first-party today but marketplace engines are code activities that the third-party gate (ADR-015) could one day let in:

1. **A fail-closed gate before the crate.** `check_schema` keeps the keyword allow-list above, refuses every `$ref` except `#` and `#/$defs/<name>`, allows `$schema`, `$id` and `$defs` only at the top, requires `$id` (if present) to be a `urn:` name so no reference can resolve against a URL, refuses `$ref` cycles, and refuses schemas that unfold (every `$ref` replaced by its target, `enum`/`const`/`required` entries counted) to more than **2,000** parts or nest more than 32 deep. The bundled schemas unfold to 11 to 37 parts.
2. **No fetching, ever.** The crate is built without its `resolve-http` and `resolve-file` features (so `reqwest`, `rustls` and the file retriever are not even compiled in) and is configured with `.offline()`; a remote or file `$ref` fails at build time (tested). Format assertions are off.
3. **Our own `pattern`.** The crate's ECMA-262 translation lets `.` match `\r`, U+2028 and U+2029 (three corpus cases fail with it). The host registers `pattern` as a custom keyword (custom keywords override built-ins in this crate) backed by the existing JavaScript-`u`-flag translation on the `regex` crate with a 1 MiB compiled-size limit, which keeps ASCII `\d`/`\w`, JavaScript `\s` and `.`, and refuses `\b`, lookaround, backreferences and flag groups.

**Resource limits.** The crate has no work budget: in testing, a 28-level `anyOf`/`$ref` ladder (2^28 paths, legal JSON Schema) ran for over 30 seconds on one number. The old checker stopped such schemas with a 500,000-step budget at validation time; the new host refuses them when the schema is checked, by the unfolded-size limit and the cycle refusal above. With no cycles, each schema part is applied at most once per data value, and data stays capped at 256 KiB (checked before parsing), so the work is bounded by schema size times data size. `uniqueItems` hashes items (no quadratic scan). At most 20 problems are reported. The regex engine is linear-time. The release profile uses `panic = "abort"`; the crate's own `catch_unwind` around regex panics therefore does not apply, but the host's `pattern` does not go through it.

**Messages.** The crate's error kinds are mapped to the same plain words as before (`cards[3].front is longer than 200 characters`); the order of several problems can differ from Sprint 07, the wording does not. A schema the gate refuses never reaches the crate, and `validate` re-runs the gate, so a schema that slips past an engine-load check still fails closed.

**Evaluation (2026-11-06).**

| | `jsonschema` 0.60.2 (chosen) | `boon` 0.6.1 | Keep the custom subset |
|---|---|---|---|
| Maintenance | Very active: 0.60.0 to 0.60.2 released 22 to 26 Oct 2026; about 117 M downloads | Last release 7 Jan 2025 (22 months); about 0.75 M downloads | Ours to maintain |
| License | MIT | MIT OR Apache-2.0 | Apache-2.0 |
| Draft 2020-12 | Yes (`Draft::Draft202012`) | Yes | Subset only |
| Regex vs Ajv on the shared corpus | 3 of 45 differ (`.` and line terminators), fixed by our `pattern` keyword | 4 of 45 differ (`.` and U+3000 in `\s`); `[a&&b]`/`[a--b]` differ too | 0 |
| All 115 engine fixtures | Agree | Agree | Agree |
| Remote `$ref` | Refused offline; HTTP/file retrievers not compiled | Refused with a custom loader; a file loader is in the default build | Refused |
| Work budget | None (guarded by the gate above) | None | 500,000 steps |
| New crates in `Cargo.lock` | 23 (`cargo tree -e normal` on a stand-alone project: 63 crates) | about 55 crates stand-alone, including `url` and `idna` | 0 |

Both crates are memory-safe Rust; neither generates code at run time, so the CSP is unchanged. `jsonschema` was chosen for maintenance and the smaller regex gap; its 0.x line breaks often, which is why it is pinned exactly and why every bump must re-run `cargo test -p lullshift-desktop` (fixtures and corpus) and `node tests/activities/check-engine-compat.mjs`.

**Dependency audit.** All 23 crates added to `Cargo.lock` are under MIT, Apache-2.0, MIT-0 or BSD-2-Clause-or-Apache-or-MIT terms, compatible with Apache-2.0. `cargo deny` and `cargo audit` are not part of `pnpm check` and were not installed, so the versions were checked against the OSV database (which includes RustSec) on 2026-11-06: no known advisories for any of them. Enabling the crate also turns on `serde_json`'s `float_roundtrip` feature for the workspace (exact float parsing; no behaviour the host relies on changes).

## Consequences

- About fifty packs run in the app with no code of their own, each with separate progress.
- Engine authors are limited to the host's schema subset; the compat check says so plainly when a keyword isn't supported.
- Three places speak the same error wording (CLI, compat check, app). Tests on each side pin the shared phrases.
- Pack media is deferred; engines that want pictures draw them themselves.
