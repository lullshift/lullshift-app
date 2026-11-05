# Sprint 10 - Activity Packages and Creator Runtime

**Status:** planned | **Suggested lead:** Opus for architecture and final review; Sonnet for bounded implementation.


## Goal
Implement an open-source `.lullpack` v1 specification and secure local install/runtime. Creator tools shall generate, simulate and validate apps independently of cloud services.
## Work
- [ ] Document ADR-022/023 and per-OS isolation proof strategy.
- [ ] Implement strict manifest JSON schema, archive unpack safeguards, hash/signature and compatibility verification.
- [ ] Build restricted activity host bridge; deny privileged Tauri and arbitrary network/filesystem access.
- [ ] Implement trusted packaged sample games, namespaced persistence, migration/rollback and crash handling.
- [ ] Create CLI commands `create`, `dev`, `simulate`, `validate`, `pack` (local; no production credentials).
- [ ] Add activity-creator skill/templates and event-sequence simulator.
## Acceptance
- [ ] Tampered/oversized/path-traversal archives fail closed.
- [ ] Needs-attention preempts plugin reliably without plugin cooperation.
- [ ] Tests demonstrate denied Tauri IPC, filesystem, network and cross-activity storage.
- [ ] Deterministic build produces stable package hash from identical inputs.
- [ ] Bundled activities run offline and recover state after crash.
