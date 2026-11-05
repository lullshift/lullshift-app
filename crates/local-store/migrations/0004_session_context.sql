-- 0004: Context Keeper and Next Move (Sprint 04, ADR-013).
-- `session_context` holds one row per session. Text the user wrote (user_intent, notes) and text
-- captured from the agent with opt-in consent (captured_intent, latest_request, ask, each with
-- its capture time) live in separate columns, so "clear captured text" removes only the latter.
-- Captured text is never written to `events`; the event log stays content-free (ADR-008).
-- `session_items` holds open decisions, review checklist items and Next Move items.
-- `session_links` holds user-made links between two sessions, stored once with the pair ordered.

CREATE TABLE session_context (
    provider                TEXT NOT NULL,
    provider_session_id     TEXT NOT NULL,
    user_intent             TEXT,
    notes                   TEXT NOT NULL DEFAULT '',
    captured_intent         TEXT,
    captured_intent_utc     TEXT,
    latest_request          TEXT,
    latest_request_utc      TEXT,
    ask                     TEXT,
    ask_utc                 TEXT,
    reviewed_utc            TEXT,
    updated_utc             TEXT NOT NULL,
    PRIMARY KEY (provider, provider_session_id)
) STRICT;

CREATE TABLE session_items (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    provider                TEXT NOT NULL,
    provider_session_id     TEXT NOT NULL,
    kind                    TEXT NOT NULL CHECK (kind IN ('decision', 'checklist', 'next_move')),
    category                TEXT CHECK (category IS NULL OR category IN ('question', 'acceptance', 'test', 'decision')),
    text                    TEXT NOT NULL,
    done                    INTEGER NOT NULL DEFAULT 0 CHECK (done IN (0, 1)),
    position                INTEGER NOT NULL,
    created_utc             TEXT NOT NULL
) STRICT;

CREATE INDEX session_items_by_session ON session_items (provider, provider_session_id, kind, position);

CREATE TABLE session_links (
    provider_a              TEXT NOT NULL,
    session_a               TEXT NOT NULL,
    provider_b              TEXT NOT NULL,
    session_b               TEXT NOT NULL,
    created_utc             TEXT NOT NULL,
    PRIMARY KEY (provider_a, session_a, provider_b, session_b),
    CHECK (provider_a < provider_b OR (provider_a = provider_b AND session_a < session_b))
) STRICT;

CREATE INDEX session_links_by_b ON session_links (provider_b, session_b);
