# Sprint 13 - Paid Creator Marketplace and Stripe Connect

**Status:** planned | **Suggested lead:** Opus for architecture and final review; Sonnet for bounded implementation.


## Goal
Add one-time paid activities, verified sellers, commission ledger and provider-managed payouts without direct platform custody of pooled funds.
## Work
- [ ] ADR-021 Connect geography, charge types, seller/MoR tax model and commission base.
- [ ] Seller verification/KYC/KYB onboarding, supported country whitelist and seller states.
- [ ] Snapshot immutable listing/version, price, currency, tax, seller and fee policy at checkout.
- [ ] Stripe Connect adapter for approved charge/transfer design, reversals and disputes.
- [ ] Creator earnings dashboard, refunds, fee reversals and periodic provider reconciliation.
- [ ] One-time purchase grants signed entitlement; test offline limits and refund policy.
## Acceptance
- [ ] Unsupported sellers cannot create paid listings.
- [ ] Payment/refund/chargeback events reconcile exactly once.
- [ ] No paid content granted on mere redirect or unverified provider event.
- [ ] Fraudulent cross-seller payout/entitlement manipulation rejected.
- [ ] Sandbox payouts and live country eligibility separately verified; production remains locked pending compliance.
