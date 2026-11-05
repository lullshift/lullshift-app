# Reflection Prompts

Engine `lullshift.reflection@1`. Shows one light prompt at a time from a content pack, with optional notes.

- Pack data: `title`, optional `intro`, `prompts` (each `text`, optional `followUp`) and optional `allowNotes`. See `data.schema.json`.
- Notes (at most 1000 characters per prompt) are kept only in the pack's saved state on this device. They are never sent anywhere and never appear in the paused-view summary or the live region. A visible note says so, and Clear my notes erases them.
- `logic.js` holds the pure rules; tests are in `apps/desktop/src/activity/engines/reflection.test.ts`.
- Compatibility fixtures: `tests/fixtures/engines/lullshift.reflection@1/`.

Original code and text, Apache-2.0.
