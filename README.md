# Lullshift - Implementation Handoff Kit

This archive is a **documentation and Claude Code execution kit**, not a runnable Lullshift application.

## What is included

- `docs/PRODUCT_REQUIREMENTS.md` and `docs/PRODUCT_REQUIREMENTS.pdf`: product vision, user journeys, feature scope, account requirements and open-source/commercial boundary.
- `docs/TECHNICAL_ARCHITECTURE.md` and `docs/TECHNICAL_ARCHITECTURE.pdf`: Tauri 2/Rust/React, .NET hosted services, Flutter, OAuth/PKCE, security, data model, protocols.
- `CLAUDE.md`: governing implementation instructions for Claude Code.
- `.claude/agents/*.md`: eight specialist subagents covering architecture, desktop/Rust, identity/cloud, security/QA, activities, creator experience, and marketplace commerce.
- `.claude/skills/*/SKILL.md`: nine project skills for sprint delivery, adapters, authentication, activity contracts/creation, publishing, Stripe commerce, QA and ADRs.
- `sprints/`: roadmap, sprint 00-14 backlogs, definition of done and acceptance criteria.
- `reference/`: agent event and activity manifest schemas, commerce domain rules, publishing security checklist and Claude handoff prompts.

## Quick start

1. Create a new empty Git repository and extract all contents at the repository root (preserve hidden `.claude/` folder).
2. Open the repository in Claude Code. Start with **Opus 5.5** to review the two docs, record ADRs and implement Sprint 00.
3. Use **Sonnet** for bounded implementation tasks, tests and repetitive UI work; use Opus for architecture, security and cross-module changes.
4. Begin each sprint using `/sprint-delivery` and its `sprints/SPRINT_XX.md` acceptance criteria.
5. Require actual commands/tests to pass; no artificial 'done' without build and smoke test evidence.

**Suggested initial prompt:** `Read CLAUDE.md, docs/PRODUCT_REQUIREMENTS.md, docs/TECHNICAL_ARCHITECTURE.md, and sprints/ROADMAP.md. Implement Sprint 00 only. Record unresolved choices as ADRs. Do not scaffold future sprint features yet.`

## Mandatory product decision

Every user (Community included) **must have a Lullshift account**, via verified email, Google, GitHub or Apple. Initial online authentication is required. Community activities run locally after sign-in, including through temporary network outages with a signed, bounded offline lease. Core features are open source; hosted identity is a real service dependency. Preserve both statements in all implementation and marketing materials.

## Clarifications and licensing

- Lullshift is a **working name** pending trademark/domain checks.
- Apache-2.0 will apply to the client and SDK when LICENSE is added to the actual repo. The handoff kit does not substitute for a formal legal review.
- Hosting/service subscription is commercial, while open-source fork rights remain intact.
- Validate all provider-specific OAuth and Claude Code lifecycle contracts during implementation; the handoff intentionally avoids hard-coding unverified SDK version assumptions.

## Revision 1.1 - Creator & Global Marketplace

This handoff now includes a first-class open-source **Creator SDK/CLI/simulator**, `.lullpack` specification, AI-assisted creator skills and prompts, hosted marketplace publishing/moderation, paid creator commerce and Stripe Billing/Connect (conditional on future eligible non-Turkish company incorporation). Read the 1.1 revisions at the bottom of the product and architecture docs and Sprints 10-14. No legal jurisdiction, merchant-of-record allocation or paid creator country list has been silently finalized.
