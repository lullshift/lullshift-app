# Future (not in v1)

This folder keeps the original handoff v1.1 material for ideas deliberately **deferred** by ADR-001, ADR-002 and ADR-003:

- Lullshift accounts, federated login, device registry, offline leases
- Commercial cloud (.NET/PostgreSQL), sync and encrypted backup, hosted AI
- Flutter mobile companion and push notifications
- Organization/Business administration
- Hosted activity catalog, upload/moderation pipeline, CDN distribution
- Paid listings, Stripe Billing/Connect, creator payouts

Contents:

| Path | What it is |
|---|---|
| `PRODUCT_REQUIREMENTS_v1.1.*`, `TECHNICAL_ARCHITECTURE_v1.1.*` | Original v1.1 specifications (superseded for v1) |
| `COMMERCE_DOMAIN.md`, `PUBLISHING_SECURITY_CHECKLIST_v1.1.md` | Commerce domain rules, hosted publishing checklist |
| `sprints-v1.1/` | Original roadmap and Sprint 00–14 backlogs |
| `claude/` | Claude Code skills and agents for accounts, Stripe and hosted publishing (not loaded by Claude Code from here) |

**Rules:** nothing here is a requirement. Do not implement, stub or add abstractions for it. Reviving any item needs a new ADR from the product owner and must be additive: no v1 feature may later be moved behind an account or payment.
