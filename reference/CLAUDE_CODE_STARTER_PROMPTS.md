# Claude Code Starter Prompts

## Start the project (Opus)

Read `CLAUDE.md`, both docs, `sprints/ROADMAP.md` and `sprints/SPRINT_00.md`. Implement Sprint 00 only. First list 5 critical ambiguities and choose reversible defaults with ADRs. Then scaffold and actually run the required tests/build checks. Do not implement Sprint 01 until Sprint 00 acceptance is verified.

## Start a sprint (Opus)

Use `/sprint-delivery` for Sprint XX. Read its tasks and acceptance criteria. Delegate tightly scoped code to Sonnet and architecture/security reviews to specialist agents. Deliver vertical slices, run real tests and report blockers.

## Auth sprint review (Opus)

Use `/account-auth` to threat-model mandatory email/Google/GitHub/Apple login, account linking, external-browser PKCE, token storage and 30-day signed offline lease. Produce failing negative tests first. Never describe mocks as live federation.

## Add an adapter (Sonnet with Opus review)

Use `/agent-adapter` for PROVIDER. Read current provider documentation and generate a capability matrix. Create fixture-based tests for duplicate, reordered, partial and stale events. Preserve previous hooks and require consent to change them.

## Release gate (Opus)

Use `/quality-gate`. Enumerate every sprint acceptance criterion and present actual commands, results, failures and blockers. Invoke security-qa-reviewer. Refuse to mark any unexecuted test passed.

## Create an activity (Sonnet implementation; Opus security review)

Use `/activity-creator` and `creator/ACTIVITY_CREATOR_PROMPT.md` to generate an original pauseable grid puzzle. Verify manifest, all simulator states, restrictive permissions, keyboard accessibility, storage persistence and license inventory. Do not claim a signed package or successful publish unless tools exist and execute successfully.

## Implement the package platform (Opus architecture, Sonnet implementation)

Start Sprint 10. Read the manifest schema, security checklist, and architecture revision 1.1. First write ADR-022/023, security negative tests and threat model. Implement safe local pack/validate/simulate and a sandbox prototype. Do not build marketplace payment routes yet.

## Implement Stripe commerce (Opus review, Sonnet bounded modules)

Start Sprint 12 only after identity/entitlement prerequisites. Read `reference/COMMERCE_DOMAIN.md`. Confirm operator country and MoR are still unresolved; keep production disabled. Implement fake checkout, Stripe sandbox adapter behind explicit capability interfaces, verified webhook inbox, idempotent fulfillment and test evidence. Sprint 13 adds Connect only after legal and seller geography decisions.
