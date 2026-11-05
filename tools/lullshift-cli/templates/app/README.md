# {{name}}

A Lullshift activity, started from the `app` template.

- `lullshift dev .` plays it in a local window with the host's controls (pause, attention, resume).
- `lullshift validate .` checks the manifest and the folder rules.
- `lullshift simulate .` runs the interruption scenarios in a sandboxed headless browser.
- `lullshift pack .` writes a deterministic `.lullpack` for sideloading.

Rules that keep it working in Lullshift:

- Classic scripts only: no modules, no inline script or inline styles.
- No network and no browser storage. Use `LullshiftBridge.storage` (with the `activity.storage` permission) for small values.
- Keep every bit of progress in the state you return from `saveState`, and report it with `LullshiftBridge.autosave` after each change.
- Full keyboard play, visible focus, and honour reduced motion.
- Controls sit on one row when they fit.
- Keep `help` in `manifest.json` up to date whenever play changes, and keep `preview` pointing at an image of your activity.
- Original names, text and art only.
