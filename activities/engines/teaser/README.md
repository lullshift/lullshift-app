# Brain Teasers

The engine `lullshift.teaser@1`: short puzzles shown one at a time, with an optional hint, a revealed answer and an optional private self-check. A content pack supplies the data (see `data.schema.json`); with no pack it shows a two-teaser sample.

- `logic.js` holds the pure rules (`globalThis.TeaserLogic`); `main.js` is the page.
- No typing is needed, self-check marks are shown privately at the end and are not kept beyond the saved place, and the place is saved through `saveState`.
- `lullshift validate .` and `lullshift simulate .` check it. License: Apache-2.0. Original text, no third-party assets.
