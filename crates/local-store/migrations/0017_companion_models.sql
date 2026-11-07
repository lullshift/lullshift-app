-- 0017 (decision 36, ADR-020 amendment): several models per Companion provider.
-- Each provider profile keeps its kind, label, base URL, budget and default flag; its models move
-- to their own table. Each existing profile's single model becomes that provider's default model.
-- The overall default is the default model of the default provider. Threads and messages keep
-- their own `model` columns (what was actually used), and usage stays per provider.
-- NO KEY MATERIAL lives here; keys stay in the OS keychain under the profile id.

CREATE TABLE companion_models (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id  TEXT NOT NULL REFERENCES companion_profiles (id) ON DELETE CASCADE,
    model       TEXT NOT NULL CHECK (length(model) BETWEEN 1 AND 200),
    label       TEXT CHECK (label IS NULL OR length(label) BETWEEN 1 AND 80),
    is_default  INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    created_utc TEXT NOT NULL,
    UNIQUE (profile_id, model)
) STRICT;

-- At most one default model per provider.
CREATE UNIQUE INDEX companion_models_one_default ON companion_models (profile_id)
    WHERE is_default = 1;

INSERT INTO companion_models (profile_id, model, is_default, created_utc)
    SELECT id, model, 1, created_utc FROM companion_profiles;

ALTER TABLE companion_profiles DROP COLUMN model;
