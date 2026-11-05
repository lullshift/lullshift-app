# Typing Drills

Engine `lullshift.typing@1`. Type short lines from a content pack, with per-character feedback.

- Pack data: `title`, optional `intro`, `lines` (printable ASCII, at most 120 characters each) and optional `mode` (`accuracy` or `flow`). See `data.schema.json`.
- Correct characters are underlined, mistakes are struck through, and a summary such as 3 mistakes is announced when a line ends. Colour is never the only signal.
- Accuracy and words per minute use active typing time only; pausing stops the clock. No leaderboard.
- Keystrokes never leave the page. Saved state holds counts only, never the typed text.
- `logic.js` holds the pure rules; tests are in `apps/desktop/src/activity/engines/typing.test.ts`.
- Compatibility fixtures: `tests/fixtures/engines/lullshift.typing@1/`.

Original code and text, Apache-2.0.
