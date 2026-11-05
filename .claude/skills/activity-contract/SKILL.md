---
name: activity-contract
description: Build Lullshift activities with consistent interruption, privacy and state-persistence rules.
---

# Activity Contract

1. Implement initialize/start/pause/resume/saveState/dispose lifecycle and test rapid event transitions.
2. Keep local activity data namespaced and serializable in SQLite.
3. Treat agent needs_attention as priority; notify and pause within timing target.
4. Use accessible input and reduced-motion options.
5. Ship only trusted bundled activities until plugin sandbox and permissions have security review.
6. Request explicit consent before reading task context or calling AI services.

## Handoff format

Summarize changed files, test results, risk/decisions and next recommended action. Follow `CLAUDE.md`.

## Revision 1.1: Distributed activities

Read `reference/ACTIVITY_MANIFEST_V1.schema.json` and `sprints/SPRINT_10.md`. Require immutable static packages, sandbox isolation, signature/hash verification, host-owned interruptibility and safe state rollback. Run schema/path traversal, cross-origin bridge denial and forced-interruption simulations. Never add privileged plugin support to bypass activity API constraints.
