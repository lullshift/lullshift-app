# Sprint 11 - Free Catalog and Publishing Pipeline

**Status:** planned | **Suggested lead:** Opus for architecture and final review; Sonnet for bounded implementation.


## Goal
Authenticated free public marketplace with reviewed and signed creator packages.
## Work
- [ ] Catalog/Publisher/Submission/Moderation modules in .NET modular monolith and PostgreSQL.
- [ ] Lullshift account-backed publisher profile, submission quotas and rate limits.
- [ ] Quarantine S3 upload; verification worker, manifest/license/dependency checks and reviewed publishing.
- [ ] Immutable versions, signed CDN downloads, install/update/uninstall UX, moderation takedown.
- [ ] Creator dashboard: submission status and findings; basic catalog search and filters.
## Acceptance
- [ ] Non-owner cannot alter publisher or submission.
- [ ] Unapproved package is not downloadable or executable.
- [ ] Hash/signature and identity mismatch fail installation.
- [ ] Revoked version is blocked for future installs; offline semantics documented.
- [ ] Free catalog works for Community account, not only Pro.
