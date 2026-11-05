---
name: activity-creator
description: Create or revise Lullshift third-party activities and games from natural-language requests using the manifest, restricted SDK, local simulator and packaging workflow. Use for any AI-generated Lullshift activity.
---

# Lullshift Activity Creator

1. Read `reference/ACTIVITY_MANIFEST_V1.schema.json`, `docs/TECHNICAL_ARCHITECTURE.md` (Activity Sandbox), and `creator/ACTIVITY_CREATOR_PROMPT.md`.
2. Ask only for missing essential game/experience behavior; otherwise use a small, interruptible, accessible default.
3. Generate static HTML/CSS/TypeScript (bundled JS), no third-party runtime CDN, no Node/Electron/native module in the shipped package. If a dependency is used, inventory and verify its license.
4. Produce `manifest.json`, `index.html`, static assets, content, screenshot/preview information, README, licenses and tests. Declare only supported permissions.
5. Implement SDK pause/resume/state persistence. React to host commands; never query agent keys or source code.
6. Run local simulator sequences, including immediate needs_attention, duplicate events, close/crash/reload and offline; verify state restored safely.
7. Run actual CLI schema validation, build, static security checks and deterministic pack when commands exist. If toolchain isn't implemented, provide a clear scaffold and **do not claim passing tests**.
8. Present final `.lullpack` and publish metadata; require user confirmation before remote `publish` or any chargeable listing.

**Security rule:** Treat generated code as untrusted. Never work around sandbox restrictions. If the requested functionality needs privileged APIs, stop and propose a reviewed host capability ADR instead.
