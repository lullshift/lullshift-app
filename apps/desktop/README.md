# Desktop app

Tauri 2 shell with a React/TypeScript UI. The Rust side (`src-tauri`) owns state; the UI renders it.

- `pnpm desktop:dev` runs the app. `pnpm desktop:build` builds the frontend.
- Sprint 00 shows a **mock agent feed** (`mock_sessions` command backed by `crates/agent-core`).
- Security baseline: strict content security policy in `src-tauri/tauri.conf.json`; only `core:default` permissions in `src-tauri/capabilities/default.json`. Add permissions deliberately, one per need.
