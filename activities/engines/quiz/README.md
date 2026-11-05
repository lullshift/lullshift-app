# Quick Quiz

The engine `lullshift.quiz@1`: a multiple-choice quiz shown one question at a time, with a kind explanation after each answer. A content pack supplies the data (see `data.schema.json`); with no pack it shows a three-question sample.

- `logic.js` holds the pure rules (`globalThis.QuizLogic`); `main.js` is the page.
- Play is keyboard friendly, the score is shown privately at the end and is not kept, and the place is saved through `saveState`.
- `lullshift validate .` and `lullshift simulate .` check it. License: Apache-2.0. Original text, no third-party assets.
