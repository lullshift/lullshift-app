# Sprint 05 - Context Keeper & Next Move

**Goal:** Context Keeper & Next Move. **Duration:** indicative two weeks. **Owner:** implementation lead, with relevant Claude Code specialist agents.

## Dependencies and scope

Read `CLAUDE.md`, product and technical docs, and `sprints/ROADMAP.md`. Complete earlier sprint acceptance criteria. Do not implement later sprints or substitute unsupported provider behavior.

## Work items

- [ ] S05-T01: Implement consent-gated ContextBookmark editor and retention controls
- [ ] S05-T02: Build re-entry card with original intent, notes and review checklist
- [ ] S05-T03: Build local Next Move suggestions and manual queue of follow-up decisions
- [ ] S05-T04: Add JSON/Markdown export, local deletion and session linking
- [ ] S05-T05: Test no accidental task-content cloud transmission

## Acceptance criteria / demonstrable exit tests

- [ ] S05-A01: Return card works with synthetic multi-session fixture
- [ ] S05-A02: User can export/delete local notes and see data retention rules
- [ ] S05-A03: No automatic prompt injection into active agents
- [ ] S05-A04: Context off means content is never silently captured

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
Sprint: 05
Completed tasks (links/commits):
Commands run and observed results:
Tests not run and reason:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next sprint action:
```
