-- 0018 (owner decision 35, ADR-025): the process check and the log catch-up.
--
-- `process_pid` / `process_start` / `process_ns`: the agent's process behind the newest hook
-- event (the hook's first non-shell ancestor), an opaque start key that tells it from a later
-- process reusing the PID, and (Linux) the inode of the PID namespace it was seen in. All null
-- when unknown (older hooks, a lookup that failed, imported sessions). A reader in another PID
-- namespace ignores the process.
--
-- `ended_reason`: set to 'process_gone' when Lullshift itself ended the session because that
-- process is gone. The state is then `ended`, but no provider event says so, nothing is written
-- to the event log, and any later real event for the session reopens it (and clears this).
--
-- `log_hint_utc` and `hint_prior_*`: a session Lullshift already tracked whose state was filled
-- in from the agent's own log at start-up (opt-in catch-up). The state columns then hold a guess
-- (and `imported_utc` is set too, so the session is shown as "from log"); the `hint_prior_*`
-- columns keep the record as it was before the guess, so the next real event is applied to that
-- record under the normal ordering rules instead of replacing it. Cleared by the next event.

ALTER TABLE sessions ADD COLUMN process_pid INTEGER;
ALTER TABLE sessions ADD COLUMN process_start INTEGER;
ALTER TABLE sessions ADD COLUMN process_ns INTEGER;
ALTER TABLE sessions ADD COLUMN ended_reason TEXT;
ALTER TABLE sessions ADD COLUMN log_hint_utc TEXT;
ALTER TABLE sessions ADD COLUMN hint_prior_state TEXT;
ALTER TABLE sessions ADD COLUMN hint_prior_state_ts TEXT;
ALTER TABLE sessions ADD COLUMN hint_prior_seen TEXT;
ALTER TABLE sessions ADD COLUMN hint_prior_received TEXT;
