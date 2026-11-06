-- 0012 (Sprint 12, decision 18.4, ADR-015 amendment): changing a marketplace's address.
-- When the user changes where a marketplace lives, Lullshift checks that each pinned commit
-- installed from it still exists at the new address. An install whose commit is missing there
-- stays exactly as it is (same files, same saved state) and is marked here, so the app can say
-- "not found at the new address". The mark is cleared when the install is updated or replaced.

ALTER TABLE installed_activities
    ADD COLUMN source_note TEXT CHECK (source_note IS NULL OR source_note IN ('notFoundAtNewAddress'));
