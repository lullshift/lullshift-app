---
name: quality-gate
description: Run the Lullshift quality gate and independently assess sprint readiness.
---

# Quality Gate

1. Enumerate changed paths and targeted verification commands.
2. Run formatter/linter/typecheck/unit/integration tests/build and OS smoke checks as available.
3. Include threat-case tests when touched: local IPC, hook install/uninstall, sandbox denial, marketplace Git handling and malicious-repo fixtures, BYOK key handling, migrations.
4. For any activity, kit, engine or content change, check the activity and marketplace gates in `sprints/DEFINITION_OF_DONE.md`.
5. Confirm no account, hosted-service or default-telemetry dependency was introduced (ADR-001).
6. Report exact commands, exit codes and failures. Never invent successful CI or native OS tests. Mark a sprint done only if its acceptance checks pass.

## Handoff format

Summarize changed files, test results, risks/decisions and next recommended action. Follow `CLAUDE.md`.
