# ADR-005: Toolchains, workspace layout and local check tooling

- **Status:** Accepted (Sprint 00)
- **Date:** 2026-11-05
- **Owner:** Implementation lead
- **Related:** ADR-004 (local checks, no hosted CI)

## Context

Sprint 00 needs a reproducible build on Windows, macOS and Linux and one local check command (ADR-004). Tools present on the reference dev machine (Windows 11): Node 24, pnpm 12, Git 2.53, Visual Studio 2026 C++ build tools, WebView2. Rust is installed via rustup.

## Decision

1. **JavaScript/TypeScript:** a single **pnpm workspace** at the repo root (`apps/*`, `packages/*`, `tools/*`). pnpm is pinned through the root `packageManager` field (Corepack). Node major is pinned in `.node-version` and `engines`.
2. **Rust:** a single **Cargo workspace** at the repo root (`crates/*`, `apps/desktop/src-tauri`). The toolchain is pinned in `rust-toolchain.toml` (stable channel, exact version, `rustfmt` and `clippy` components). `Cargo.lock` is committed.
3. **Exact dependency versions** are resolved at scaffold time from the registries and locked by `pnpm-lock.yaml` and `Cargo.lock`; upgrades are deliberate commits.
   - pnpm's minimum release age policy stays on. No `minimumReleaseAgeExclude` entries: a release published hours ago waits (Vite was pinned to 8.3.5 at scaffold time for this reason).
   - **TypeScript is pinned to 6.0.x** because typescript-eslint 8.x supports `<6.1`. Move to TypeScript 7 once typescript-eslint supports it.
   - Rust is pinned to 1.99.1 (current stable at scaffold time).
4. **One check command: `pnpm check`**, implemented as a Node script (`scripts/check.mjs`) so it behaves the same on every OS. It runs, in order: secret scan, schema/fixture validation, generated-types freshness, TypeScript typecheck, lint, format check, unit tests, `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, and the desktop frontend build. It prints a summary with each step's exit status and the OS, and exits non-zero if any step fails. `pnpm check --fast` skips the slow steps (Rust build/test and desktop build) for the pre-commit hook.
5. **Secret scanning:** `secretlint` with the recommended preset (an npm package, so there is no extra binary to install on any OS).
6. **Git hooks are opt-in:** hooks live in `.githooks/`; `pnpm hooks:install` sets `core.hooksPath` for this clone only. Pre-commit runs `pnpm check --fast`; pre-push runs `pnpm check`.
7. **Formatting and linting:** Prettier and ESLint (typescript-eslint) for TS/JSON/Markdown-adjacent code; rustfmt and clippy for Rust.
8. **Tests:** Vitest for TypeScript; `cargo test` for Rust. Playwright/Tauri end-to-end smoke tests arrive with Sprint 01 UI work.

## Consequences

- Contributors need Node (with Corepack), Rust via rustup, and the platform's Tauri prerequisites; the bootstrap guide lists them per OS.
- A future hosted CI only has to run `pnpm install --frozen-lockfile` and `pnpm check`.
