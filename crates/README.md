# Rust crates

| Crate | Purpose | Sprint |
|---|---|---|
| `agent-core` | Agent event contract v1, session state, ADR-008 reducer and derived view | 00 → 01 |
| `local-store` | SQLite (WAL) event log and session records; migrations with backup, rollback and downgrade refusal | 01 |
| `agent-ipc` | Authenticated local IPC (named pipe / Unix socket), token, limits; dev client example | 01 |
| `agent-adapters` | Claude Code hook mapping, hook forwarder (app executable in `hook` mode), consent-based hook installer | 02 |

Planned (see `docs/TECHNICAL_ARCHITECTURE.md`): `activity-runtime`, `marketplace`. Add each when its sprint starts, not before.
