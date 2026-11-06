# Tutorial: your first content pack

This builds a three-question quiz pack for the bundled Quick Quiz engine (`lullshift.quiz@1`), in a folder outside the Lullshift repository. It takes about ten minutes. The commands and output below were run on Windows 11 with Node.js 24 and pnpm 12.6.0, and `simulate` used Microsoft Edge.

You need a clone of this repository with `pnpm install` done (see [CONTRIBUTING.md](../../CONTRIBUTING.md)) and Edge, Chrome or Chromium for `simulate`.

`pnpm lullshift` runs from the repository root but resolves folders from where you typed the command, so you can use `pnpm --dir <repo> lullshift ...` from anywhere. The examples use `~/lullshift-work` for your work folder and `~/src/lullshift-app` for the clone; use your own paths.

## 1. Create the pack

```bash
cd ~/lullshift-work
pnpm --dir ~/src/lullshift-app lullshift create tea-quiz --kind content --engine lullshift.quiz@1 --name "Tea Quiz" --author "You" --id dev.you.tea-quiz
```

`create` writes `manifest.json`, `deck.json` and `README.md`, then validates the folder. The starter `deck.json` is shaped for the Calm Cards engine, so with the quiz engine it fails at once:

```text
  error   deck.json: questions is missing
  error   deck.json: cards isn't a known field
  warning no preview image: cards and the paused view will show none (add "preview")
✗ Tea Quiz (dev.you.tea-quiz@0.1.0): 2 error(s), 1 warning(s)
```

That is expected. The errors name the file and the place in the data.

## 2. Write the data

Delete `deck.json` and create `quiz.json` that matches the quiz engine's schema (`activities/engines/quiz/data.schema.json`):

```json
{
  "title": "Tea Quiz",
  "intro": "Three light questions about tea.",
  "questions": [
    {
      "prompt": "Green and black tea come from which plant?",
      "choices": ["Camellia sinensis", "Mentha spicata", "Aspalathus linearis"],
      "answer": 0,
      "explanation": "Both come from Camellia sinensis. Processing makes the difference."
    },
    {
      "prompt": "Which step mainly turns tea leaves black?",
      "choices": ["Steaming", "Oxidation", "Freezing"],
      "answer": 1,
      "explanation": "Black tea is fully oxidised. Green tea is heated early to stop oxidation."
    },
    {
      "prompt": "Rooibos is a true tea.",
      "choices": ["True", "False"],
      "answer": 1,
      "explanation": "Rooibos comes from a different plant, so it is an herbal infusion."
    }
  ]
}
```

`answer` is the zero-based index of the right choice.

## 3. Add a preview image

Create `preview.svg` with your own art (png, jpg, jpeg, webp or svg, at most 256 KiB):

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 100"><rect width="160" height="100" rx="12" fill="#e8f0ea"/><circle cx="80" cy="54" r="26" fill="#9cc3a6"/><rect x="104" y="44" width="14" height="18" rx="7" fill="none" stroke="#9cc3a6" stroke-width="5"/></svg>
```

## 4. Update the manifest

In `manifest.json`, set `data`, `preview`, the description and the help. The rest stays as `create` wrote it:

```json
  "description": "Three light questions about tea.",
  "category": "interactive",
  "data": "quiz.json",
  "preview": "preview.svg",
  "help": {
    "summary": "Three light questions about tea.",
    "goal": "Answer each question. There is no time limit.",
    "howToPlay": [
      "Read the question.",
      "Pick an answer.",
      "Read the short explanation, then go to the next question."
    ],
    "controls": [
      { "keys": "Tab", "action": "Move between answers and buttons" },
      { "keys": "Enter / Space", "action": "Choose the focused answer" }
    ]
  }
```

The starter licence is `CC-BY-4.0`. Change `license` to the SPDX expression you want.

## 5. Validate

```bash
pnpm --dir ~/src/lullshift-app lullshift validate tea-quiz
```

```text
✓ Tea Quiz (dev.you.tea-quiz@0.1.0): valid
```

Bundled engines are found automatically. Add `--engines <folder>` only for an engine outside `activities/engines`.

## 6. Simulate

```bash
pnpm --dir ~/src/lullshift-app lullshift simulate tea-quiz
```

```text
✓ Tea Quiz (dev.you.tea-quiz@0.1.0): valid
Simulating with msedge.exe (headless)
  ✓ fresh start: ready in 836 ms
  ✓ keyboard: 5 focusable element(s)
  ✓ save: state of 51 bytes
  ✓ restore: save → restore → save gives the same state
  ✓ working, completed, failed, cancelled: kept running
  ✓ needs_attention, duplicate, out of order: paused once
  ✓ resume after attention: resumed from its save
  ✓ needs_attention before ready: unloaded at once; saved state untouched
  ✓ start after an early pause: started
  ✓ close: saved and unloaded in 4 ms
  ✓ crash and recover: started again from the last save
  ✓ reduced motion, dark theme: started cleanly
  ✓ paused view stats: [{"label":"Question","value":"1 of 3"}]
  ✓ errors: no uncaught errors
  ✓ offline: no network requests
✓ all scenarios passed
```

To play it yourself in a host-like window, run `lullshift dev tea-quiz`. Ctrl+C stops it.

## 7. Pack

```bash
pnpm --dir ~/src/lullshift-app lullshift pack tea-quiz --out tea-quiz.lullpack
```

```text
✓ Tea Quiz (dev.you.tea-quiz@0.1.0): valid
Packed 4 files into tea-quiz.lullpack (2787 bytes)
sha256 4308001df67ccc09e9a79df6cd2704a9527179b5a298b27e61b947f48ce924da
```

The four files are `manifest.json`, `quiz.json`, `preview.svg` and the `README.md` note. Your hash will differ unless your files are byte for byte the same, but the same folder always gives the same bytes and hash. Users don't install `.lullpack` files directly: the app installs from Git marketplaces. Keep the file and its hash as a record of what you published.

## 8. Publish

Put the `tea-quiz` folder in a Git repository, list it in `.lullshift/marketplace.json` and push. Users add your repository in the app. See [Publishing a marketplace](publishing-a-marketplace.md).
