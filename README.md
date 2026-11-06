# Lullshift

**The human attention layer for asynchronous AI work.** While your AI agents work, Lullshift lets you disconnect (games, breaks) or stay in flow (Next Move, Companion), tells you the moment an agent needs you, and helps you return with context.

> Status: pre-release. The desktop app follows Claude Code, Codex and Cursor sessions through their hooks, runs bundled games and breaks in a sandbox, installs content packs from Git marketplaces and offers an optional Companion that uses your own AI provider key. See the [user guide](docs/user/README.md) to use it, the [creator guide](docs/creators/README.md) to build activities, [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) to build it and [CONTRIBUTING.md](CONTRIBUTING.md) to help.

## Principles

- **Fully open source** (Apache-2.0). No accounts, no sign-in, no Lullshift servers, no telemetry by default.
- **Local-first and offline.** Your agent sessions, notes and activity progress stay on your machine.
- **Observes, never drives.** Lullshift watches agent lifecycle hooks (Claude Code first, then Codex and Cursor); it never acts on your agents.
- **Interruptible by design.** Every game and activity pauses and saves instantly when an agent needs you.

## Games and activities

v1 targets **20 original games and ~100 activities** (breathing, movement, quizzes, brain teasers, creative toys, …), bundled in this repo under `activities/`.

Anyone can build more:

- **Code activities**: any static web app using the Lullshift SDK, optionally built on our grid, card or arcade kits.
- **Content packs**: just a JSON file for one of our engines (quiz, breathing, routine, …), no code.

Activities are distributed through **Git repositories**. Publish a public or private repo with a `.lullshift/marketplace.json` index; users add it in the app by `owner/repo` or Git URL, and each install is pinned to a commit. Private repos use the user's own Git credentials; Lullshift stores none. Git doesn't have to be installed for public https marketplaces: without it, Lullshift uses a built-in Git (gitoxide) for those; private and ssh marketplaces need Git installed ([ADR-023](docs/adr/ADR-023-bundled-git-fallback.md)). This repository is the built-in marketplace. In this release, third-party marketplaces can supply content packs; third-party code activities can't be installed yet.

Start with the [creator guide](docs/creators/README.md).

## Documentation

| Guide | For |
|---|---|
| [User guide](docs/user/README.md) | Installing, connecting agents, activities, marketplaces, Companion, privacy, troubleshooting, uninstalling |
| [Creator guide](docs/creators/README.md) | SDK and bridge, kits, engines and content packs, the CLI, sandbox rules, publishing a marketplace |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Setup, checks, commit style, ADRs, the original-content rule |
| [SECURITY.md](SECURITY.md) | Reporting vulnerabilities privately |

## Repository guide

| Path | Contents |
|---|---|
| `apps/desktop` | Tauri 2 + React desktop app |
| `crates/` | Rust core crates |
| `packages/` | TypeScript packages: agent protocol, activity SDK and kits |
| `tools/lullshift-cli` | The `lullshift` creator CLI |
| `activities/` | Bundled games, engines and content packs |
| `tests/fixtures/` | Shared contract fixtures |
| `docs/PRODUCT_REQUIREMENTS.md` | What we are building and why |
| `docs/TECHNICAL_ARCHITECTURE.md` | How it is built |
| `docs/adr/` | Architecture decision records |
| `docs/user/`, `docs/creators/` | User and creator guides |
| `sprints/` | Roadmap, sprint backlogs, definition of done |
| `reference/` | JSON schemas: agent events, activity manifest, marketplace index |
| `creator/` | Tool-neutral prompts for AI-assisted activity creation |
| `.claude/` | Claude Code agents and skills used to build Lullshift |
| `docs/future/` | Deferred ideas (accounts, cloud, mobile, commerce). Not part of v1. |

## Name

"Lullshift" is a working name pending trademark and domain checks.

## License

[Apache-2.0](LICENSE).
