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

This opens the Lullshift window showing a **mock agent feed** (Sprint 00). Real agent sessions arrive in Sprints 01–02.

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

## Environment variables

None are required. `.env.example` lists optional development variables; never commit a real `.env`.
