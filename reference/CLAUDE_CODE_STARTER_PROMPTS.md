# Claude Code Starter Prompts

## Start the project (Opus)

Read `CLAUDE.md`, both docs, `docs/adr/`, `sprints/ROADMAP.md` and `sprints/SPRINT_00.md`. Implement Sprint 00 only. First list the critical open technical choices and record reversible defaults as ADR-005 onward. Then scaffold and actually run the required tests/build checks. Do not start Sprint 01 until Sprint 00 acceptance is verified.

## Start a sprint (Opus)

Use `/sprint-delivery` for Sprint XX. Read its tasks and acceptance criteria. Delegate tightly scoped code to Sonnet and architecture/security reviews to specialist agents. Deliver vertical slices, run real tests and report blockers.

## Add an adapter (Sonnet with Opus review)

Use `/agent-adapter` for PROVIDER. Read current provider documentation and write a capability matrix. Create fixture-based tests for duplicate, reordered, partial and stale events. Preserve existing hooks and require consent to change them.

## Build the activity host and sandbox (Opus architecture, Sonnet implementation)

Start Sprint 03. Read ADR-003 and the sandbox section of the architecture doc. Write the sandbox strategy ADR (ADR-010) and denial tests first (Tauri IPC, network, filesystem, clipboard, navigation, cross-activity storage), then the lifecycle bridge and host-enforced pause. Record per-OS results honestly.

## Create an activity (Sonnet implementation; Opus security review)

Use `/activity-creator`, `creator/ACTIVITY_CREATOR_PROMPT.md` and `packages/activity-sdk/README.md`. Prefer a content pack if an engine fits; otherwise build on a kit or custom code. Every activity needs `help` in its manifest (a missing-help warning is a failure), a `LullshiftBridge.summary` call after meaningful changes, a `preview` image and a one-row controls layout. Run `pnpm lullshift validate <dir>` and `pnpm lullshift simulate <dir>` and report the real output. Do not claim passing checks unless they ran.

## Build Git marketplaces (Opus review, Sonnet implementation)

Start Sprint 08. Read ADR-002, `reference/MARKETPLACE_V1.schema.json` and the distribution section of the architecture doc. Write the malicious-repo fixtures and hardened Git invocation tests first, then add/install/update with commit pinning and the install review screen. Never store or log Git credentials.

## Release gate (Opus)

Use `/quality-gate`. Enumerate every sprint acceptance criterion and present actual commands, results, failures and blockers. Invoke security-qa-reviewer. Refuse to mark any unexecuted test as passed.
