---
name: stripe-commerce
description: Implement or review Lullshift Stripe Billing, Checkout, Connect, creator settlement, subscription and paid activity entitlements with legal eligibility gates.
---

# Stripe Commerce

1. Read `reference/COMMERCE_DOMAIN.md`, Sprints 12/13 and ADR-020/021 status.
2. Reject production activation if company incorporation, merchant-of-record, tax treatment, provider eligibility or supported seller countries remain unresolved.
3. Implement server-constructed orders and price snapshots, provider interfaces and fake/sandbox integration.
4. Verify webhook signature from exact raw body; deduplicate provider event IDs, store durable inbox/outbox state, retry safely.
5. Fulfill entitlements only after verified trusted provider confirmation, not redirect or client claims.
6. Exercise refunds, disputes, transfer reversals, Connect payout delays, FX/currency and reconciliations.
7. Keep creator funds within compliant provider settlement; never create an unlicensed funds-custody design.
8. Document current provider constraints and executed integration tests. Do not claim live payments without configured credentials and actual sandbox evidence.
