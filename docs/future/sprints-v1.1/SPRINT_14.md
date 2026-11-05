# Sprint 14 - Creator Experience and Marketplace Hardening

**Status:** planned | **Suggested lead:** Opus for architecture and final review; Sonnet for bounded implementation.


## Goal
Make AI-assisted community publishing usable and reliable, then prepare moderated commercial rollout.
## Work
- [ ] Complete Claude Code creator skill plus generic prompt, Codex and Cursor-compatible instructions.
- [ ] Ship 2-3 official creator templates: puzzle, break timer, decision card; provide example screenshots.
- [ ] Improve simulator and `publish` CLI with secure OAuth device/browser flow and status polling.
- [ ] Moderation queue, abuse reports, creator appeals, immutable release history and incident runbook.
- [ ] Package installation analytics using opt-in aggregate telemetry only.
- [ ] Validate accessibility, creator onboarding, pricing disclosures and cross-platform runtime.
## Acceptance
- [ ] Independent creator uses AI prompt to produce a passing `.lullpack` without manual host modification.
- [ ] Reviewer can reject, approve, delist and revoke with audit trail.
- [ ] Cross-platform package loading and interruptibility test matrix completed.
- [ ] Purchases/payouts unavailable in unsupported geographies; no fake success claims.
