-- 0003: user preferences and activity data (Sprint 03).
-- `preferences` holds small JSON values keyed by name (notification priorities, quiet hours).
-- `activity_storage` is each activity's namespaced key-value store, written only through the
-- host bridge and bounded by a per-activity quota enforced in Rust.
-- `activity_snapshots` keeps the last saved state of each activity so a pause, a crash or a
-- restart never loses progress. Activity IDs come from manifests the host validated.

CREATE TABLE preferences (
    key             TEXT PRIMARY KEY,
    value_json      TEXT NOT NULL,
    updated_utc     TEXT NOT NULL
) STRICT;

CREATE TABLE activity_storage (
    activity_id     TEXT NOT NULL,
    key             TEXT NOT NULL,
    value           TEXT NOT NULL,
    updated_utc     TEXT NOT NULL,
    PRIMARY KEY (activity_id, key)
) STRICT;

CREATE TABLE activity_snapshots (
    activity_id     TEXT PRIMARY KEY,
    snapshot        TEXT NOT NULL,
    saved_utc       TEXT NOT NULL
) STRICT;
