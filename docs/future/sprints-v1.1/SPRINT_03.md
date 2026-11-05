# Sprint 03 - Claude Code Adapter

**Goal:** Claude Code Adapter. **Duration:** indicative two weeks. **Owner:** implementation lead, with relevant Claude Code specialist agents.

## Dependencies and scope

Read `CLAUDE.md`, product and technical docs, and `sprints/ROADMAP.md`. Complete earlier sprint acceptance criteria. Do not implement later sprints or substitute unsupported provider behavior.

## Work items

- [ ] S03-T01: Confirm current Claude Code hook payloads and supported event semantics
- [ ] S03-T02: Create hook forwarder and reversible installer with backups and consent
- [ ] S03-T03: Map hook events into normalized session state, including ambiguous stop
- [ ] S03-T04: Observe simultaneous sessions, handle missing/replayed events
- [ ] S03-T05: Write adapter fixtures and uninstall/upgrade tests

## Acceptance criteria / demonstrable exit tests

- [ ] S03-A01: Real Claude Code smoke test sees working/attention/turn events
- [ ] S03-A02: No duplicated critical attention events across replay
- [ ] S03-A03: Hooks restored after uninstall and previous config remains valid
- [ ] S03-A04: Provider capability matrix documented

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
Sprint: 03
Completed tasks (links/commits):
Commands run and observed results:
Tests not run and reason:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next sprint action:
```
