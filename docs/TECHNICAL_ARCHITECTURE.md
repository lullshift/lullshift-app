# Lullshift - Technical Architecture and Stack

**Version:** 2.0 | **Date:** 2026-11-05 | **Audience:** Engineering and Claude Code implementation agents

**Governing decisions:** [ADR-001](adr/ADR-001-open-source-v1-no-accounts.md), [ADR-002](adr/ADR-002-git-repo-marketplaces.md), [ADR-003](adr/ADR-003-activity-model-kits-engines-content.md). Where this document and an accepted ADR disagree, the ADR wins. The v1.1 cloud/commerce design is kept unchanged in [`docs/future/TECHNICAL_ARCHITECTURE_v1.1.md`](future/TECHNICAL_ARCHITECTURE_v1.1.md) and is not part of v1.

## 1. Executive architecture

Lullshift v1 is a **local-first, desktop-only Tauri 2 application**, fully open source under Apache-2.0. It observes external coding agents through hook adapters on the user's computer, keeps agent sessions and user context in a local SQLite database, and offers interruptible activities while agents work. There is **no account, no sign-in, no device enrollment, no offline lease and no Lullshift-operated server**. The app is fully functional on first launch.

The only network traffic is **user-initiated**:

- Git fetches of marketplaces the user added (system `git`, user's own credentials).
- Update checks and downloads for the app itself (signed updater artifacts).
- BYOK Companion calls to the AI provider the user configured.

There is no telemetry by default; any future diagnostics must be opt-in (ADR-001).

```text
               User's computer (all Apache-2.0, no Lullshift server)
+-----------------------------------------------------------------+
| React + TS UI: tray, Context Keeper, Next Move, Companion,      |
|   activity library, marketplace + install review, settings      |
+--------------------------+--------------------------------------+
                           | typed Tauri commands/events
+--------------------------v--------------------------------------+
| Rust core: agent-core (reducer/state machine), agent-ipc,       |
|   agent-adapters, local-store (SQLite), activity-runtime,       |
|   marketplace (git/index/validate/install/pin), keychain (BYOK) |
+----^----------------+-------------------+----------------+-------+
     | local IPC      | nonce-bound       | hardened git   | HTTPS
     | (UDS/pipe)     | postMessage       | (user-init.)   | (user-init.)
+----+-----------+ +--v---------------+ +-v--------------+ +v-------------+
| Hook forwarder | | Sandboxed        | | Git remotes    | | BYOK AI      |
|   ^ external   | | activity WebViews| | (https/ssh,    | | provider     |
|   agents hooks | | no IPC/net/fs    | | user creds)    | | (Companion)  |
+----------------+ +------------------+ +----------------+ +--------------+
```

**Architectural principles**

- The agent runtime is external; Lullshift observes and guides human attention, it does not orchestrate agents.
- Rust core and SQLite are authoritative; the UI stays responsive when agents, Git remotes or the AI provider are unavailable, and shows unknown/stale state honestly.
- No source code or raw prompts leave the machine unless the user sends selected context to their own BYOK provider.
- v1 code must not add abstractions, feature flags or stubs for accounts, sync, mobile, hosted catalogs or payments (ADR-001 §5).

## 2. Stack decisions

| Layer | Technology | Notes |
|---|---|---|
| Desktop runtime | Tauri 2 | system tray, notifications, autostart, signed updates |
| Desktop UI | React + TypeScript + Vite | React Router, typed command/event boundaries |
| Styling | Tailwind CSS + shadcn/ui | accessible primitives; keyboard-first; reduced-motion support |
| UI state | Zustand | ephemeral view state only; Rust/SQLite authoritative |
| Rust core | Rust stable, serde, tokio | SQLite library (`rusqlite` vs `sqlx`) chosen by ADR after spike |
| Local database | SQLite | versioned migrations, WAL, transactions; no secrets |
| Local agent IPC | Unix domain socket / Windows named pipe | authenticated local endpoint; no unauthenticated TCP listener |
| Adapter packaging | Native hook forwarder + Rust adapter registry | explicit capability reporting |
| Activity runtime | Isolated WebViews + versioned host bridge | per-OS sandbox proof; third-party code with risk warning and consent (§6.4) |
| Secrets | OS keychain via native API | BYOK provider keys only; never browser storage, SQLite or logs |
| Local AI | Provider abstraction, BYOK HTTP clients | user-initiated calls only |
| Marketplaces | System `git` executable (hardened invocation) | uses the user's existing Git credentials; library alternative evaluated by ADR |
| Creator tooling | TypeScript SDK, kits, CLI, simulator | CLI language/distribution chosen by ADR |
| Tests | Vitest, Playwright, `cargo test` | plus fixture-driven adapter and malicious-repo tests |
| Checks | Local check script + opt-in Git hooks (ADR-004) | fmt/lint/typecheck/test/build/schemas/secret scan; no hosted CI for now |

Explicitly **not** in the v1 stack: .NET/ASP.NET Core, PostgreSQL, OIDC/OAuth brokers, Flutter, a React web console, push services, billing/payment providers, object storage/CDN, Docker deployment, cloud OpenTelemetry export.

Use explicit version pins only after bootstrap resolves stable compatible releases, verified against official documentation. Record framework constraints in ADRs.

## 3. Repository design

Single open-source repository, `lullshift-app`. There is no separate commercial repository in v1.

```text
lullshift-app/
  CLAUDE.md, LICENSE (Apache-2.0)
  docs/                       # PRD, architecture; adr/ (ADRs); future/ (deferred)
  .claude/{agents,skills}/    # delegations and workflow skills
  .lullshift/marketplace.json # built-in marketplace index ("lullshift")
  apps/desktop/               # Tauri shell + React UI
  crates/agent-core/          # event model, dedupe, state reducer
  crates/agent-adapters/      # hook adapters, install/uninstall
  crates/agent-ipc/           # UDS / named pipe server, peer auth
  crates/local-store/         # SQLite migrations and repositories
  crates/activity-runtime/    # sandbox host, bridge, lifecycle, quotas
  crates/marketplace/         # git fetch, index, validate, install, pin
  packages/agent-protocol/    # versioned event schemas/types
  packages/activity-sdk/      # lifecycle + bridge client (TS)
  packages/activity-kits/{grid,card,arcade}/  # optional kits
  tools/lullshift-cli/        # create / dev / simulate / validate / pack
  creator/                    # provider-independent creator prompts
  activities/{games,engines,content}/*        # bundled activities
  reference/                  # JSON Schemas (manifest, marketplace, events)
  tests/fixtures/             # hook payloads, event replay, malicious repos
  sprints/                    # implementation backlog
```

Every item under `activities/` is listed in `.lullshift/marketplace.json`, which makes this repository the built-in marketplace (ADR-002 §6).

## 4. Domain model and event protocol

### 4.1 Entities (all local, SQLite)

- `AgentSession`: UUID, adapter, provider session ID, workspace alias, state, observed timestamps, confidence.
- `AgentEvent`: event UUID, session ID, event type, sequence, provider timestamp, receipt timestamp, payload reference.
- `ContextBookmark`: session, intent, approved notes, review items, retention policy.
- `NextMoveItem`: session, content, status, created time.
- `CompanionThread`: thread, provider reference (no key material), user-approved context scopes.
- `AdapterInstallation`: adapter ID, target config path, backup reference, installed version, consent timestamp, status.
- `Marketplace`: ID (short name), Git URL, `added_at`, `last_fetched_commit`, `builtin` flag. The built-in `lullshift` marketplace is always present and cannot be removed.
- `InstalledActivity`: activity ID, marketplace ID, source (path within marketplace repo, or external Git URL + path), pinned commit SHA, manifest version, `kind` (`app` | `content`), engine reference (`id@major`, content only), `enabled`, `installed_at`, content digest of the installed folder.
- `ActivityInstance`: activity ID, optional session link, lifecycle status, timestamps.
- `ActivityState`: activity ID, namespace, activity-declared schema version, snapshot, size, `saved_at`. Host-enforced quota; last committed snapshot kept for crash recovery.

No `Account`, `ExternalIdentity`, `Device`, `EntitlementLease`, `Organization` or `SyncItem` entities exist in v1.

### 4.2 Event schema (illustrative, version 1)

Canonical schema: `reference/AGENT_EVENT_V1.schema.json`; TypeScript/Rust types generated or mirrored in `packages/agent-protocol`.

```json
{
  "schemaVersion": 1,
  "eventId": "uuid",
  "provider": "claude-code",
  "providerSessionId": "opaque",
  "eventType": "needs_attention",
  "timestampUtc": "2026-11-05T10:00:00Z",
  "correlationId": "opaque",
  "capabilities": ["lifecycle", "permission_prompt"],
  "metadata": { "reason": "permission_request" }
}
```

Contract rules: preserve raw provider messages locally only when needed and allowed; validate bounded input (size, depth, string lengths); dedupe by stable event ID; tolerate reordered and duplicated callbacks; state changes are idempotent. A stop/turn-end event can mean `idle` or `unknown`, not necessarily `completed`. Never log raw task content.

### 4.3 State machine

States: `unknown`, `idle`, `working`, `needs_attention`, `completed`, `failed`, `cancelled`. Explicit transitions with timestamps and source confidence; providers can emit only partial signals. Restore sessions as `unknown` after restart until confirmed. A stale timeout moves a session to `unknown` rather than producing a false success notice. Transition to `needs_attention` is the highest-priority signal and drives host-enforced activity freeze (§6.4).

## 5. Local secrets and trust boundaries

There are **no accounts and no hosted identity** (ADR-001). Consequently:

- **The only secrets the app holds are BYOK AI provider keys.** They live exclusively in the OS keychain (Windows Credential Manager, macOS Keychain, Linux Secret Service). SQLite stores only a non-secret reference. Keys never appear in SQLite, logs, crash reports, activity bridges, exported data or the UI after entry.
- **Git credentials are never handled by Lullshift.** System `git` uses the user's own credential manager, SSH agent or `gh` auth; Lullshift does not prompt for, store, read or transmit tokens (ADR-002 §3).
- **Local IPC authentication remains required** for hook forwarder -> app traffic (see table).
- **Activities are untrusted.** Code and data from any non-built-in marketplace is attacker-controlled (§6.4, §7).

| Boundary | Threat | Control |
|---|---|---|
| Hook forwarder -> core | Local malicious process injecting events | user-only socket/pipe ACL, per-install secret, peer checks, validation, rate limits |
| Core -> agent configs | Corrupting user agent configuration | explicit consent, backup, atomic merge, idempotent install, restorable uninstall |
| Core -> keychain | Key theft from disk/logs | OS keychain only, redaction, no key in IPC to WebViews |
| Core -> activity WebView | Sandbox escape, data exfiltration | isolated origin, no Tauri IPC, no network, no fs, CSP, nonce-bound bridge, quotas |
| Core -> git remote | Malicious repository content or git config | hardened `git` invocation, app-owned dirs, strict validation (§7.3) |
| Core -> BYOK provider | Over-sharing context | user-selected context scopes, preview before send, TLS verification never disabled |
| App updater | Tampered update | signed updates verified by the Tauri updater |
## 6. Local runtime, adapters and activities

### 6.1 Runtime and adapters

- Tauri holds tray and windows; Rust core stays alive when windows are hidden; shutdown explicitly releases hooks/IPC.
- The hook forwarder is a small packaged native executable invoked by supported agent tools, with timeouts, structured JSON and bounded logs. It must fail fast and never block the agent when the app is not running.
- Existing agent configurations are read, backed up and merged atomically, with uninstall restoration and explicit user consent. Install/uninstall is idempotent.
- Adapter capabilities: `canObserveLifecycle`, `canObserveAttention`, `canObserveCompletion`, `canProvideTaskSummary`, `canRemoteApprove` (default false; out of scope for v1).
- Gracefully degrade when a provider lacks support. Avoid UI/terminal scraping as a default integration strategy.
- Keep the core protocol independent from the Tauri UI so it could later run as a local daemon.

### 6.2 Activity model (ADR-003)

Three tiers, chosen by the creator: **custom code activity** (any static web app implementing the SDK lifecycle), **code activity on a kit**, and **content pack** (JSON data plus a preview image for a named engine; runs no code; the only other media are optional, listed, checked still PNG/WebP pictures, ADR-016).

**Manifest v1** (`reference/ACTIVITY_MANIFEST_V1.schema.json`) gains `kind: "app" | "content"`:

- `kind: "app"` requires `entrypoint` and `runtime` (`isolated-web-v1`), `sdkVersion`, `permissions` and `interruptibility`.
- `kind: "content"` requires `engine` (`id@major`, e.g. `com.lullshift.engine.breathing@1`) and `data` (path to the JSON data file within the pack) and **forbids executable files** (no `.js`, `.mjs`, `.wasm`, `.html`, native binaries, scripts). In v1 a pack holds only its manifest, data file (at most 256 KiB), a preview image, `.md`/`.txt` notes and up to 20 still PNG/WebP pictures listed in `images` (each at most 512 KiB and 2048 × 2048 pixels; validated, fully decoded and served only at the pack's own `images/` path; ADR-016 amendment).

Illustrative content pack manifest (an `app` manifest instead carries `entrypoint`, `runtime`, `sdkVersion`, `permissions` and `interruptibility`):

```json
{
  "schemaVersion": 1,
  "kind": "content",
  "id": "com.example.box-breathing",
  "version": "1.0.0",
  "name": "Box Breathing",
  "category": "break",
  "engine": "com.lullshift.engine.breathing@1",
  "data": "data.json",
  "minHostVersion": "0.3.0",
  "offline": true
}
```

The manifest is author-controlled and therefore untrusted. Host-derived metadata (marketplace, source URL, pinned commit, installed digest, built-in flag) lives in `InstalledActivity`, never in the manifest.

### 6.3 Lifecycle bridge

Every code activity (custom, kit-based or engine) implements the SDK lifecycle:

`initialize -> start -> pause -> resume -> saveState -> restoreState -> dispose`

- `initialize` receives host capabilities, locale, reduced-motion/theme preferences and (engines) the validated pack data.
- `pause` may come from the user or the host; the host does not wait for acknowledgment before freezing (§6.4).
- `saveState` returns a quota-bounded snapshot; `restoreState` accepts it or rejects cleanly (fresh start, user informed). `dispose` is best-effort; the host destroys the WebView regardless.
- Real-time games use forced pause plus a resume countdown (arcade kit).

### 6.4 Activity sandbox and host bridge

Treat all third-party and AI-generated activity JavaScript as attacker-controlled.

- Load each activity into a **dedicated isolated origin / WebView** with **no Tauri IPC** and no access to the main UI's origin or storage.
- Strict **CSP**: only the activity's packaged assets via a host-controlled local scheme; no `eval`; no remote navigation, frames or popups.
- **No network** (default deny, including fetch, WebSocket, WebRTC, beacons, remote fonts/images). **No filesystem.** No shell, no clipboard without an explicit grant, no notifications API, no agent data or BYOK key exposure.
- Expose only a **versioned, origin-bound, nonce-validated `postMessage` bridge** for lifecycle, namespaced storage and declared permissions. The host validates every message against a schema, size limit and rate limit; unknown messages are dropped and counted.
- **Storage quota** per activity namespace; snapshots exceeding quota are rejected.
- **Host-enforced freeze:** on `needs_attention` (or when the user switches away), the host requests `saveState`, then freezes the WebView (suspends timers/rendering, blocks input, hides or overlays) without relying on activity cooperation. A crash or unresponsive activity is killed; the last committed snapshot is kept for recovery.
- Bound CPU and memory where the OS/WebView allows; enforce timeouts on lifecycle calls.
- Sensitive agent context is never a default permission; any future context capability needs its own ADR and threat model.

**Per-OS proof gate:** WebView isolation guarantees differ across WebView2 (Windows), WKWebView (macOS) and WebKitGTK (Linux). Third-party **code** activities (`kind: "app"` from any non-built-in marketplace) stay disabled until sandbox denial tests pass on all three targets. If the guarantees are insufficient on an OS, block third-party code there rather than shipping a cosmetic sandbox. **Content packs can be enabled earlier** because they contain no code; their data is still untrusted input to a first-party engine (§6.6).

**Amended by owner decision 27.5 (ADR-015 and ADR-012 amendments):** third-party code activities can be installed on all three platforms after a clear risk warning and explicit consent at each install and at each update that changes their pinned commit. The sandbox itself is unchanged; the warning states honestly what is verified on the user's platform (Windows: proof passed, WebRTC block not read back; macOS and Linux: block in place, proof not yet run; or that the block failed).

### 6.5 Kits

Kits (`packages/activity-kits/{grid,card,arcade}`) are optional first-party TypeScript libraries **compiled into the activity bundle**; the host never loads kit code separately or trusts an activity because it uses a kit. Kits share tested implementations of keyboard navigation, undo, seeded generation, forced pause and resume countdown.

### 6.6 Engines

An engine is a **first-party code activity** that renders a family of experiences from data (breathing, movement routine, guided pause, quiz, brain teaser, reflection prompts, creative toy, typing drills, away prompts).

- Referenced by `id@major`; each major publishes a **public data JSON Schema** beside its `engine.json` (`activities/engines/<name>/data.schema.json`), with valid and invalid fixtures in `tests/fixtures/engines/<id>@<major>/`.
- Breaking data changes require a new major, shipped beside the old one. **Old majors keep running installed packs**; no major is removed within v1, and the app ships every major any bundled or installed pack uses (ADR-016).
- The app lists each pack as its own activity and serves the engine's files at the pack's address, so snapshots, storage and time played belong to the pack. Engines are hidden. Before starting a pack, the host resolves `id@major` and re-validates the data against the engine schema in Rust; a pack that fails shows a calm error and never starts (ADR-016).
- Pack data is untrusted: render text as text (no HTML injection), bound lists and durations. Packs run inside their engine's sandboxed WebView.

### 6.7 Host features (not activities)

Context Keeper, Next Move and Companion need task context and/or AI access, so they are **host features** in the main UI, not sandboxed activities, and are not distributed through marketplaces.

## 7. Distribution and marketplaces (ADR-002)

### 7.1 Marketplace model

- A **marketplace is any Git repository** with an index at `.lullshift/marketplace.json`, validated against `reference/MARKETPLACE_V1.schema.json`.
- Each index entry points to either a **folder in the same repo** or **another Git URL + path**, with an optional `ref`. A `ref` is only a resolution hint: at install time it is resolved to a commit SHA and the SHA is pinned. Branch heads are never executed directly.
- Any Git URL works (GitHub, GitLab, self-hosted); `owner/repo` is shorthand for GitHub. Public and private repositories are both supported.
- **No central review.** The install UI labels third-party marketplaces as unverified. Only the built-in marketplace is reviewed (by pull request).

### 7.2 Git access and hardened invocation

Lullshift uses the **system `git` executable** so the user's credential manager, SSH keys and `gh` auth work for private repos; Lullshift never stores or transmits tokens. Transport is plain `clone`/`fetch`, not a provider REST API.

Every invocation is hardened:

- Runs in an **app-owned cache directory**, one directory per marketplace/source; never inside a user repository.
- `-c core.hooksPath=<empty app-owned dir>` (no hooks), `-c core.fsmonitor=false`, `-c protocol.allow=never` plus `protocol.https.allow=always`, `protocol.ssh.allow=always`, `protocol.file.allow=always`; no `ext::` or other transports.
- No submodule recursion; no Git LFS (v1); no custom config read from the repository; `GIT_CONFIG_NOSYSTEM` and a controlled environment where possible without breaking the user's credential helpers.
- Shallow/partial clone (`--depth`, `--filter=blob:none`) where possible.
- Timeouts, output bounds and UI cancellation on every call; no terminal prompts (credential helpers may show their own UI).
- Argument vector, never a shell; URLs and refs validated (reject leading `-`).
- Failures (offline, auth denied, repo gone, force-push) are reported honestly; installed activities keep working.

### 7.3 Install flow

1. **Fetch** the marketplace repo into the cache; record `last_fetched_commit`.
2. **Resolve** the entry: parse and validate `marketplace.json` strictly; for external-repo entries, fetch that repo too.
3. **Pin** the resolved commit SHA for the pack source.
4. **Copy** the pack folder from the pinned tree into an app-owned install directory (never execute from the cache).
5. **Validate strictly**: manifest against schema; `kind` rules; canonical relative paths only (no `..`, absolute paths, drive letters, case-collisions, reserved names); no symlinks or hard links escaping the pack folder (v1: reject all links); file count and total/per-file size limits; allowlisted file types; **no executables in content packs**; content data against the referenced engine major's data schema; referenced engine must exist in the host; declared permissions must be known.
6. **Install review** shows source marketplace and repo URL, commit SHA, kind (content vs code), engine, requested permissions, size, and an **"unverified third-party"** notice for non-built-in sources. For third-party code it also shows a risk warning ("This is someone else's program") and an unticked consent checkbox; Install stays disabled, and the host refuses the install, until the user ticks it (owner decision 27.5, ADR-015 amendment).
7. **Enable** on explicit user confirmation; record `InstalledActivity` with the installed digest.

**Updates** are explicit: fetch, show a diff summary (version, commit, permission and kind/engine changes), re-validate, swap atomically, keep the previous version until the new one works. Removal deletes the install directory and, with consent, its `ActivityState`.

### 7.4 Built-in marketplace

- `lullshift` (this repository) is the built-in marketplace; its activities are **bundled into the installer at the app's release tag**, validated at build time and work offline.
- They **cannot be removed, only disabled**, and **update with the app** (not via Git fetch).
- Reviewed via pull request, they are the only code activities allowed before the per-OS sandbox gate is met, and still run in the sandbox.

### 7.5 `.lullpack` archive (optional)

`.lullpack` is an optional deterministic ZIP-compatible archive of a pack folder, for **sideloading from a local file** and as `lullshift pack` output. Same validation and install review as Git sources (source "local file", digest instead of commit). Archive rules: reject `../`, absolute paths, links, nested archives, zip bombs (ratio and size bounds), duplicate canonical filenames, executable native files, dangerous content types and unknown permissions. Use SHA-256 for integrity; reproducible packing yields stable hashes for identical inputs.

**Signing is deferred past v1.** Commit pinning plus the sandbox is the v1 trust baseline.

## 8. Creator tooling

- **SDK** (`packages/activity-sdk`, Apache-2.0, TypeScript): lifecycle interface, bridge client, storage helpers, types generated from the manifest schema.
- **Kits** (`packages/activity-kits/*`): optional; see §6.5.
- **CLI** (`tools/lullshift-cli`): `create` (scaffold app or content pack from templates), `dev` (local preview with the host bridge emulated), `simulate`, `validate` (same validator as the app, incl. engine data schemas), `pack` (deterministic `.lullpack`). There is **no `publish`**: publishing is pushing to a Git repository. An optional `lullshift marketplace init` scaffolds `.lullshift/marketplace.json`. These are planned commands, not existing ones.
- **Simulator** drives an activity through agent states and host events: `working`, `needs_attention`, `completed`, `failed`, `cancelled`, duplicate and out-of-order events, forced pause, close, crash and resume-from-snapshot.
- **AI-assisted creation:** provider-independent prompts in `creator/`; Claude Code skill `.claude/skills/activity-creator`. Generated code is untrusted (no unrestricted fs, network or Tauri IPC). All tooling works offline with no account.

## 9. Context and local persistence

- SQLite migrations with tests and a backup/rollback plan; WAL and atomic writes; migrations tested from every released schema version.
- Metadata-only agent sessions by default; per-session user context is opt-in. No secrets in SQLite (§5).
- Installed activity files live in an app-owned directory; SQLite records pins and digests.
- Data export (JSON and Markdown), delete-all-local-data and configurable retention.
- Logs are bounded, rotated and redacted: no raw prompts, source, task content or keys.

## 10. Testing and operations

- **Rust (`cargo test`):** reducer, dedupe, ordering, stale timeout, IPC validation/auth, migrations and rollback/recovery, index parsing, pinning, validation.
- **TypeScript (Vitest):** UI components, stores, SDK, kits, engines, CLI.
- **End-to-end (Playwright + Tauri smoke)** on Windows, macOS, Linux: tray, notifications, freeze on `needs_attention`, install review.
- **Adapter fixtures:** exact hook payloads, unknown state, restarts, install/uninstall idempotence and backup restoration.
- **Sandbox denial tests per OS:** network (fetch/WebSocket/WebRTC/img), Tauri IPC, filesystem, navigation/popups, cross-origin storage, bridge spoofing, message floods, quota overflow, freeze without cooperation, crash recovery. Must pass before third-party code is enabled on that OS.
- **Malicious-repo fixtures:** path traversal, absolute paths, symlinks/hard links, case collisions, huge files/file counts, zip bombs (`.lullpack`), repo hooks, submodules, LFS pointers, config tricks, executables in content packs, malformed indexes, refs starting with `-`, force-pushed history.
- **Engine schema compatibility:** every bundled pack validates against its engine major; every retained major still runs its fixture packs. Simulator lifecycle tests for every bundled code activity and engine.
- **Local checks (ADR-004, no hosted CI for now):** one cross-platform entry point running Rust fmt/clippy/test, TS lint/typecheck/test/build, schema validation of the built-in index and manifests, secret/dependency scans, SBOM, provenance.
- **Release:** code signing/notarization of the desktop app and signed updater artifacts; signing keys never enter the repository. Security review before enabling third-party code activities.

## 11. ADRs

Accepted:

- [ADR-001](adr/ADR-001-open-source-v1-no-accounts.md): Fully open-source v1 with no accounts and no hosted services.
- [ADR-002](adr/ADR-002-git-repo-marketplaces.md): Git repositories as activity marketplaces.
- [ADR-003](adr/ADR-003-activity-model-kits-engines-content.md): Activity model: code activities, kits, engines and content packs; v1 content target.
- [ADR-004](adr/ADR-004-local-quality-checks-no-hosted-ci.md): Local quality checks instead of hosted CI (for now); amended with a manual-only release workflow.
- [ADR-005](adr/ADR-005-toolchains-workspace-and-checks.md): Toolchains, workspace layout and local check tooling.
- [ADR-006](adr/ADR-006-schema-first-contracts.md): Schema-first contracts and type generation.
- [ADR-007](adr/ADR-007-sqlite-library.md): SQLite library (rusqlite).
- [ADR-008](adr/ADR-008-session-state-machine.md): Agent event contract v1 and session state machine.
- [ADR-009](adr/ADR-009-local-ipc-transport-and-auth.md): Local IPC transport and authentication.
- [ADR-010](adr/ADR-010-agent-adapters-and-hook-installation.md): Agent adapters, hook forwarder and hook installation.
- [ADR-011](adr/ADR-011-ui-design-system.md): UI design system.
- [ADR-012](adr/ADR-012-activity-sandbox-and-bridge.md): Activity sandbox strategy and host bridge protocol.
- [ADR-013](adr/ADR-013-context-keeper-and-opt-in-capture.md): Context Keeper data and opt-in task-context capture.
- [ADR-016](adr/ADR-016-engines-and-content-packs.md): Engines, data schemas, packs in the app, pack media and retirement policy.
- [ADR-017](adr/ADR-017-creator-cli-and-simulator.md): Creator CLI implementation language and distribution.

To be recorded in later sprints (numbering indicative):

- ADR-014: Git integration (system `git` vs library), hardening, cache layout.
- ADR-015: Marketplace index schema v1 and install/update/pin semantics.
- ADR-018: App update signing, release pipeline and key custody.
- ADR-019: Bundling built-in activities into the installer.
- ADR-020: BYOK Companion provider abstraction and context boundary.

Use the `architecture-decision` skill and link new ADRs here.

## 12. External reference URLs

- Tauri 2: https://v2.tauri.app/ (updater: https://v2.tauri.app/plugin/updater/)
- Git configuration: https://git-scm.com/docs/git-config

## 13. Future (not in v1)

Identity, offline leases, sync, mobile/push, organizations, hosted AI, a hosted catalog, package signing, payments and creator payouts are **not in v1**. Earlier designs remain in [`docs/future/TECHNICAL_ARCHITECTURE_v1.1.md`](future/TECHNICAL_ARCHITECTURE_v1.1.md) as reference, not approved design. Any revival needs a new product-owner ADR and must be **additive**: no v1 feature may move behind an account (ADR-001). A hosted catalog would be just another marketplace source (ADR-002).

---
**Status:** Design baseline v2.0. Provider support and exact dependency versions must be validated against current official documentation during each sprint.
