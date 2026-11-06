-- 0010 (Sprint 12, S12-T16, ADR-020 amendment): the read-only lookups ("tools") a Companion reply
-- used. One row per lookup the user approved or declined, tied to the user message it belongs to.
-- `result_text` is exactly what was sent to the provider (for a declined lookup, the fixed
-- "The user declined." sentence, never the data that was withheld). `host` is the host it went to:
-- like message text, it is only ever shown with that host. No keys, no file paths.
CREATE TABLE companion_tool_calls (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id       INTEGER NOT NULL REFERENCES companion_messages (id) ON DELETE CASCADE,
    round            INTEGER NOT NULL CHECK (round BETWEEN 1 AND 20),
    tool             TEXT NOT NULL CHECK (tool IN ('session_overview', 'suggest_break', 'activity_help')),
    arguments        TEXT NOT NULL CHECK (length(arguments) <= 2048),
    result_text      TEXT NOT NULL CHECK (length(result_text) <= 20000),
    status           TEXT NOT NULL CHECK (status IN ('approved', 'declined')),
    -- suggest_break only: the card the user can click. The tool never starts anything.
    card_activity_id TEXT CHECK (card_activity_id IS NULL OR length(card_activity_id) BETWEEN 1 AND 128),
    card_name        TEXT CHECK (card_name IS NULL OR length(card_name) <= 200),
    card_minutes     INTEGER CHECK (card_minutes IS NULL OR card_minutes BETWEEN 1 AND 240),
    host             TEXT CHECK (host IS NULL OR length(host) BETWEEN 1 AND 300),
    created_utc      TEXT NOT NULL
) STRICT;
CREATE INDEX companion_tool_calls_message ON companion_tool_calls (message_id, id);
