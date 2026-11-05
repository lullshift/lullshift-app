---
name: activity-contract
description: Build Lullshift activities with consistent interruption, privacy, state-persistence, help and paused-view rules.
---

# Activity Contract

1. Read `packages/activity-sdk/README.md`, `reference/ACTIVITY_MANIFEST_V1.schema.json`, ADR-003 and ADR-012.
2. Use the bridge: `LullshiftBridge.start({ onInit, saveState, onPause, onResume })`. Restore from `onInit({ restore })`, and call `LullshiftBridge.autosave(state)` after each meaningful change. Test rapid transitions.
3. The host owns interruption: on `needs_attention` it pauses and unloads the activity without relying on cooperation. Keep one JSON state (at most 256 KiB), versioned, validated on restore, so restore is exact.
4. `activity.storage` is the only permission. Declare it only if used.
5. Keyboard-only play, text labels, visible focus, reduced motion, light and dark theme; works fully offline.
6. Help is mandatory: `help` in `manifest.json` with `summary`, `goal`, `howToPlay`, `controls`, `tips`, plain text, in sync with play. Treat the `validate` "no help" warning as a failure.
7. Layout: controls on one row when they fit, no mid-label breaks, wrap only on very narrow windows.
8. Paused view: `LullshiftBridge.summary([...])` with 1 to 4 stats after each meaningful change, and a `preview` image of original art.
9. Code activities: static web only; classic scripts, no inline script or style, no network, filesystem, clipboard, navigation or Tauri IPC. Content packs: data only, validated against the engine's schema, no executable files.
10. Run `pnpm lullshift validate <dir>` and `pnpm lullshift simulate <dir>` and report the real output. Never claim success if they were not run or if they failed. Never add privileged host APIs to make an activity work; propose an ADR instead.

## Handoff format

Summarize changed files, the real command output, risks and decisions, and the next recommended action. Follow `CLAUDE.md`.
