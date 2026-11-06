-- 0011 (Sprint 12, decision 17, ADR-015 amendment): private-network marketplaces.
-- A marketplace whose address is an IP address, localhost, a one-label name or a local-network
-- suffix (or whose name resolves to a private address) is used only after the user ticked
-- "This marketplace is on my own computer or network". `private_host` records that they did.
-- Existing rows were all added under the strict public-only rule, so they default to 0.

ALTER TABLE marketplaces
    ADD COLUMN private_host INTEGER NOT NULL DEFAULT 0 CHECK (private_host IN (0, 1));
