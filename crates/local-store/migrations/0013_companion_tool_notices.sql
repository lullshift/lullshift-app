-- 0013 (Sprint 12, owner decision 27.6, ADR-020 amendment): a small notice the conversation shows
-- when the model asked for a lookup that Lullshift would not run (an unknown tool, malformed
-- arguments, or over the per-message limit). One row per rejected call, tied to the user message
-- it belongs to. Stores only the reason and, when it is a short safe identifier, the tool name
-- the model asked for. Never the model's arguments.
CREATE TABLE companion_tool_notices (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id  INTEGER NOT NULL REFERENCES companion_messages (id) ON DELETE CASCADE,
    round       INTEGER NOT NULL CHECK (round BETWEEN 1 AND 20),
    reason      TEXT NOT NULL CHECK (reason IN ('unknown', 'malformed', 'limit')),
    tool        TEXT CHECK (tool IS NULL OR length(tool) BETWEEN 1 AND 32),
    created_utc TEXT NOT NULL
) STRICT;
CREATE INDEX companion_tool_notices_message ON companion_tool_notices (message_id, id);
