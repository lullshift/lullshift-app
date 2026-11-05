# Sprint 06 - Companion BYOK & Activity SDK

**Goal:** Companion BYOK & Activity SDK. **Duration:** indicative two weeks. **Owner:** implementation lead, with relevant Claude Code specialist agents.

## Dependencies and scope

Read `CLAUDE.md`, product and technical docs, and `sprints/ROADMAP.md`. Complete earlier sprint acceptance criteria. Do not implement later sprints or substitute unsupported provider behavior.

## Work items

- [ ] S06-T01: Implement secure BYOK provider settings and keychain storage
- [ ] S06-T02: Build Companion threads with scoped context permissions
- [ ] S06-T03: Define versioned TypeScript activity API and state persistence
- [ ] S06-T04: Bundle trusted activities only; design plugin sandbox ADR
- [ ] S06-T05: Test network failure, budgets, permissions and malformed plugins

## Acceptance criteria / demonstrable exit tests

- [ ] S06-A01: BYOK key never appears in SQLite, logs or UI error traces
- [ ] S06-A02: Context sharing requires explicit scope selection
- [ ] S06-A03: Activity SDK contract tests pass with sample activity
- [ ] S06-A04: Untrusted executable plugins remain disabled

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
Sprint: 06
Completed tasks (links/commits):
Commands run and observed results:
Tests not run and reason:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next sprint action:
```
