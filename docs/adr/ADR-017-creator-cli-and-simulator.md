# ADR-017: Creator CLI, simulator and Activity SDK packaging

- **Status:** Accepted (Sprint 05)
- **Date:** 2026-11-05
- **Owner:** Implementation lead
- **Related:** ADR-003 (activity model), ADR-006 (schema-first contracts), ADR-012 (sandbox and bridge), ADR-016 (engines), PRD 4.6, TECHNICAL_ARCHITECTURE §7.5 and §8

## Context

Sprint 05 gives creators (people and AI tools) a local, offline way to build, check and package activities: an SDK, and a `lullshift` CLI with `create`, `dev`, `simulate`, `validate`, `pack` and `marketplace init`. Three choices needed a decision:

- what the CLI is written in and how creators get it;
- how `simulate` runs activity code, which is untrusted;
- how the SDK is shipped to activities that may use no build step.

## Decision

### Language and distribution

- **TypeScript on Node 24,** the version the workspace already requires. The CLI is the workspace package `@lullshift/cli` in `tools/lullshift-cli`, run from source with Node's built-in type stripping. Only erasable TypeScript syntax is allowed, and there is no build step.
- **Shared code, not copies.** The CLI uses the same Ajv setup and schemas as the app's checks (`createAjv` in `@lullshift/agent-protocol`), and the host-side bridge protocol from `@lullshift/activity-sdk`.
- **The only runtime dependencies are Ajv and ajv-formats,** both already used. HTTP serving, ZIP writing, SHA-256 and the browser protocol use Node's standard library.
- **How to run it:** in this repository, `pnpm lullshift <command>`. Publishing `@lullshift/cli` and `@lullshift/activity-sdk` to npm needs the product owner's approval, like any release. Until then, creators clone the repository.
- **No network, ever.** No command opens an outgoing connection. `dev` and `simulate` listen on `127.0.0.1` only, on ports chosen by the OS.

### Validation and packing

- **`validate` errors** (exit code 1):
  - the manifest fails the schema;
  - a declared file is missing (entry point, data, preview);
  - a file type is outside the host's allowlist, or a file is a link;
  - a path is unsafe;
  - the folder breaks the size limits: 2,000 files, 5 MiB per file, 25 MiB in all, 256 KiB for a preview;
  - a content pack contains code (any `.html`, `.js`, `.mjs` or `.css`), or its data fails the engine's data schema.
- **`validate` warnings** (exit code 0):
  - no `help`;
  - no `preview`;
  - the bridge script differs from the SDK's copy;
  - source text that names network or privileged APIs (`fetch`, `XMLHttpRequest`, `WebSocket`, `RTCPeerConnection`, `__TAURI`), which the sandbox blocks anyway.
- **`pack` writes a deterministic `.lullpack`.** It is a ZIP of stored (uncompressed) entries in sorted path order. Every entry has the timestamp 1980-01-01 00:00, fixed attributes and no extra fields. The same folder therefore always gives the same bytes, and `pack` prints their SHA-256. It refuses to pack a folder that fails `validate`.

### Simulator and dev server: a real browser sandbox

- **Activity code never runs in Node.** A DOM emulation such as jsdom is not a security boundary, and the activities being checked include AI-generated and third-party code.
- **`simulate` and `dev` use a Chromium-family browser already on the machine**: Microsoft Edge, Google Chrome or Chromium. They find it in the standard install locations, or take the path from `--browser` or `LULLSHIFT_BROWSER`. Nothing is downloaded. Without a browser, `simulate` stops with exit code 2 and says how to point it at one.
- **The same sandbox as the app.** The harness page runs a host emulator built on the SDK's host protocol, at `http://127.0.0.1:<port A>`. The activity is served from a second origin, `http://127.0.0.1:<port B>`, with the app's activity headers:
  - the CSP from ADR-012, including `sandbox allow-scripts` and `connect-src 'none'`, with `frame-ancestors` set to the harness;
  - a sandboxed iframe with the same `allow` denials.
  An activity that works in the simulator therefore works under the same rules in the app, and one that reaches for the network fails the same way.
- **`simulate` runs headless** with a temporary profile, deleted afterwards. It drives scenarios over the Chrome DevTools Protocol and records:
  - uncaught exceptions inside the activity;
  - blocked network attempts;
  - every bridge message.
- **Scenarios** (PRD 4.6):
  - fresh start;
  - save and restore round trip, which must be stable: restoring a saved state and saving again gives the same state;
  - `needs_attention` straight away, before the activity is ready;
  - duplicate and out-of-order agent events, which pause once;
  - `working`, `completed`, `failed` and `cancelled`, which must not pause;
  - a forced pause with no answer from the activity;
  - close and reopen;
  - a crash (unload with no save), then resume from the last save;
  - offline and network denial;
  - reduced motion and the dark theme at start;
  - a focusable element for keyboard play;
  - the `summary` message, if sent.
- **`dev`** opens the same harness in a visible window, with controls for each host event, so creators can play and interrupt by hand.

### The SDK

- **`@lullshift/activity-sdk`** (`packages/activity-sdk`, Apache-2.0) holds:
  - the bridge client as one classic script, `lullshift-bridge-v1.js`, with type declarations;
  - the host-side protocol (parser, limits, rate limiter), used by the app and the simulator;
  - types for the manifest, help and summary;
  - `SDK_VERSION`.
- **Activities load the bridge as a plain script.** Activities can't use module scripts (ADR-012), so `create` copies the bridge script into each new activity, and bundled activities carry identical copies, checked by a test. The canonical copy moves from `activities/lib/` to the SDK.
- **The bridge always answers a save request.** If `saveState` returns nothing or throws, the bridge replies with the last state it knew: the last one reported, or the state it was restored from. An activity that has nothing new to say is therefore never closed as unresponsive (closes an ADR-012 gap for SDK users).

### Templates

- `create` offers two templates:
  - **`app`:** a custom code activity using the bridge, keyboard play, reduced motion, help and preview;
  - **`content`:** a content pack for a named engine.
- **The kit-based template arrives with the kits** in Sprint 06. Until then, `create --kind app` is the starting point for kit games as well.

## Consequences

- Creators need Node 24 and a Chromium-family browser to simulate. Edge ships with Windows, and the CLI says plainly when no browser is found. Firefox and Safari aren't supported for `simulate`. Their engines differ from WebView2, but WebKit testing is covered by the app's manual tests on macOS and Linux.
- Because the simulator serves activities with the app's headers, a change to the activity CSP must be made in both `activities.rs` and the CLI. A test compares the two.
- The CLI is not yet installable without a clone; publishing it is a separate, approved step.

## Amendment (2026-11-06, owner decision 29): npm packages

The owner decided to publish the CLI and the SDK to npm. This replaces "The CLI is not yet installable without a clone" above.

### What is published

- **`@lullshift/agent-protocol`** (types, validators and the schemas), **`@lullshift/activity-sdk`** (bridge script, host protocol, types) and **`@lullshift/cli`** (the `lullshift` command). All Apache-2.0, public, under the `@lullshift` scope.
- **The CLI is not bundled with the desktop installer.** Creators get it from npm (`npx @lullshift/cli ...`) or from a clone.
- **Nothing else.** `@lullshift/activity-kits` and the desktop app stay private workspace packages. The CLI carries copies of the kit scripts and the bundled engines instead (below).

### Build

- **Plain `tsc` emit, no bundler and no new dependency.** `scripts/npm-build.mjs` runs `tsc -p tsconfig.build.json` in each package, writing JavaScript (and `.d.ts` for the two libraries) to `dist/`. `rewriteRelativeImportExtensions` turns the sources' `./x.ts` imports into `./x.js`. No source maps.
- **Inside the repository nothing changes.** The workspace manifests still point at `src/*.ts`; `pnpm lullshift`, the tests and the desktop app run the TypeScript sources on Node 24. pnpm's `publishConfig` swaps `exports`, `types`, `bin` and `engines` for the `dist/` ones when packing, and replaces `workspace:*` with the exact version.
- **The schemas travel as a module.** `pnpm gen:types` now also writes `src/generated/schemas.ts` from `reference/*.schema.json`, so the published protocol package doesn't need `/reference` or JSON import attributes. `gen:types:check` and a test keep it identical to the canonical files.
- **The installed CLI reads only its own package.** The build copies the harness page (with `host.ts` already stripped to JavaScript), the kit scripts from `packages/activity-kits/dist` and the bundled engines from `activities/engines` into `dist/`. The bridge and the host protocol come from the installed SDK. `src/paths.ts` picks the layout by where it runs (`src/` or `dist/`). So `create`, `validate` and `simulate` work offline for apps and content packs, against the engines of the release the CLI was built from.
- **Tarball allowlists.** Each package lists its `files`. `scripts/check-npm-packages.mjs` packs all three exactly as a publish would and fails on any file outside the allowlist, tests or fixtures, source maps, `.env`, local paths, `.ts` imports, `workspace:` specifiers, a missing entry point or a wrong `engines`. It runs in `pnpm check` (full) and before every publish.

### Node 22 and newer

- Published packages declare `"engines": { "node": ">=22.12.0" }`. The compiled JavaScript doesn't need Node 24's type stripping. The CLI needs `zlib.crc32` (Node 22.2) and the global `WebSocket` (stable in 22.4); 22.12 is a recent 22 LTS release.
- `serve.ts` no longer imports `stripTypeScriptTypes` by name, because that export is missing on older 22.x releases. Only the repository layout calls it.
- Development in this repository stays on Node 24.
- **Not yet tested on Node 22.** Only Node 24 was available when this was built. The owner's manual checks include a Node 22 run.

### Gating and provenance

- **Only one way to publish:** `.github/workflows/publish-npm.yml`. It is `workflow_dispatch` only and runs only on `main`. It uses the protected `npm` environment, whose required reviewer is the owner, so every run waits for the owner's approval. It installs with a frozen lockfile, checks the generated schemas, typechecks and tests the three packages, packs and checks the tarballs, installs them outside the repository and runs `create`, `validate` and `simulate`. Then it publishes in dependency order (protocol, SDK, CLI) with `pnpm publish --provenance --access public`, skipping versions already on npm. A dry run is the default.
- **Least privilege.** The workflow has `contents: read` and `id-token: write` (for provenance). Every action is pinned to a full commit SHA. The npm token is a secret of the `npm` environment, passed only to the publish step as `NODE_AUTH_TOKEN`. `release_config.rs` guards this.
- **A seatbelt for local publishes.** Each package's `prepublishOnly` runs `scripts/npm-publish-guard.mjs`, which refuses unless it runs inside that workflow on `main`. `npm publish --dry-run` is let through. The guard can be skipped with `--ignore-scripts`, so the real lock is the credential: the only npm token lives in the `npm` environment.
- **Trusted publishing (preferred once set up).** npm's trusted publishing (OIDC) removes the long-lived token. npm's docs (docs.npmjs.com/trusted-publishers, read 2026-11-06) say it needs npm CLI 11.5.1 or later and Node 22 or later, runs only on GitHub-hosted runners, needs `id-token: write`, matches the workflow file name exactly, and adds provenance on its own. pnpm's docs (pnpm.io/cli/publish, read 2026-11-06) describe publishing under trusted publishing and say pnpm then adds provenance on its own. The workflow already has `id-token: write` and works without `NPM_TOKEN`. Neither path has been run yet, so the first publish uses whichever the owner sets up, and RELEASING.md says how to switch.
- Provenance needs a **public** repository. npm doesn't generate it for private ones.

### Consequences

- A change to the kits or the bundled engines reaches npm users only with the next CLI release.
- The `lullshift --version` output is the SDK version (0.2.0), while the CLI package is 0.1.0. Align them before the first publish if that would confuse creators.
- Releasing is now two manual steps: the installers (ADR-004) and, separately, the npm packages. Either can ship without the other.
