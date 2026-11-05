---
name: activity-contract
description: Build Lullshift activities with consistent interruption, privacy and state-persistence rules.
---

# Activity Contract

1. Read `reference/ACTIVITY_MANIFEST_V1.schema.json`, ADR-003 and the activity sections of `docs/TECHNICAL_ARCHITECTURE.md`.
2. Implement the lifecycle `initialize/start/pause/resume/saveState/restoreState/dispose` and test rapid transitions.
3. The host owns interruption: on `needs_attention` it pauses/freezes the activity without relying on cooperation. Keep state snapshots small, serializable and versioned so restore is exact.
4. Keep activity data namespaced and within quota; `activity.storage` is the only permission.
5. Keyboard-only play, screen-reader labels, reduced-motion mode; works fully offline.
6. Code activities: static web only; no network, filesystem, clipboard, navigation or Tauri IPC. Content packs: data only, validated against the engine's data schema, no executable files.
7. Run sandbox-denial and forced-interruption simulations. Never add privileged host APIs to make an activity work; propose an ADR instead.

## Handoff format

Summarize changed files, test results, risks/decisions and next recommended action. Follow `CLAUDE.md`.
