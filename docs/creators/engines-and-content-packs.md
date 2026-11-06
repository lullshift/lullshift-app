# Engines and content packs

A **content pack** is data with no code. It names a bundled **engine**, and the engine renders it (ADR-016). Packs are the easiest way to make something, and the only kind of third-party activity users can install in this release.

## What a pack contains

- `manifest.json` with `"kind": "content"`, `"engine": "<id>@<major>"` and `"data": "<file>.json"`
- the data file, at most 256 KiB, valid against the engine's `data.schema.json`
- a preview image (png, jpg, jpeg, webp or svg, at most 256 KiB)
- optional pictures for the engine to show, listed in `images` (see below)
- optional notes: `.md` or `.txt` files

Nothing else. `validate` refuses any HTML, JavaScript, CSS or other file, and so does the app's installer. Packs still need `help` in the manifest.

## Pictures

A pack can carry still pictures for an engine that shows them (today: Quick Quiz). List each one in the manifest:

```json
"images": ["images/hexagon.png", "images/pentagon.webp"]
```

- PNG or WebP only, directly in the pack's `images/` folder. Names use lowercase letters, digits and dashes (`images/rainy-street.png`). No SVG, GIF or JPEG, and no subfolders.
- At most 20 pictures, each at most 512 KiB and at most 2048 × 2048 pixels.
- Still pictures only: animated PNG or WebP is refused. So are pictures carrying text chunks, EXIF or XMP metadata, colour profiles or time stamps. Export without metadata (most editors have a "strip metadata" or "export for web" option).
- Every picture in `images/` must be listed, and every `images/...` path in the data must be listed. An unlisted picture is refused, not ignored.
- Every picture needs alt text in the data that says what it shows, and the question or step must make sense from the alt text alone. Players may not see the picture.

The app checks each picture before installing the pack and again before starting it: the bytes must really be a PNG or WebP (a renamed SVG or web page is refused), and the whole picture is decoded. A pack with a picture that fails doesn't install or start. Pictures are served only to the pack's own engine, with a fixed content type; nothing else in the pack is served. `validate` makes the same checks, except that it checks WebP files' structure without decoding them, so the app may still refuse a damaged WebP.

See `content/quiz/quiz-shapes-and-sides` for an example.

## How the app runs a pack

- Each pack appears as its own card in Activities, with its own name, preview, help, saved state and play time. Engines themselves are never listed.
- Before starting, the app resolves `id@major` to a bundled engine and checks the data against the engine's schema again. If either fails, the player sees "This pack's data doesn't match its engine" and the engine never starts.
- The engine receives the data as `content` in `onInit` and renders it as plain text.

## The bundled engines

All are `major` 1. Each schema is in `activities/engines/<folder>/data.schema.json`; `*` marks required fields. Packs live in `activities/content/<folder>/`.

| Engine | Folder | Data (top level) | Example pack |
|---|---|---|---|
| `lullshift.away@1` Away Prompts | `away` | `title*`, `intro`, `prompts*` (1 to 60: `text*`, `category` of water, walk, window, tidy, chat or rest), `shuffle`, `backAtDesk` | `content/away/away-look-outside` |
| `lullshift.breathing@1` Breathing Space | `breathing` | `title*`, `intro`, `phases*` (2 to 12: `kind*` inhale, hold, exhale or rest; `seconds*` 1 to 8, holds at most 4; `cue`), `rounds*` (1 to 20), `closing` | `content/breathing/breath-box-gentle` |
| `lullshift.cards@1` Calm Cards | `calm-cards` | `title*`, `intro`, `cards*` (1 to 60: `front*`, `back`) | `content/calm-cards/desk-stretches` |
| `lullshift.movement@1` Movement Routine | `movement` | `title*`, `intro`, `moves*` (1 to 20: `name*`, `steps*`, `seconds` or `reps`, `side`, `position`), `restSeconds`, `closing`, `wayOut*` | `content/movement/move-ankles-and-feet-seated` |
| `lullshift.pause@1` Guided Pause | `pause` | `title*`, `intro`, `steps*` (1 to 12: `text*`, `seconds*` 5 to 120, `look`), `closing` | `content/pause/pause-eye-rest` |
| `lullshift.quiz@1` Quick Quiz | `quiz` | `title*`, `intro`, `shuffle`, `questions*` (1 to 40: `prompt*`, `choices*` 2 to 5, `answer*` index, `explanation*`, `image` with `src*` and `alt*`) | `content/quiz/quiz-everyday-materials` |
| `lullshift.reflection@1` Reflection Prompts | `reflection` | `title*`, `intro`, `allowNotes`, `prompts*` (1 to 30: `text*`, `followUp`) | `content/reflection/reflect-end-of-day` |
| `lullshift.teaser@1` Brain Teasers | `teaser` | `title*`, `intro`, `teasers*` (1 to 40: `question*`, `hint`, `answer*`, `explanation*`, `kind`) | `content/teaser/teasers-gentle-logic` |
| `lullshift.toy@1` Creative Toy | `toy` | `title*`, `intro`, `palette*` (2 to 8 named colours), `mode*` (ripples, mosaic, stars or threads), `symmetry`, `brushSize`, `background` | `content/toy/toy-dusk-ripples` |
| `lullshift.typing@1` Typing Drills | `typing` | `title*`, `intro`, `mode` (accuracy or flow), `lines*` (1 to 40, printable ASCII, at most 120 characters each) | `content/typing/typing-calm-sentences` |

Text fields have length limits and refuse control and invisible formatting characters. The schema is the authority; when this table and a schema disagree, the schema wins. Engines with their own README (breathing, pause, quiz, reflection, teaser, toy, typing) describe play in more detail.

Write content in the calm, plain tone of [docs/CONTENT_STYLE_GUIDE.md](../CONTENT_STYLE_GUIDE.md). Breathing, movement and pause packs make no medical or health claims and always offer a way out.

## Creating a pack

```bash
pnpm lullshift create my-pack --kind content --engine lullshift.breathing@1 --name "My Pack" --id dev.you.my-pack
```

Without `--engine`, `create` uses `lullshift.cards@1`. The starter data file `deck.json` is shaped for Calm Cards; for any other engine, replace it with data for that engine's schema and update `data` in the manifest. The [tutorial](tutorial-content-pack.md) walks through this for the quiz engine.

`validate` names the file, the place in the data and the problem, for example `deck.json: cards[3].front is longer than 200 characters`.

## Compatibility promise

An engine's data schema is a public contract. A change that would make an existing valid pack invalid, or change how it plays in a way that matters, needs a new major version, which ships beside the old one. No major is removed within v1. Each engine has compatibility fixtures in `tests/fixtures/engines/<id>@<major>/{valid,invalid}/`, checked by `node tests/activities/check-engine-compat.mjs` (part of `pnpm check`).

## Writing a new engine

Engines are first-party: they ship in this repository, by pull request. An engine is an `app` activity with one more file, `engine.json`:

```json
{ "schemaVersion": 1, "id": "lullshift.quiz", "major": 1, "dataSchema": "data.schema.json" }
```

Data schemas use JSON Schema 2020-12 with a supported subset. Objects set `"additionalProperties": false`, strings have `maxLength` and arrays have `maxItems`. The full list of supported keywords is in the [SDK guide](../../packages/activity-sdk/README.md#content-packs-and-engines). Even though the app has checked the data, an engine treats `content` as untrusted: it renders text with `textContent`, never `innerHTML`, and keeps its own bounds checks. With no pack, an engine shows a small built-in sample.
