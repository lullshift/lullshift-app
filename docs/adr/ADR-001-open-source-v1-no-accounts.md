# ADR-001: Fully open-source v1 with no accounts and no hosted services

- **Status:** Accepted
- **Date:** 2026-11-05
- **Owner:** Product owner
- **Supersedes:** Handoff v1.1 "mandatory Lullshift account", "signed offline lease" and the commercial edition model for the initial release

## Context

Handoff v1.1 required every user (Community included) to sign in with a hosted Lullshift account (email, Google, GitHub, Apple) before using the desktop app, with a 30-day signed offline lease. It also planned a commercial backend (.NET/PostgreSQL), Pro/Business plans, cloud sync, a Flutter mobile app, a hosted marketplace and Stripe Billing/Connect.

The product owner has decided the initial version must not be commercial.

## Decision

1. **v1 is fully open source** (Apache-2.0): desktop app, Rust core, agent protocol, activity SDK, CLI, kits, engines and all bundled games/activities.
2. **No account of any kind.** The app is fully functional on first launch with no sign-in, no registration, no device enrollment and no offline lease. There is no "anonymous mode" because there is no other mode.
3. **No Lullshift-operated server is required** for any v1 feature. The only network activity is user-initiated: Git fetches of marketplaces the user added (ADR-002), update checks for the app itself, BYOK Companion calls to the AI provider the user configured, and, only when the user asks, the Companion model list from models.dev (decision 32, ADR-020).
4. **No telemetry by default.** Any future diagnostics must be opt-in.
5. **Commercial and hosted ideas are deferred, not designed for.** Identity, sync, mobile, organization features, hosted AI, a hosted catalog and payments are recorded in `docs/future/` for possible later versions. v1 code must not add abstractions, feature flags or stubs for them.

## Consequences

- Removes the biggest adoption friction and the hosted-identity service dependency.
- Removes OAuth, token storage, lease cryptography, the .NET backend and Flutter from the v1 stack; the v1 stack is Tauri 2 + React/TypeScript + Rust + SQLite only.
- Without accounts, there is no publisher identity; trust in third-party content comes from Git source + commit pinning + sandbox (ADR-002, ADR-003).
- Secrets the app holds are limited to BYOK provider keys (OS keychain only).
- If commercial services are added later, they must be additive: no v1 feature may later be moved behind an account.

## Rollback

Reintroducing accounts requires a new ADR from the product owner. Material in `docs/future/` is a starting point, not an approved design.
