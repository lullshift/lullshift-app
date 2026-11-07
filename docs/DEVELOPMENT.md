# Developer bootstrap

Lullshift builds on Windows, macOS and Linux. There are no accounts, servers or credentials to set up (ADR-001). There is no hosted CI; `pnpm check` is the quality gate (ADR-004).

## 1. Prerequisites

| Tool | Version | Notes |
|---|---|---|
| Node.js | 24.x (`.node-version`) | Includes Corepack |
| pnpm | 12.6.0 (pinned in `package.json`) | `corepack enable` installs the pinned version |
| Rust | 1.99.1 (pinned in `rust-toolchain.toml`) | Install [rustup](https://rustup.rs); it fetches the pinned toolchain plus `rustfmt` and `clippy` automatically |
| Git | any recent | |
| Tauri system prerequisites | per OS | See [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) |

Tauri prerequisites in short:

- **Windows:** Microsoft C++ Build Tools ("Desktop development with C++") and WebView2 (preinstalled on Windows 11).
- **macOS:** Xcode Command Line Tools (`xcode-select --install`).
- **Linux (Debian/Ubuntu):** `libwebkit2gtk-4.1-dev`, `build-essential`, `curl`, `wget`, `file`, `libxdo-dev`, `libssl-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`.

## 2. First build

```bash
corepack enable
```

```bash
pnpm install
```

```bash
pnpm check
```

`pnpm check` runs the secret scan, schema fixture validation, generated-types freshness, typecheck, lint, format, TypeScript tests, `cargo fmt`, `cargo clippy`, `cargo test` and the desktop frontend build. It ends with a pass/fail summary that includes your OS. Paste that summary into sprint closeouts.

## 3. Run the desktop app

```bash
pnpm desktop:dev
```

This opens Lullshift. Closing the window keeps it running in the tray; quit from the tray menu.

Local data lives in the app data directory (`%APPDATA%\app.lullshift.desktop` on Windows,
`~/Library/Application Support/app.lullshift.desktop` on macOS, `~/.local/share/app.lullshift.desktop`
on Linux): `lullshift.db` (SQLite, WAL), upgrade backups `lullshift.db.v<N>-<time>.bak`, and
`ipc/` with `endpoint.json` and the per-install `token` (ADR-009). Delete `ipc/token` and restart
to rotate the token.

### Send a test agent event

Real Claude Code sessions report through hooks once you choose **Connect Claude Code** in the app
(ADR-010, `docs/adapters/CLAUDE_CODE.md`). To try the hooks without changing your own Claude Code
settings, put the `hooks` block in a scratch file and run `claude --settings <file>`.

To send events by hand, use the dev client:

```bash
cargo run -p agent-ipc --example send_event -- "<app data dir>" tests/fixtures/agent-event/valid/working-minimal.json
```

It prints the reply (`{"ok":true}`, `{"ok":true,"duplicate":true}` or `{"ok":false,"error":"<code>"}`).
Use a fresh `eventId` for each new event; reusing one is reported as a duplicate.

### Demo mode and screenshots

The desktop dev server also serves a browser demo of the whole UI at `/demo.html`. It runs on a pretend backend (`apps/desktop/src/demo/`) with made-up sessions and Companion history, and it plays the bundled activities from `activities/`. No Rust core and no real data are involved. Use it for UI review and for the website's screenshots.

```bash
pnpm --filter @lullshift/desktop exec vite --port 1420
```

Then open <http://localhost:1420/demo.html>. Add `?welcome` to start with the first-launch welcome. In the browser console:

- `lullshiftDemo.attention("claude-code")` makes an agent need you. A running activity pauses.
- `lullshiftDemo.changed()` re-reads the sessions.

The demo exists only on the dev server: `demo.html` isn't a build input, and the `/demo-activities/` route is dev-only.

To capture screenshots, keep the dev server running and run this from the repo root:

```bash
node apps/desktop/scripts/demo-screenshots.mjs demo-screenshots
```

It drives Edge or Chrome headless (set `LULLSHIFT_BROWSER` to use another path). It writes `<shot>-light.png` and `<shot>-dark.png` at 1280x800 CSS pixels and 2x density for these shots: sessions, welcome back, Home, Play (the `activities` shot), Rest, a game, a paused game, Companion and integrations (Settings, scrolled to Integrations). Add or change shots in the `SHOTS` list in the script.

The website (`lullshift/lullshift.github.io`) uses five of them: sessions, game, game-paused, welcome-back and companion. They are WebP, 1600 px wide, in its `public/screens/`. Convert new captures to that size and format when you update them there.

## 4. Optional Git hooks

```bash
pnpm hooks:install
```

This enables `.githooks/` for this clone only:

- **pre-commit:** secret-scans the staged content (`scripts/scan-staged-secrets.mjs`), then runs `pnpm check:fast` (no Rust/desktop build).
- **pre-push:** runs the full `pnpm check`.

Undo with `git config --unset core.hooksPath`.

**Residual risk:** with no hosted CI (ADR-004), hooks are the only automatic safeguard, and they are opt-in and skippable with `--no-verify`. Before pushing, run `pnpm check` yourself if hooks are off.

## 5. Working with contracts

- Canonical JSON Schemas live in `reference/` (ADR-006).
- After changing a schema, run `pnpm gen:types` and update `tests/fixtures/<schema>/{valid,invalid}`.
- Each invalid fixture breaks exactly one rule, named in its file name.
- TypeScript (Ajv) and Rust (`crates/agent-core`) both test against the same fixtures.

## 6. Dependency policy

- Lockfiles (`pnpm-lock.yaml`, `Cargo.lock`) are committed; upgrades are deliberate commits.
- pnpm's minimum release age policy is left on; don't add `minimumReleaseAgeExclude` entries to install brand-new releases.
- TypeScript is pinned to 6.0.x until typescript-eslint supports TypeScript 7.

## Creating activities

The SDK guide is `packages/activity-sdk/README.md`. It covers the bridge, sandbox rules, the required `help` section, paused-view stats and previews. The creator CLI runs from the repo root:

```bash
pnpm lullshift create my-activity --name "My Activity"
```

```bash
pnpm lullshift validate my-activity
```

```bash
pnpm lullshift simulate my-activity
```

`simulate` needs Edge, Chrome or Chromium on your computer. Run `pnpm lullshift --help` for `dev`, `pack` and the rest.

## Environment variables

None are required. `.env.example` lists optional development variables; never commit a real `.env`.
