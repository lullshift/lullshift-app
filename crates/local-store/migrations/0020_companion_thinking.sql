-- 0020 (ADR-020 amendment "Thinking"): the model's reasoning summary, kept with the assistant
-- reply that it belongs to.
--
--   thinking     the cleaned reasoning text the provider streamed (control and invisible
--                characters already dropped), at most 65536 bytes plus a short note. Shown in a
--                collapsed "Thinking" section above the reply as plain text. NEVER part of the
--                reply text and NEVER sent back to a provider in later history.
--   thinking_ms  how long the thinking took, in milliseconds, for the "Thought for 12 s" header.
--
-- Existing rows keep NULL. Rollback: an older build refuses a database at version 20 (TooNew);
-- to go back, restore the backup the upgrade took before this step (schema version 19).
ALTER TABLE companion_messages
    ADD COLUMN thinking TEXT
    CHECK (thinking IS NULL OR length(CAST(thinking AS BLOB)) <= 66000);
ALTER TABLE companion_messages
    ADD COLUMN thinking_ms INTEGER
    CHECK (thinking_ms IS NULL OR thinking_ms BETWEEN 0 AND 86400000);
