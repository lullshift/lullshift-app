# ADR-007: SQLite library

- **Status:** Accepted, provisional (confirm during Sprint 01 implementation)
- **Date:** 2026-11-05
- **Owner:** Implementation lead

## Context

The Rust core persists agent sessions, events, context bookmarks, activity state and marketplace installs in one local SQLite database (WAL mode, versioned migrations, transactions). The candidates were `rusqlite` and `sqlx`.

## Decision

Use **`rusqlite` with the `bundled` feature**:

- SQLite is compiled in, so the version is the same on every OS and doesn't depend on a system library.
- It's synchronous and simple, which suits a single-user embedded database. Calls run on a dedicated blocking task or thread when used from async (tokio) code.
- There's no compile-time database requirement (sqlx's checked queries need a database or offline metadata at build time).
- Migrations are plain SQL files applied in order inside a transaction, with a `schema_version` table. Each migration ships with a test that applies all previous migrations and then this one. A pre-migration backup copy is made on upgrade.

## Alternatives

- `sqlx`: async and compile-time-checked queries, but heavier build setup and no real benefit for a local single-writer database.

## Revisit if

Sprint 01 finds measured contention or async integration pain.
