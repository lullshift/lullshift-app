---
name: agent-adapter
description: Build or review a Lullshift adapter for external coding or business agents.
---

# Agent Adapter

1. Verify provider lifecycle API documentation and supported events; write a capability matrix.
2. Create normalized fixtures, idempotent mappings, stale/unknown semantics and bounded payload parsing.
3. Install hooks only with explicit consent, backup, atomic updates and uninstall path.
4. Implement secured native IPC and reconnect/replay handling.
5. Test duplicate, reordered, missing, malicious and partial provider events.
6. Never infer task completion from a turn-stop without provider evidence.

## Handoff format

Summarize changed files, test results, risk/decisions and next recommended action. Follow `CLAUDE.md`.
