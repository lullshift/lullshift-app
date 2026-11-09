# Brain Teasers

The engine `lullshift.teaser@1`: short puzzles shown one at a time, with an optional hint, a revealed answer and an optional private self-check. A content pack supplies the data (see `data.schema.json`); with no pack it shows a two-teaser sample.

- `logic.js` holds the pure rules (`globalThis.TeaserLogic`); `main.js` is the page.
- Optional `roundSize` (3 to 40, at most the number of teasers): each round shows that many teasers in the pack's written order, drawn with a seeded generator and preferring teasers not shown in recent rounds; when too few are left unshown, the least recently shown fill the round. The saved state then also holds `seed` and `seen` (teaser indexes from earlier rounds, each at most once), which rebuild the same round after a pause or restart; positions and self-check marks count within the round. A `roundSize` above the teaser count plays the whole pack, and `lullshift validate` warns about it. Without `roundSize` play is exactly as before. Bundled teaser packs have fewer than 12 teasers and don't set it.
- No typing is needed, self-check marks are shown privately at the end and are not kept beyond the saved place, and the place is saved through `saveState`.
- `lullshift validate .` and `lullshift simulate .` check it. License: Apache-2.0. Original text, no third-party assets.
