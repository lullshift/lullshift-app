-- 0001: agent event log and session records (ADR-008).
-- Timestamps are RFC 3339 text as received (provider) or recorded (local, UTC).

CREATE TABLE events (
    receipt_seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id            TEXT NOT NULL UNIQUE,
    provider            TEXT NOT NULL,
    provider_session_id TEXT NOT NULL,
    event_type          TEXT NOT NULL,
    timestamp_utc       TEXT NOT NULL,
    received_utc        TEXT NOT NULL,
    event_json          TEXT NOT NULL
) STRICT;

CREATE INDEX events_by_session ON events (provider, provider_session_id, receipt_seq);

CREATE TABLE sessions (
    provider            TEXT NOT NULL,
    provider_session_id TEXT NOT NULL,
    state               TEXT NOT NULL,
    state_timestamp_utc TEXT,
    last_seen_utc       TEXT NOT NULL,
    last_received_utc   TEXT NOT NULL,
    PRIMARY KEY (provider, provider_session_id)
) STRICT;
