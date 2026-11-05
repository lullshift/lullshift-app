# Sprint 01 - Mandatory Identity & Offline Entitlements

**Goal:** Mandatory Identity & Offline Entitlements. **Duration:** indicative two weeks. **Owner:** implementation lead, with relevant Claude Code specialist agents.

## Dependencies and scope

Read `CLAUDE.md`, product and technical docs, and `sprints/ROADMAP.md`. Complete earlier sprint acceptance criteria. Do not implement later sprints or substitute unsupported provider behavior.

## Work items

- [ ] S01-T01: Build .NET 10 modular identity service with Postgres and provider interfaces
- [ ] S01-T02: Implement verified email login (mock SMTP locally) plus Google/GitHub/Apple broker adapters with config
- [ ] S01-T03: Implement native external-browser PKCE login and protected callback handling
- [ ] S01-T04: Register devices and issue asymmetric signed offline leases
- [ ] S01-T05: Use OS keychain for tokens and test refresh/revoke/expiry/account linking

## Acceptance criteria / demonstrable exit tests

- [ ] S01-A01: No main UI operation before first successful account activation
- [ ] S01-A02: All four login choices visible; mocked flow tests pass; live providers marked pending if credentials absent
- [ ] S01-A03: 30-day prototype lease validates offline; expired lease requires sign-in without data deletion
- [ ] S01-A04: Provider-link takeover, replay, invalid signature and PKCE negative tests pass

## Engineering guardrails

- Account is mandatory for both Community and commercial clients; every provider is covered by interface and tests.
- No secrets in git, no covert source/prompt upload, no unapproved hook/agent changes.
- Maintain Windows/macOS/Linux compatibility in native code where applicable.
- Always represent unknown agent status accurately, and retain local user-owned data.

## Delegation guidance

- Use Opus 5.5 for architecture, identity, cryptography, cross-module risks, and acceptance review.
- Use Sonnet for scoped code generation, fixtures, UX components and tests.
- Delegate reviews to `.claude/agents/architecture-reviewer.md`, `.claude/agents/security-qa-reviewer.md` or other relevant agents.

## Sprint closeout template

```text
Sprint: 01
Completed tasks (links/commits):
Commands run and observed results:
Tests not run and reason:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next sprint action:
```
