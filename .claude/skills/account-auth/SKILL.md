---
name: account-auth
description: Implement or review mandatory Lullshift account, federated login, offline lease and device security.
---

# Account Auth

1. Preserve mandatory sign-in for Community and paid editions; no anonymous official build.
2. Implement email verification plus Google, GitHub and Apple via hosted identity broker.
3. Use external system browser, authorization code + PKCE, state, nonce and strict redirect validation.
4. Link identities only with authenticated user proof. Use provider subject as stable key, not email.
5. Store native refresh tokens in OS secure storage; rotate/revoke tokens and bind devices.
6. Sign time-limited offline lease asymmetrically; handle expired/revoked/clock-skew states without erasing local data.
7. Test account takeovers, token replay, offline behavior and cloud outage.
8. Use mock providers until production credentials are explicitly supplied.

## Handoff format

Summarize changed files, test results, risk/decisions and next recommended action. Follow `CLAUDE.md`.
