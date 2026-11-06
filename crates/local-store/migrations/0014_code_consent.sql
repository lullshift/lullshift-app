-- 0014 (owner decision 27.5, ADR-015 amendment): consent to third-party code.
-- Code activities (kind 'app') from a marketplace other than the built-in one can be installed
-- after the user reads a risk warning and ticks "I understand and want to install this program".
-- Each install, and each update that changes the pinned commit, asks again. The answer is
-- recorded here with the commit it was given for. Both columns are set together or not at all,
-- and only for code activities; content packs and older rows have neither.

ALTER TABLE installed_activities
    ADD COLUMN code_consent_at TEXT;
ALTER TABLE installed_activities
    ADD COLUMN code_consent_commit TEXT
        CHECK ((code_consent_commit IS NULL) = (code_consent_at IS NULL)
            AND (code_consent_commit IS NULL OR (
                length(code_consent_commit) = 40
                AND code_consent_commit NOT GLOB '*[^0-9a-f]*'
                AND kind = 'app')));
