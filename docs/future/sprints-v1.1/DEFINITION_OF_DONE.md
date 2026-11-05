# Definition of Done and Release Gates

A sprint is only complete when observed behavior meets documented acceptance criteria, the correct test suite has been executed, and risks are documented. Claude Code cannot declare success from generated code alone.

## Mandatory gates

- [ ] Unit tests, integration tests and type/lint/format checks pass where applicable.
- [ ] User stories tested on the development OS; release gates include cross-platform smoke results.
- [ ] Provider APIs and versions validated against current official documentation.
- [ ] New state changes tested with duplicate, reordered, missing and malicious inputs.
- [ ] Account enrollment, required login, offline lease, cancellation, failed login and sign-out behavior verified.
- [ ] Auth data is kept in OS secure store, never app/local database plaintext.
- [ ] User privacy: no implicit context capture, prompts/source upload, hidden employee metrics.
- [ ] Agent hooks are installed/uninstalled reversibly with user consent.
- [ ] Migration and rollback paths tested for persistent changes.
- [ ] Keyboard, screen-reader labels and error/offline states inspected.
- [ ] Documentation, ADR and release notes updated.

## Required QA evidence

Capture the actual command and exit code, environment, fixture identifiers, failed/blocked checks and manual steps. Report platform gaps honestly. For live OAuth integrations, list provider configuration and test-account prerequisites rather than pretending they passed with mocks.

## Creator/marketplace-specific release gates

- All downloadable activities are strictly sandboxed without privileged Tauri access; tests prove denial, not only happy-path rendering.
- `.lullpack` signatures/hash/version/paths verified; invalid, oversized, and malicious archives rejected.
- Creator packages pass CLI simulation, pause/resume/persistence, accessibility and content-license checks.
- Publish flow includes quarantine, moderation, signed immutable release, security revocation and safe update rollback.
- Paid flows have legal merchant-of-record, tax, seller-country and payout approvals before production enablement.
- Real-money entitlement grants require signature-verified provider events; duplicate events remain idempotent.
- Webhook replay, refund/dispute, reconciliation and cross-account authorization tests documented.
