# Rust crates

| Crate | Purpose | Sprint |
|---|---|---|
| `agent-core` | Agent event contract v1, session state, ADR-008 reducer and derived view | 00 → 01 |
| `local-store` | SQLite (WAL) event log and session records; migrations with backup, rollback and downgrade refusal | 01 |
| `agent-ipc` | Authenticated local IPC (named pipe / Unix socket), token, limits; dev client example | 01 |

Planned (see `docs/TECHNICAL_ARCHITECTURE.md`): `agent-adapters`, `activity-runtime`, `marketplace`. Add each when its sprint starts, not before.
