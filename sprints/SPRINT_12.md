# Sprint 12 - Subscriptions, Billing and Entitlement Backbone

**Status:** planned | **Suggested lead:** Opus for architecture and final review; Sonnet for bounded implementation.


## Goal
Feature-flagged hosted subscriptions using Stripe Billing with provider-neutral domain boundaries and offline entitlement support.
## Work
- [ ] ADR-020 operator jurisdiction and MoR open until counsel decision; capture compliance checklist.
- [ ] Build Order/Payment/Subscription/Entitlement domain and transaction inbox/outbox.
- [ ] Implement Stripe Checkout/Billing sandbox adapter and deterministic fake provider.
- [ ] Verified webhook raw-body signature checks, retries, idempotency and reconciliation jobs.
- [ ] Test account-scoped subscription and signed device entitlements, refund and cancellation behavior.
## Acceptance
- [ ] Browser redirect never independently grants entitlement.
- [ ] Duplicate/reordered webhook messages do not double-fulfill.
- [ ] Community local features remain available independent of billing status.
- [ ] Production payments blocked until approved operator/MoR/tax decisions.
- [ ] Customer receipts and invoicing obligations documented; no card data stored.
