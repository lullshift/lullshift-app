-- 0005 (Sprint 05): time played per activity, sessions no longer tracked after a disconnect,
-- and session titles.
-- `activity_play` is written only by the host (ADR-012 additions); an activity can't change it.
-- `sessions.disconnected_utc` is set when the user disconnects the session's tool (S05-T10);
-- null means tracked. A new event from that session clears it.
-- `session_context.user_title` is a name the user gave the session; `agent_title` is the
-- agent's own title, read only with capture consent (S05-T11, ADR-013).

CREATE TABLE activity_play (
    activity_id     TEXT PRIMARY KEY,
    played_seconds  INTEGER NOT NULL CHECK (played_seconds >= 0),
    launches        INTEGER NOT NULL CHECK (launches >= 0),
    last_played_utc TEXT NOT NULL
) STRICT;

ALTER TABLE sessions ADD COLUMN disconnected_utc TEXT;

ALTER TABLE session_context ADD COLUMN user_title TEXT;
ALTER TABLE session_context ADD COLUMN agent_title TEXT;
ALTER TABLE session_context ADD COLUMN agent_title_utc TEXT;
