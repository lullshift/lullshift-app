-- 0021 (ADR-028 section 7): sets of engine data written by the user's own Companion model, kept
-- so they replay offline and resume like any activity.
--
--   generated_sets       one row per set. `id` is the host-made activity id under the reserved
--                        `lullshift.generated.` namespace (installs and bundling refuse that
--                        prefix). `engine` is `id@major` of a bundled first-party engine.
--                        `data_json` is the checked engine data; the host checks it again on
--                        every load (engine schema and plain-text checks), and `unusable` is set
--                        when that check fails. `topic`, `data_json` and `model` are never logged.
--   generated_set_skips  items the user marked "This looks wrong", stored locally only.
--
-- A set's snapshot, activity storage and play time live in the existing activity tables keyed by
-- the set id; the host deletes them together with the set in one transaction (no foreign key:
-- those tables hold every activity, not only sets). The skips cascade with the set.
--
-- Rollback: an older build refuses a database at version 21 (TooNew); to go back, restore the
-- backup the upgrade took before this step (schema version 20).
CREATE TABLE generated_sets (
    id            TEXT PRIMARY KEY
        CHECK (length(id) BETWEEN 21 AND 128 AND substr(id, 1, 20) = 'lullshift.generated.'),
    engine        TEXT NOT NULL CHECK (length(engine) BETWEEN 3 AND 64),
    topic         TEXT NOT NULL CHECK (length(topic) <= 200),
    difficulty    TEXT NOT NULL CHECK (length(difficulty) BETWEEN 1 AND 32),
    data_json     TEXT NOT NULL CHECK (length(CAST(data_json AS BLOB)) <= 262144),
    created_utc   TEXT NOT NULL,
    provider_host TEXT NOT NULL CHECK (length(provider_host) <= 255),
    model         TEXT CHECK (model IS NULL OR length(model) <= 200),
    origin_pack   TEXT CHECK (origin_pack IS NULL OR length(origin_pack) <= 128),
    unusable      INTEGER NOT NULL DEFAULT 0 CHECK (unusable IN (0, 1))
) STRICT;

CREATE INDEX generated_sets_by_engine ON generated_sets (engine, created_utc);

CREATE TABLE generated_set_skips (
    set_id     TEXT NOT NULL REFERENCES generated_sets (id) ON DELETE CASCADE,
    item_index INTEGER NOT NULL CHECK (item_index BETWEEN 0 AND 9999),
    PRIMARY KEY (set_id, item_index)
) STRICT;
