# Sprint 00 - Foundation, ADRs & Tooling

**Goal:** Foundation, ADRs & Tooling. **Duration:** indicative two weeks. **Owner:** implementation lead, with relevant Claude Code specialist agents.

## Dependencies and scope

Read `CLAUDE.md`, product and technical docs, and `sprints/ROADMAP.md`. Complete earlier sprint acceptance criteria. Do not implement later sprints or substitute unsupported provider behavior.

## Work items

- [ ] S00-T01: Create monorepo paths and readme with Tauri 2/React/Rust boundaries
- [ ] S00-T02: Pin toolchains and scaffold UI/Rust workspace with mock agent feed
- [ ] S00-T03: Add schema-first agent event JSON contract and fixtures
- [ ] S00-T04: Add build/test/lint/format CI and secret scan
- [ ] S00-T05: Write ADR-001 to ADR-005 and local developer bootstrap guide

## Acceptance criteria / demonstrable exit tests

- [ ] S00-A01: Fresh clone builds desktop scaffold and runs unit test suite
- [ ] S00-A02: Schema fixtures validate; CI checks fail correctly on a bad fixture
- [ ] S00-A03: No real credentials required; sample env uses placeholders

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
Sprint: 00
Completed tasks (links/commits):
Commands run and observed results:
Tests not run and reason:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next sprint action:
```
