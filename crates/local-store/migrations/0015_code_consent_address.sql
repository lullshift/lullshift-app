-- 0015 (security review of owner decision 27.5, INFO-8): the address a program was installed
-- from when the user accepted the risk. A later change of the marketplace's address moves
-- `source_url` (the install keeps working because its pinned commit is checked against its
-- digest), but this column keeps the address the user actually saw and consented to, so the
-- Manage list can say "Address changed since you accepted". Rows from before this step, and
-- packs, have none.

ALTER TABLE installed_activities
    ADD COLUMN code_consent_url TEXT
        CHECK (code_consent_url IS NULL OR (
            length(code_consent_url) BETWEEN 1 AND 2048
            AND code_consent_commit IS NOT NULL));
