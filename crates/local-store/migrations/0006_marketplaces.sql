-- 0006 (Sprint 08, ADR-015): marketplaces, installed activities, built-in activity disable.
-- URLs are stored scrubbed of userinfo; the API refuses a URL with credentials, and the CHECKs
-- below are a second line of defence. Lullshift never stores Git credentials.
--
-- Removing a marketplace must leave its installs in place (pinned copies). installed_activities
-- therefore has a RESTRICT foreign key and the API soft-removes a marketplace that still has
-- installs by setting `removed_at`; one with none is deleted. The built-in row can't be
-- deleted or marked removed (triggers below, also checked in the API).

CREATE TABLE marketplaces (
    id                  TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 64),
    url                 TEXT NOT NULL CHECK (length(url) BETWEEN 1 AND 2048 AND url NOT LIKE '%://%@%'),
    added_at            TEXT NOT NULL,
    last_fetched_commit TEXT CHECK (last_fetched_commit IS NULL
        OR (length(last_fetched_commit) = 40 AND last_fetched_commit NOT GLOB '*[^0-9a-f]*')),
    last_fetched_at     TEXT,
    builtin             INTEGER NOT NULL DEFAULT 0 CHECK (builtin IN (0, 1)),
    removed_at          TEXT
) STRICT;

INSERT INTO marketplaces (id, url, added_at, builtin)
VALUES ('lullshift', 'https://github.com/lullshift/lullshift-app', '1970-01-01T00:00:00Z', 1);

CREATE TRIGGER marketplaces_builtin_no_delete
BEFORE DELETE ON marketplaces WHEN OLD.builtin = 1
BEGIN SELECT RAISE(ABORT, 'the built-in marketplace cannot be removed'); END;

CREATE TRIGGER marketplaces_builtin_no_remove
BEFORE UPDATE ON marketplaces WHEN OLD.builtin = 1 AND (NEW.removed_at IS NOT NULL OR NEW.builtin = 0)
BEGIN SELECT RAISE(ABORT, 'the built-in marketplace cannot be removed'); END;

CREATE TABLE installed_activities (
    activity_id      TEXT PRIMARY KEY CHECK (length(activity_id) BETWEEN 1 AND 128),
    marketplace_id   TEXT NOT NULL REFERENCES marketplaces (id) ON DELETE RESTRICT,
    source_url       TEXT NOT NULL CHECK (length(source_url) BETWEEN 1 AND 2048
        AND source_url NOT LIKE '%://%@%'),
    source_path      TEXT NOT NULL CHECK (length(source_path) <= 1024),
    commit_sha       TEXT NOT NULL
        CHECK (length(commit_sha) = 40 AND commit_sha NOT GLOB '*[^0-9a-f]*'),
    manifest_version TEXT NOT NULL CHECK (length(manifest_version) BETWEEN 1 AND 64),
    kind             TEXT NOT NULL CHECK (kind IN ('app', 'content')),
    engine           TEXT CHECK (engine IS NULL OR (kind = 'content' AND length(engine) BETWEEN 1 AND 128)),
    permissions      TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(permissions) AND json_type(permissions) = 'array'),
    digest           TEXT NOT NULL CHECK (length(digest) = 64 AND digest NOT GLOB '*[^0-9a-f]*'),
    size_bytes       INTEGER NOT NULL CHECK (size_bytes >= 0),
    enabled          INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    installed_at     TEXT NOT NULL,
    updated_at       TEXT NOT NULL
) STRICT;

CREATE INDEX installed_activities_marketplace ON installed_activities (marketplace_id);

-- Bundled (built-in) activities are not install records; a row here means "hidden from the
-- library". Saved state is untouched.
CREATE TABLE activity_disabled (
    activity_id TEXT PRIMARY KEY CHECK (length(activity_id) BETWEEN 1 AND 128),
    disabled_at TEXT NOT NULL
) STRICT;
