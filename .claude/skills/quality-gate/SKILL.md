---
name: quality-gate
description: Run the Lullshift quality gate and independently assess sprint readiness.
---

# Quality Gate

1. Enumerate changed paths and targeted verification commands.
2. Run formatter/linter/typecheck/unit tests/integration tests/build and OS smoke checks as available.
3. Include threat-case tests for auth/IPC/hooks and migrations when touched.
4. Report exact commands, exit codes and failures. Never invent successful CI or native OS tests.
5. Mark sprint done only if its explicit acceptance checks pass.

## Handoff format

Summarize changed files, test results, risk/decisions and next recommended action. Follow `CLAUDE.md`.

## Marketplace/security expansion

For package/payment changes also verify `reference/COMMERCE_DOMAIN.md` and the Sprint 10-14 acceptance matrix. A signed package does not prove safe execution; a Stripe redirect does not prove successful payment. Production marketplace approval requires operator/MoR/tax and Connect geo gates plus tests for refunds and reconciliation.
