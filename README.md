# Lullshift

**The human attention layer for asynchronous AI work.** While your AI agents work, Lullshift lets you disconnect (games, breaks) or stay in flow (Next Move, Companion), tells you the moment an agent needs you, and helps you return with context.

> Status: early development. Sprint 00 (foundation) provides the workspace, contracts and a desktop scaffold with a mock agent feed. See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) to build it and [CONTRIBUTING.md](CONTRIBUTING.md) to help.

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

Activities are distributed through **Git repositories**. Publish a public or private repo with a `.lullshift/marketplace.json` index; users add it with something like `lullshift marketplace add owner/repo`. Private repos use your existing Git credentials. This repository is the built-in marketplace.

## Repository guide

| Path | Contents |
|---|---|
| `apps/desktop` | Tauri 2 + React desktop app |
| `crates/` | Rust core crates |
| `packages/` | TypeScript packages (contracts, later SDK and kits) |
| `activities/` | Bundled games, engines and content packs |
| `tests/fixtures/` | Shared contract fixtures |
| `docs/PRODUCT_REQUIREMENTS.md` | What we are building and why |
| `docs/TECHNICAL_ARCHITECTURE.md` | How it is built |
| `docs/adr/` | Architecture decision records |
| `sprints/` | Roadmap, sprint backlogs, definition of done |
| `reference/` | JSON schemas: agent events, activity manifest, marketplace index |
| `creator/` | Tool-neutral prompts for AI-assisted activity creation |
| `.claude/` | Claude Code agents and skills used to build Lullshift |
| `docs/future/` | Deferred ideas (accounts, cloud, mobile, commerce). Not part of v1. |

## Name

"Lullshift" is a working name pending trademark and domain checks.

## License

[Apache-2.0](LICENSE).
