# Sprint 04 - Interruption UX: Notifications, Break, Arcade

**Goal:** Interruption UX: Notifications, Break, Arcade. **Duration:** indicative two weeks. **Owner:** implementation lead, with relevant Claude Code specialist agents.

## Dependencies and scope

Read `CLAUDE.md`, product and technical docs, and `sprints/ROADMAP.md`. Complete earlier sprint acceptance criteria. Do not implement later sprints or substitute unsupported provider behavior.

## Work items

- [ ] S04-T01: Implement attention-priority native notifications, quiet hours and preferences
- [ ] S04-T02: Implement activity contract pause/resume/save and robust interrupted state
- [ ] S04-T03: Ship Break with away mode and one or two simple local games
- [ ] S04-T04: Test pause during active input, multi-session priority and focus UX
- [ ] S04-T05: Accessibility audit keyboard, labels and reduced motion

## Acceptance criteria / demonstrable exit tests

- [ ] S04-A01: Agent attention interrupts and saves activities without losing progress
- [ ] S04-A02: Do not show completions for mere turn-stop
- [ ] S04-A03: Works offline with valid lease; activity state restored on restart
- [ ] S04-A04: Screen reader and keyboard smoke checks documented

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
Sprint: 04
Completed tasks (links/commits):
Commands run and observed results:
Tests not run and reason:
Security/privacy review:
Remaining defects and risks:
Acceptance checks passed/failed:
Recommended next sprint action:
```
