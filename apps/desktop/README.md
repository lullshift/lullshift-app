# Desktop app

Tauri 2 shell with a React/TypeScript UI. The Rust side (`src-tauri`) owns state; the UI renders it.

- `pnpm desktop:dev` runs the app. `pnpm desktop:build` builds the frontend.
- Runs from the tray: closing the window hides it, "Quit Lullshift" in the tray menu exits. A second launch focuses the running instance. Start-at-sign-in is off until the user enables it.
- On start it opens the local store (`crates/local-store`) and the authenticated agent endpoint (`crates/agent-ipc`). The `dashboard` command returns sessions with their derived state (ADR-008); the UI refreshes on `sessions-changed` and every 30 s so stale states appear without new events.
- Security baseline: strict content security policy in `src-tauri/tauri.conf.json`. `src-tauri/capabilities/default.json` grants only the `dashboard` command, event listen/unlisten, autostart enable/disable/status and notification permission check/request. Add permissions deliberately, one per need.
