# ADR-008: Agent event contract v1 and session state machine

- **Status:** Accepted (Sprint 01)
- **Date:** 2026-11-05
- **Owner:** Implementation lead
- **Related:** ADR-006 (schema-first contracts), architecture §4.2–4.3

## Context

Providers emit partial, duplicated and reordered lifecycle signals. The product rule is: never claim more than we know. A turn ending is not task completion; silence is not success; after a restart we don't know what happened while we were away.

## Decision

### Identity and idempotence

- A session is identified by `(provider, providerSessionId)`.
- Events are deduplicated by `eventId`. A duplicate is stored once and has no effect.

### Event → stored state

| Event | Stored state |
|---|---|
| `session_started` | `idle` |
| `working` | `working` |
| `needs_attention` | `needs_attention` |
| `turn_ended` | `idle` (never `completed`) |
| `completed` / `failed` / `cancelled` | that terminal state |
| `session_ended` | `ended` (terminal; the provider closed the session without saying the task was done; added in Sprint 02) |
| `heartbeat` | no change; refreshes liveness only |

### Ordering

- Each session records the provider timestamp of the event that set its state.
- An event changes state only if its `timestampUtc` is **strictly newer** than that timestamp; ties are broken by local receipt order.
- Older (reordered) events are stored for history but don't change state.
- A newer non-terminal event after a terminal one reopens the session; for example, a new turn after `completed`.
- `lastSeen` (liveness) is the newest timestamp of any event, heartbeats included.

### Displayed state (derived, never stored)

The UI shows a **view** computed from the stored state, `lastSeen`, the time the app started and "now":

1. **Unconfirmed after restart:** a session whose newest event arrived before this app start, and whose stored state is not terminal, is shown as `unknown` with reason `unconfirmed_after_restart` and its last known state, until a new event arrives.
2. **Stale:** a non-terminal session with no event for longer than the stale timeout (default **10 minutes**, configurable later) is shown as `unknown` with reason `stale` and its last known state.
3. Otherwise the stored state is shown with reason `observed`.

Terminal states are facts and never go stale. Unknown is always shown as unknown, never guessed.

Clarifications made while implementing (`crates/agent-core/src/reducer.rs`):

- The restart and stale rules use the **local receipt time** of the newest event, not the provider timestamp, so a skewed provider clock can't make a session look fresh. Provider timestamps still decide ordering, but a timestamp later than its receipt time is capped at the receipt time, so a future-dated event can't freeze the state.
- A session seen only through heartbeats stores `unknown` and is shown as `unknown` with reason `not_reported`.

### Input limits (closes S01-T06)

- `provider`: lowercase slug `^[a-z0-9][a-z0-9._-]{0,79}$`.
- `providerSessionId` and `correlationId`: 1–256 characters, no control, bidirectional, zero-width or other invisible formatting characters (U+0000–001F, U+007F–009F, U+061C, U+200B–200F, U+2028–202E, U+2060–2069, U+FEFF, U+FFF9–FFFB). Metadata strings follow the same rule.
- Optional fields, when present, must not be `null`.
- `capabilities`: at most 32 items, each a slug of at most 64 characters.
- `metadata`: at most 20 keys. Keys are identifier-like (`^[A-Za-z][A-Za-z0-9_.-]{0,63}$`). Values are scalars only (string of at most 256 characters, number, boolean, or null), with no nested objects or arrays.
- The transport rejects a payload over **64 KiB** before parsing (ADR-009).
- Schema, fixtures, TypeScript and Rust validators change together.

## Consequences

- Sessions may show `unknown` more often than a naive design would. That's intended.
- Sessions are never deleted by the reducer; retention and deletion arrive with Context Keeper (Sprint 04).

## Amendment (2026-11-07, decision 35, ADR-025): ended because the process is gone

A session can now be ended by Lullshift itself, not by an event: when the agent's recorded process is gone (`reducer::process_gone`), a non-terminal session becomes `ended` with a stored `ended_reason` of `process_gone`, timestamped "now". It is not a provider event and is never written to the event log; `last_seen` and `last_received` are unchanged. The ordering rules above apply to it unchanged: a real event with a timestamp at or after the mark reopens the session (and clears the reason in the same transaction), an older one does not, a heartbeat reopens it to `unknown`, and terminal sessions are never touched. A newer log can also fill a gap on a tracked, unfinished session as a marked guess (`imported_utc`, as for imports in ADR-024), never overriding newer reported state. Details, limits and tests: ADR-025.
