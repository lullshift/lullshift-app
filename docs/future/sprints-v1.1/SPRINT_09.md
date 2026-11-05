# Sprint 09 - Flutter Mobile & Organization Pilot

**Goal:** Flutter Mobile & Organization Pilot. **Duration:** indicative two weeks. **Owner:** implementation lead, with relevant Claude Code specialist agents.

## Dependencies and scope

Read `CLAUDE.md`, product and technical docs, and `sprints/ROADMAP.md`. Complete earlier sprint acceptance criteria. Do not implement later sprints or substitute unsupported provider behavior.

## Work items

- [ ] S09-T01: Build Flutter sign-in and device pairing via PKCE
- [ ] S09-T02: Implement status list, minimal push notifications, context and Companion screens
- [ ] S09-T03: Implement organization membership/role/policy baseline and web management
- [ ] S09-T04: Audit APNs/FCM payload privacy and mobile token handling
- [ ] S09-T05: Conduct end-to-end pilot, release/security/privacy review

## Acceptance criteria / demonstrable exit tests

- [ ] S09-A01: Mobile can view authorized status but not execute arbitrary agent approvals
- [ ] S09-A02: Push contains only opaque identifiers and generic message until unlock
- [ ] S09-A03: Organization isolation and role checks pass
- [ ] S09-A04: Commercial pilot supported by incident and support runbooks

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
Sprint: 09
Completed tasks (links/commits):
Commands run and observed results:
Tests not run and reason:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next sprint action:
```
