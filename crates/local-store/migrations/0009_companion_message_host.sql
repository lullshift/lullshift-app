-- 0009 (Sprint 09 review fix, ADR-020): the host (host or host:port) a Companion message was sent
-- to, or a reply came from. Chat history is only re-sent to the same host. Older rows stay NULL
-- and are never replayed.
ALTER TABLE companion_messages ADD COLUMN host TEXT
    CHECK (host IS NULL OR length(host) BETWEEN 1 AND 300);
