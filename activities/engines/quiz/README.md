# Quick Quiz

The engine `lullshift.quiz@1`: a multiple-choice quiz shown one question at a time, with a kind explanation after each answer. A content pack supplies the data (see `data.schema.json`); with no pack it shows a three-question sample.

- `logic.js` holds the pure rules (`globalThis.QuizLogic`); `main.js` is the page.
- A question may have a picture: `"image": {"src": "images/<name>.png", "alt": "<what it shows>"}`, where `src` is one of the pack's listed `images` (PNG or WebP) and `alt` is required. The picture sits above the choices with its alt text; if it can't load, the engine shows "Picture not available. It shows: <alt>". Write every question so it can be answered from the alt text alone.
- Optional `roundSize` (3 to 40, at most the number of questions): each round asks that many questions, drawn with the engine's seeded generator and preferring questions not asked in recent rounds; when too few are left unasked, the least recently asked fill the round. The round keeps the written order unless `shuffle` is on. The saved state then also holds `seen`, the question indexes from earlier rounds (each at most once, so never more than the question count), and the seed and `seen` rebuild the same round after a pause or restart. The schema can't compare `roundSize` with the question count, so a larger value plays the whole quiz each round and `lullshift validate` warns about it. Without `roundSize` the quiz plays exactly as before. Bundled quiz packs have fewer than 12 questions and don't set it.
- Play is keyboard friendly, the score is shown privately at the end and is not kept, and the place is saved through `saveState`.
- `lullshift validate .` and `lullshift simulate .` check it. License: Apache-2.0. Original text, no third-party assets.
