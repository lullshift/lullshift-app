-- 0016 (owner decision 34, ADR-024): sessions the user imported from an agent's own local log.
-- `sessions.imported_utc` is set when the row was created by "Import sessions" instead of by a
-- hook event; null means the session is tracked from hook events as usual. The stored state is
-- only a guess from the log. Imported sessions never notify, never wake the tray mascot and never
-- pause an activity. The first real hook event for the same session clears the mark (in the same
-- ingest transaction), and normal tracking takes over.

ALTER TABLE sessions ADD COLUMN imported_utc TEXT;
