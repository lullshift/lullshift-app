---
name: marketplace-publishing
description: Prepare, validate, submit and review Lullshift activity packages for free or paid catalog distribution with approval and licensing checks.
---

# Marketplace Publishing

1. Read `docs/PRODUCT_REQUIREMENTS.md` revision 1.1, `reference/ACTIVITY_MANIFEST_V1.schema.json`, and `sprints/SPRINT_11.md`.
2. Verify source/license inventory, AI-generation disclosure, creator rights and publisher identity.
3. Run local CLI simulation/security tests and check manifest/permissions/offline claims.
4. Upload only through authenticated quarantine endpoint; never point public CDN to an unapproved upload.
5. Apply server checks, moderation and immutable version signing before publication.
6. If paid, require verified seller and successful compliance/Connect capability gates from Sprint 13; do not confuse free creator registration with KYC/KYB.
7. Test delisting and emergency revocation. Report actual evidence and any manual reviewer steps.
