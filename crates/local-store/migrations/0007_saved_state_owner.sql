-- 0007 (Sprint 08 security review, L7): who a kept saved state belongs to.
-- When an activity is uninstalled but its saved state is kept, the install's marketplace and
-- source are recorded here. A later install of the same id from a different marketplace or
-- source clears that state instead of handing it to the new code. The row is deleted when the
-- state is cleared or claimed. No foreign key: the marketplace row may be gone.

CREATE TABLE activity_state_owner (
    activity_id    TEXT PRIMARY KEY CHECK (length(activity_id) BETWEEN 1 AND 128),
    marketplace_id TEXT NOT NULL CHECK (length(marketplace_id) BETWEEN 1 AND 64),
    source_url     TEXT NOT NULL CHECK (length(source_url) BETWEEN 1 AND 2048
        AND source_url NOT LIKE '%://%@%'),
    recorded_at    TEXT NOT NULL
) STRICT;
