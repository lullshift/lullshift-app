# Sprint 07 - Adapters, Hardening & Community Release

**Goal:** Adapters, Hardening & Community Release. **Duration:** indicative two weeks. **Owner:** implementation lead, with relevant Claude Code specialist agents.

## Dependencies and scope

Read `CLAUDE.md`, product and technical docs, and `sprints/ROADMAP.md`. Complete earlier sprint acceptance criteria. Do not implement later sprints or substitute unsupported provider behavior.

## Work items

- [ ] S07-T01: Research and implement supported Codex/Cursor lifecycle adapters with honest capabilities
- [ ] S07-T02: Add installers, signed update pipeline and release checklist
- [ ] S07-T03: Run full threat-case, privacy, migration and cross-platform QA
- [ ] S07-T04: Polish onboarding, login error/offline UI, documentation and accessibility
- [ ] S07-T05: Publish Community release candidate and feedback channels

## Acceptance criteria / demonstrable exit tests

- [ ] S07-A01: Capability gaps documented; no unsupported remote approval claims
- [ ] S07-A02: Smoke tests on Windows/macOS/Linux or clearly flagged blocked platforms
- [ ] S07-A03: Signed update validation and release provenance tests pass
- [ ] S07-A04: Community edition usable without paid subscription after login

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
Sprint: 07
Completed tasks (links/commits):
Commands run and observed results:
Tests not run and reason:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next sprint action:
```
