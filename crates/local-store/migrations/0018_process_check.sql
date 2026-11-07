-- 0018 (owner decision 35, ADR-025): the process check.
-- `process_pid` / `process_start`: the agent's process behind the newest hook event (the hook's
-- first non-shell ancestor) and an opaque start key that tells it from a later process reusing
-- the PID. Both null when unknown (older hooks, a lookup that failed, imported sessions).
-- `ended_reason`: set to 'process_gone' when Lullshift itself ended the session because that
-- process is gone. The state is then `ended`, but no provider event says so, nothing is written
-- to the event log, and any later real event for the session reopens it (and clears this).

ALTER TABLE sessions ADD COLUMN process_pid INTEGER;
ALTER TABLE sessions ADD COLUMN process_start INTEGER;
ALTER TABLE sessions ADD COLUMN ended_reason TEXT;
