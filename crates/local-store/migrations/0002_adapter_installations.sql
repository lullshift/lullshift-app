-- 0002: agent adapter installations (ADR-010). One row per adapter; `record_json` holds the
-- adapter's undo information (paths and hashes only, never settings content).

CREATE TABLE adapter_installations (
    adapter_id      TEXT PRIMARY KEY,
    status          TEXT NOT NULL CHECK (status IN ('installed', 'uninstalled')),
    record_json     TEXT NOT NULL,
    consent_utc     TEXT NOT NULL,
    updated_utc     TEXT NOT NULL
) STRICT;
