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

- **Identity is the pack.** A pack is listed as its own activity card with the name, description, category, preview and help from the pack's manifest. Its URL is `lullact://localhost/<launch key>/<pack id>/index.html` (`http://lullact.localhost/<launch key>/<pack id>/…` on Windows; the key is per launch, see ADR-012), so two packs never share an address. Snapshots, storage, time played and the paused-view summary are all keyed by the pack id.
- **The pack's path serves the engine's files.** The scheme handler maps `/<pack id>/<file>` to the engine's folder. The pack's own files (data, preview, notes) are never served, with one exception since Sprint 12: the pictures listed in its `images` are served at `/<pack id>/images/<name>` (see the amendment); the preview reaches the UI only through `activity_preview` as a `data:` image.
- **Engines are hidden.** They are not listed, have no URL of their own, and every activity command (`activity_info`, snapshots, storage, play time, `activity_content`) refuses an engine id.
- **The host re-validates before starting.** `activity_content(id)` resolves `id@major` to exactly one bundled engine with a usable schema, reads the data (at most 256 KiB), parses it and validates it. Results are cached for the app's life (bundled files can't change while it runs). The UI calls it before creating the iframe; on failure it shows a calm card ("This pack's data doesn't match its engine, so Lullshift didn't start it. Nothing was lost.") with the plain-text reason, and never creates the iframe. The scheme handler independently refuses to serve engine files under a pack id whose check failed.
- **Why Rust, not Ajv in the UI.** Ajv compiles schemas with `new Function`; the app CSP (`script-src 'self'`, no `unsafe-eval`) forbids that and must not be loosened. A precompiled Ajv build would need regenerating for every engine. Rust tests run every engine fixture and the shared Ajv/Rust corpus through the host's check, so it can't silently disagree with Ajv. (Sprint 07 used a hand-written subset validator; Sprint 12 replaced it with an established crate, see the amendment.)
- **The engine receives the data in `init`.** `init.content` (ADR-012) holds the pack's checked data for packs and is `null` for every other activity. Engines still treat it as untrusted text. `connect-src 'none'` and the rest of the activity CSP are unchanged.
- **Permissions.** A pack may use `activity.storage` only if both the pack (what the user is shown) and its engine (the code that would use it) declare it.

### Messages creators see

- `lullshift validate`, `pnpm check` (`tests/activities/check-engine-compat.mjs`) and the app word schema problems the same way: file, data path, plain message, e.g. `deck.json: cards[3].front is longer than 200 characters`. The TypeScript formatter is `friendlyErrors` in `@lullshift/agent-protocol`; the app's wording lives in `data_schema.rs`.

### Pack media (v1)

- **Sprint 07: packs were data plus a preview.** A pack held `manifest.json`, its data file, an optional preview image (PNG, JPEG, WebP or SVG, at most 256 KiB) and notes (`.md`, `.txt`). Nothing else; `validate` refused other files.
- **Sprint 12: packs may also hold still pictures** listed in the manifest's `images`. See the amendment "Pictures in content packs" below. Audio and other media still need a later ADR.

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

### Pack data uses the host's full plain-text set (S12-T14 security review, LOW-2; decision 18.7)

- **The pattern.** Every engine's plain-text pattern is now the host's hidden-character class, the same one the manifest and marketplace schemas use (ADR-015 amendment, item 7): it adds U+034F, U+115F-1160, U+17B4-17B5, U+180B-180D, U+3164, U+FFA0, U+FFF9-FFFB, U+1BCA0-1BCA3, U+1D173-1D17A and the tag characters U+E0000-E007F to the set above. It uses `\u{...}` escapes, so it needs the `u` flag, which Ajv (`unicodeRegExp`) and the app's translation both use. The pattern still starts `^[^\u0000-\u001F`, so problems keep their plain-text wording.
- **Combining marks.** "More than four combining marks in a row" needs a lookahead, which the app's pattern translation refuses on purpose. So it is not in the engine pattern. Instead, after the schema, every string in the pack data (values and field names) goes through the host's own check: `marketplace::pack_text::pack_data_problems` at install, for bundled and installed packs at load, and for the engine fixtures. `packTextProblems` in `@lullshift/activity-sdk` does the same in `lullshift validate` and `tests/activities/check-engine-compat.mjs`, with the same words (`cards[3].front has more than four combining marks in a row, which isn't allowed`). `data_schema.rs` is unchanged.
- **Tests.** The shared corpus `tests/fixtures/plain-text/cases.json` now also runs against every engine's pattern, through the app's translation (Rust) and Ajv (SDK test and the compat script): the pattern must refuse exactly the hidden cases, and pattern plus walk must match the host on every case. Each engine has three new invalid fixtures (`tag-character.json`, `hangul-filler.json`, `combining-mark-run.json`).
- **Why no new major.** Same reasoning as decision 14 above: a one-pass tightening, no bundled pack uses these characters, and an installed third-party pack that does stops running with the "can't run on its engine" problem until its author fixes it; nothing is deleted.

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

### Pictures in content packs (S12-T11)

**Decision.** A content pack may carry still PNG and WebP pictures. They are untrusted input from any marketplace, so the format, the checks and the serving path are kept narrow.

- **Declared in the manifest.** `images` (optional, content packs only) lists 1 to 20 unique paths of the form `images/<name>.png` or `images/<name>.webp`, where the name is 1 to 64 lowercase letters, digits and dashes and starts with a letter or digit. No subfolders, no `..`, no other extensions; an app manifest with `images` is invalid. The data refers to a picture by the same path (for Quick Quiz: `"image": {"src": "images/hexagon.png", "alt": "…"}`), and every data string that starts with `images/` must be listed, so nothing the engine asks for is unchecked.
- **Limits.** At most 20 pictures, 512 KiB each, 2048 × 2048 pixels on either side. 20 × 512 KiB keeps a pack's pictures at 10 MiB, inside the install size cap; 2048 pixels is sharp on any desktop window the activity pane can fill; the largest decoded buffer is 2048 × 2048 × 8 bytes (32 MiB, 16-bit RGBA), and one picture is decoded at a time.
- **No SVG, GIF or JPEG.** SVG is a document that can hold script and links; GIF is mostly animation; leaving JPEG out keeps one decoder fewer (and its EXIF and ICC handling) out of the trusted base. The preview image rules (ADR-012) are unchanged.

**Checks** (`crates/pack-media`, used by the installer in `crates/marketplace`, the app for bundled and installed packs, and mirrored in the CLI):

1. Path and size, then the leading bytes must match the extension (an SVG, HTML page or script renamed `.png` is refused).
2. A container walk with an allow-list. PNG: signature, `IHDR` first, every chunk CRC checked, only `PLTE`, `tRNS`, `gAMA`, `cHRM`, `sRGB`, `sBIT`, `bKGD`, `pHYs` beside `IDAT`; refused are animation (`acTL`, `fcTL`, `fdAT`), text and metadata (`tEXt`, `zTXt`, `iTXt`, `eXIf`, `tIME`), colour profiles (`iCCP`), unknown chunks and any byte after `IEND`. WebP: the RIFF size must match the file exactly; `VP8 ` or `VP8L`, or `VP8X` without the animation, ICC, EXIF or XMP flags (with `ALPH`); refused are `ANIM`, `ANMF`, `EXIF`, `XMP `, `ICCP` and unknown chunks.
3. Width and height are read from the header and checked against the limit **before** any decoder allocates; the decoder's own header must agree.
4. A full decode with `png` 0.18.1 and `image-webp` 0.2.4 (image-rs crates with `#![forbid(unsafe_code)]`, pinned exactly), into one output buffer sized by the decoder and refused if over 32 MiB. `png` also runs under its own allocation limit (`png::Limits`); `image-webp` applies its memory limit only to metadata reads, which the host never makes, so for WebP the bound comes from the dimension check in step 3, which runs before the decoder is created. Any decode error refuses the picture.

**Dependencies.** `png` 0.18.1 and its inflate stack (`fdeflate`, `flate2`, `miniz_oxide`, `simd-adler32`, `crc32fast`) were already in `Cargo.lock` through Tauri; the only new crates are `image-webp` 0.2.4 and `quick-error` 2.0.1 (both MIT OR Apache-2.0). `png` and `image-webp` forbid `unsafe` themselves; `simd-adler32` uses SIMD intrinsics. The OSV database (including RustSec) had no advisories for any crate in `cargo tree -p pack-media` on 2026-11-06; `cargo deny` and `cargo audit` were not installed.

**Not re-encoded.** The bytes that pass are the bytes that are hashed at install, verified each session and served. Re-encoding would put an encoder in the trusted path and change the digest; refusing metadata chunks has the same privacy effect as stripping them.

**Serving.** For a pack, `/<pack id>/images/<name>` is answered only from the pack's own listed pictures (bundled files, or for installed packs the bytes verified this session), with `Content-Type: image/png` or `image/webp` from the checked kind, `X-Content-Type-Options: nosniff` and the unchanged activity CSP. Anything else under `images/` is a 404, and an engine's own `images/` folder is never served for a pack (the CLI refuses engines with one). The activity CSP is not changed: `img-src 'self' data:` already limits pictures to the activity's own origin, and `connect-src 'none'` still blocks every network request. A pack whose pictures fail is refused at install, and a bundled or installed pack whose pictures fail at start is not started (`image_invalid`: "One of this pack's pictures didn't pass Lullshift's checks, so it didn't start.").

**CLI.** `lullshift validate` applies the same path, size, signature and chunk rules, checks every PNG CRC and inflates the pixel data with a size bound, requiring exactly the size the header implies. Node has no WebP decoder and the CLI adds no dependency for one, so WebP is checked structurally only there; the full decode happens at install and in the app. `simulate` and `serve` serve the pack's listed pictures beside the engine's files, as the app does.

**Quick Quiz.** A question may have `image: {src, alt}`; `alt` is required plain text (1 to 200 characters). The engine shows the picture in a `figure` with the alt text; if it fails to load, it shows "Picture not available. It shows: <alt>". Every question must still be answerable from the alt text alone. The bundled example is "Shapes and Sides" (`quiz-shapes-and-sides`), whose six pictures are generated by a small program, not drawn or copied.

## Amendments (Sprint 13)

### Rounds: optional `roundSize` for quiz, teaser and calm cards (S13-T09, ADR-028 section 10)

**Schema.** `lullshift.quiz@1`, `lullshift.teaser@1` and `lullshift.cards@1` gain an optional top-level `"roundSize": {"type": "integer", "minimum": 3, "maximum": <the item list's maxItems>}` (40, 40 and 60). Existing packs stay valid and play unchanged, so the majors stay 1; a host from before this change would refuse packs that use the field (`additionalProperties: false`), which is acceptable only because v1 has not shipped (ADR-028 section 10). Old fixtures are unchanged; new `round-size*` fixtures cover the range, fractions, text and `null`, and the shared corpus gained whole-number cases (including `8.0`, which both Ajv and the app accept).

**"At most the item count" is not in the schema.** The host subset has no `$data` reference; the only way to say it would be an `if`/`then` ladder with one branch per possible count (up to 59), which is hard to read and reports dozens of errors for one mistake. Instead: each engine clamps (a `roundSize` at or above the item count plays the whole pack, exactly as without it), `lullshift validate` warns (`roundSizeProblem` in `@lullshift/activity-sdk`), and the compat check makes it an error for bundled packs. A pack that breaks the rule can only show its whole pack, so nothing unsafe is reachable.

**Engines.** Each round draws `roundSize` items with the engine's seeded generator (mulberry32, as Quick Quiz already used), first from items not in the snapshot's `seen` list, then from the least recently seen. Teasers and cards keep the written order within a round; the quiz shuffles only with `shuffle`. The snapshot gains `seen` (each index at most once, so at most the item count) and, for teaser and cards, `seed`; the round itself is rebuilt from them, so a resume shows the same round at the same place. A snapshot that doesn't fit (including one saved before the pack set `roundSize`) starts a new round. Without `roundSize` the snapshot shape and play are unchanged. Calm Cards' rules moved into a `logic.js`, like the other engines. Bundled packs with 12 or more items set it: today only `wonder-facts` (12 cards, `roundSize: 6`).

## Consequences

- About fifty packs run in the app with no code of their own, each with separate progress.
- Engine authors are limited to the host's schema subset; the compat check says so plainly when a keyword isn't supported.
- Three places speak the same error wording (CLI, compat check, app). Tests on each side pin the shared phrases.
- Packs may carry still PNG and WebP pictures (Sprint 12); the Quick Quiz engine shows them with alt text. Audio and animation remain deferred.
- Two image decoders (`png`, `image-webp`) are now part of the trusted computing base for third-party pack data; both forbid `unsafe` code and are pinned exactly.
