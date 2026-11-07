# Rust crates

| Crate | Purpose | Sprint |
|---|---|---|
| `agent-core` | Agent event contract v1, session state, ADR-008 reducer and derived view | 00 → 01 |
| `local-store` | SQLite (WAL) event log and session records; migrations with backup, rollback and downgrade refusal | 01 |
| `agent-ipc` | Authenticated local IPC (named pipe / Unix socket), token, limits; dev client example | 01 |
| `agent-adapters` | Claude Code hook mapping, hook forwarder (app executable in `hook` mode), consent-based hook installer | 02 |
| `marketplace` | Hardened git runner, index and pack validation, pinned installs, digests and update diffs; also the manifest and engine-data validators shared with the desktop crate | 08 |
| `macos-webrtc` | The Objective-C messages that turn off WebRTC in a macOS webview; one of two crates that allow unsafe code (ADR-012) | 12 |
| `windows-package` | One Win32 call (`GetCurrentPackageFamilyName`) that tells whether Lullshift runs inside another app's package; the other crate that allows unsafe code (ADR-010 amendment, decision 33) | 12 |

Planned (see `docs/TECHNICAL_ARCHITECTURE.md`): `activity-runtime`. Add each when its sprint starts, not before.
