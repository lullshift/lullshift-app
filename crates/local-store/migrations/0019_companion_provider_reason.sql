-- 0019 (ADR-020 amendment, 2026-11-07): the provider's own words, kept apart from Lullshift's
-- fixed error sentence, and a flag for a reply that was not streamed.
--
--   error_reason_by  who said it ("OpenAI", "Anthropic" or "The provider")
--   error_reason     one sanitised, capped sentence from the provider's standard JSON error
--                    (see crates/companion/src/reason.rs). Never the raw body, never the key.
--                    Shown as plain text in its own element, labelled as the provider's words.
--   no_stream        1 on an assistant reply that arrived all at once because the request was
--                    not streamed (OpenAI refused streaming, or the app remembered it). A note
--                    about delivery, never part of the reply text and never replayed.
--
-- Existing rows keep NULL / 0. (0018 is reserved by another branch.)
ALTER TABLE companion_messages
    ADD COLUMN error_reason_by TEXT
    CHECK (error_reason_by IS NULL OR length(error_reason_by) BETWEEN 1 AND 24);
ALTER TABLE companion_messages
    ADD COLUMN error_reason TEXT
    CHECK (error_reason IS NULL OR length(error_reason) BETWEEN 1 AND 300);
ALTER TABLE companion_messages
    ADD COLUMN no_stream INTEGER NOT NULL DEFAULT 0 CHECK (no_stream IN (0, 1));
