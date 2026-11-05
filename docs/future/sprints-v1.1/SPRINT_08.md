# Sprint 08 - Commercial Cloud & Sync

**Goal:** Commercial Cloud & Sync. **Duration:** indicative two weeks. **Owner:** implementation lead, with relevant Claude Code specialist agents.

## Dependencies and scope

Read `CLAUDE.md`, product and technical docs, and `sprints/ROADMAP.md`. Complete earlier sprint acceptance criteria. Do not implement later sprints or substitute unsupported provider behavior.

## Work items

- [ ] S08-T01: Implement plan entitlements and billing adapter with webhook verification
- [ ] S08-T02: Add encrypted sync protocol, devices and conflict-resolution handling
- [ ] S08-T03: Provide encrypted backup/restore and hosted AI boundary (feature flagged)
- [ ] S08-T04: Add observability, quotas, abuse limits and data export/delete APIs
- [ ] S08-T05: Deploy nonproduction Docker Compose/Postgres/object-store environment

## Acceptance criteria / demonstrable exit tests

- [ ] S08-A01: Free users retain all local features without paid plan
- [ ] S08-A02: Sync uploads opaque encrypted notes with no source code by default
- [ ] S08-A03: Payment webhooks idempotent; downgrade does not erase local data
- [ ] S08-A04: Account deletion/device revocation and tenant isolation tests pass

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
Sprint: 08
Completed tasks (links/commits):
Commands run and observed results:
Tests not run and reason:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next sprint action:
```
