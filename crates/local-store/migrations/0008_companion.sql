-- 0008 (Sprint 09, ADR-020): Companion provider profiles, threads, messages and token usage.
-- NO KEY MATERIAL lives here: not the key, not a hash, not a "last four" hint. Keys are in the
-- OS keychain under service `app.lullshift.companion`, account `<profile id>`.

CREATE TABLE companion_profiles (
    id                   TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 64),
    kind                 TEXT NOT NULL CHECK (kind IN ('anthropic', 'openai', 'openaiCompatible')),
    label                TEXT NOT NULL CHECK (length(label) BETWEEN 1 AND 80),
    base_url             TEXT NOT NULL CHECK (length(base_url) BETWEEN 1 AND 2048
        AND base_url NOT LIKE '%://%@%'),
    model                TEXT NOT NULL CHECK (length(model) BETWEEN 1 AND 200),
    monthly_token_budget INTEGER CHECK (monthly_token_budget IS NULL
        OR monthly_token_budget BETWEEN 1 AND 1000000000000),
    is_default           INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    created_utc          TEXT NOT NULL,
    updated_utc          TEXT NOT NULL
) STRICT;

-- At most one default profile.
CREATE UNIQUE INDEX companion_one_default ON companion_profiles (is_default) WHERE is_default = 1;

CREATE TABLE companion_threads (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    title       TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 120),
    profile_id  TEXT REFERENCES companion_profiles (id) ON DELETE SET NULL,
    model       TEXT,
    created_utc TEXT NOT NULL,
    updated_utc TEXT NOT NULL
) STRICT;

CREATE TABLE companion_messages (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    thread_id        INTEGER NOT NULL REFERENCES companion_threads (id) ON DELETE CASCADE,
    role             TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    text             TEXT NOT NULL CHECK (length(text) <= 200000),
    -- The exact context text that was sent with this user message (what the user approved).
    attached_context TEXT CHECK (attached_context IS NULL OR length(attached_context) <= 200000),
    scope_kind       TEXT NOT NULL DEFAULT 'none'
        CHECK (scope_kind IN ('none', 'sessionSummary', 'selectedNotes')),
    status           TEXT NOT NULL CHECK (status IN ('sent', 'failed', 'pending')),
    error_kind       TEXT CHECK (error_kind IS NULL OR length(error_kind) <= 40),
    error_message    TEXT CHECK (error_message IS NULL OR length(error_message) <= 500),
    input_tokens     INTEGER CHECK (input_tokens IS NULL OR input_tokens >= 0),
    output_tokens    INTEGER CHECK (output_tokens IS NULL OR output_tokens >= 0),
    profile_id       TEXT REFERENCES companion_profiles (id) ON DELETE SET NULL,
    model            TEXT,
    created_utc      TEXT NOT NULL
) STRICT;
CREATE INDEX companion_messages_thread ON companion_messages (thread_id, id);

-- One row per provider call that reported usage. `month` is the UTC month, `YYYY-MM`.
CREATE TABLE companion_usage (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id    TEXT NOT NULL REFERENCES companion_profiles (id) ON DELETE CASCADE,
    month         TEXT NOT NULL CHECK (month GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]'),
    input_tokens  INTEGER NOT NULL CHECK (input_tokens >= 0),
    output_tokens INTEGER NOT NULL CHECK (output_tokens >= 0),
    recorded_utc  TEXT NOT NULL
) STRICT;
CREATE INDEX companion_usage_month ON companion_usage (profile_id, month);
