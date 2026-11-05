# Lullshift Commerce Domain and Payment Test Matrix

## Provider choice
The operator's incorporation jurisdiction is undecided, so Stripe is the preferred **conditional** provider. Keep domain objects provider-neutral, model the exact Stripe capabilities behind explicit interfaces and validate legal/Connect geography before merchant production activation. Never assume every international creator can receive transfers.

## Commerce invariants
- Do not trust client-submitted price, seller, currency, ownership or commission.
- Webhook signature is validated against raw payload before processing; event IDs deduplicated.
- Order state and entitlement grant are driven by durable verified provider events; browser redirect is display-only.
- Monetary amounts are integers in currency minor units; account for currencies with non-2 decimal precision.
- Reconcile provider events and settlement balances with persistent internal records.
- Initial one-time catalog purchases; recurring payments are Lullshift subscription plans. Creator subscriptions deferred.
- Official catalog requires a Lullshift account, paid publishing requires supported verified seller.
- Creator earnings, fees, refund clawbacks and payouts remain separate from subscription revenue and buyer entitlements.

## Required scenario tests
1. Checkout starts -> user closes browser -> no entitlement granted.
2. Provider event signed and successful -> entitlement granted exactly once.
3. Duplicate/reordered webhook messages -> no duplicate entitlement, commission, transfer or payout.
4. Forged or old webhook secret -> reject; no state mutation.
5. Price changed after order creation -> historical snapshot honored, no tampering.
6. Buyer buys activity already owned -> clear policy, no accidental duplicate charges.
7. Full/partial refunds -> correct ledger entries and entitlement state according to policy.
8. Dispute/chargeback -> audit and restricted seller settlement as applicable.
9. Seller onboarding in unsupported jurisdiction -> blocked before checkout publication.
10. Connect delayed payout or unavailable balance -> no fabricated paid status.
11. Fraudulent creator updates content -> quarantined; immutable existing release retained/revoked by moderation.
12. Offline customer -> locally valid signed entitlement respected within lease; refresh later.
13. Buyer account isolation -> cannot use another user's entitlement.
14. Payment provider downtime -> local core continues, payment attempts are recoverable.
15. Tax/MoR jurisdiction undecided -> production flag cannot be enabled.
