# Publishing and Security Checklist

- [ ] Creator account confirmed; publisher attribution is verified and separately scoped.
- [ ] Package metadata, schema, canonical paths, sizes, duplicates and compression limits pass.
- [ ] Third-party libraries, media, licenses and AI-generated-content disclosures are declared.
- [ ] Static analysis, dependency and malware scanning complete.
- [ ] WebView isolated origin, CSP, network denial and Tauri IPC denial tests complete on target OS.
- [ ] Activity host lifecycle, forced pause, resume and state migration pass.
- [ ] Human/moderation policy reviewed; all submissions begin in quarantine.
- [ ] Immutable approved version signed and assigned verified digest; package never changes in place.
- [ ] Approved content only reaches CDN; rollback/delist/revoke drills pass.
- [ ] For paid listing: seller KYC/KYB, seller country, MoR/tax/Connect eligibility and payouts are approved.
- [ ] Test purchase, refund, chargeback, reconciliation and offline entitlement behavior.
