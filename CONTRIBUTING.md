# Contributing to Lullshift

Thanks for helping. Lullshift is fully open source under Apache-2.0.

## Before you start

- Read the [product requirements](docs/PRODUCT_REQUIREMENTS.md), the [architecture](docs/TECHNICAL_ARCHITECTURE.md) and the [ADRs](docs/adr/).
- Work is planned in [sprints](sprints/ROADMAP.md). Larger changes should match the active sprint or start with an issue.
- `docs/future/` holds deferred ideas. It is not a source of requirements; please don't build for it.

## Setup

Full details, including the Tauri system packages for each OS, are in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

| Tool | Version |
|---|---|
| Node.js | 24.x (`.node-version`) |
| pnpm | 12.6.0, pinned in `package.json`. `corepack enable` installs it |
| Rust | 1.99.1, pinned in `rust-toolchain.toml`. Install [rustup](https://rustup.rs) |
| Git | Any recent version |
| Tauri prerequisites | See [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) |

```bash
corepack enable
pnpm install
pnpm check
pnpm desktop:dev
```

No accounts, servers or credentials are needed. `.env.example` lists optional development variables; never commit a real `.env`.

## Checks and tests

There is no hosted CI (ADR-004). `pnpm check` is the quality gate. It runs the secret scan, schema fixture validation, generated-types freshness, kit build check, typecheck, lint, format check, TypeScript tests, engine compatibility checks, validate and simulate for bundled activities, `cargo fmt`, `cargo clippy`, `cargo test` and the desktop build. It ends with a pass/fail summary that includes your OS.

| Command | What it runs |
|---|---|
| `pnpm check` | Everything above |
| `pnpm check:fast` | Everything except the Rust and desktop build steps |
| `pnpm test` | TypeScript tests in every package |
| `cargo test --workspace` | Rust tests |
| `pnpm lint`, `pnpm typecheck`, `pnpm format` | One step each |
| `pnpm gen:types` | Regenerates types after a schema change |

`simulate` and the bundled-activity check need Microsoft Edge, Google Chrome or Chromium. If a step can't run on your machine, say which and why in your pull request.

Optional Git hooks: `pnpm hooks:install` runs `check:fast` before each commit and the full `check` before each push, for this clone only.

## Making changes

- One logical change per pull request, with tests.
- Run `pnpm check` and paste its summary into the pull request description.
- Schema changes in `reference/` need updated fixtures in `tests/fixtures/` and regenerated types (`pnpm gen:types`).
- Persistent schema changes need migration and recovery tests.
- Keep docs in step with behaviour. User-facing docs are in `docs/user/`, creator docs in `docs/creators/`.

### Commit messages

Follow the style in `git log`:

- A short summary in the imperative, sentence case, no full stop: `Add Codex and Cursor adapters with a capability matrix`.
- Add the sprint task id in brackets when there is one: `(S10-T01)`.
- A blank line, then a few lines on what changed and why.

### Architecture decisions

Decisions that change architecture, a security boundary, a contract or a dependency policy go in a new ADR in `docs/adr/`, numbered after the last one. Don't change product requirements silently; propose an ADR.

## Activities and content

- Read the [creator guide](docs/creators/README.md) first.
- Every activity must pass `pnpm lullshift validate` and `pnpm lullshift simulate`, and meet the activity gates in [sprints/DEFINITION_OF_DONE.md](sprints/DEFINITION_OF_DONE.md).
- Add new bundled activities to `.lullshift/marketplace.json`.
- **Original content only.** Games and activities use original names, text and art. Generic mechanics are welcome; copies of specific branded games (names, characters, art, level designs, look) are not. Bundled items are Apache-2.0 with compatible asset licences, and every third-party asset must be listed with its licence.
- Follow the [content style guide](docs/CONTENT_STYLE_GUIDE.md): no medical claims, no pressure or guilt.

## Ground rules

- No accounts, hosted services or default telemetry (ADR-001).
- Network use only on a user's action: marketplace Git fetches, update checks and BYOK Companion calls.
- Never commit secrets, signing keys or personal data. Never log raw task data or keys.
- Treat agent hooks, marketplace repositories and activity code as sensitive or untrusted input. Never weaken the sandbox or the Git hardening to make content work.
- Hook install and uninstall need explicit consent, backups, idempotence and a clean uninstall.

## Security

Please don't report vulnerabilities in public issues. See [SECURITY.md](SECURITY.md).

## License

By contributing, you agree your contribution is licensed under the [Apache License 2.0](LICENSE).
