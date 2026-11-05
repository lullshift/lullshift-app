# Contributing to Lullshift

Thanks for helping. Lullshift is fully open source under Apache-2.0.

## Before you start

- Read the [product requirements](docs/PRODUCT_REQUIREMENTS.md), the [architecture](docs/TECHNICAL_ARCHITECTURE.md) and the [ADRs](docs/adr/).
- Work is planned in [sprints](sprints/ROADMAP.md). Larger changes should match the active sprint or start with an issue.
- Set up your machine with [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Making changes

- One logical change per pull request, with tests.
- Run `pnpm check` and paste its summary (it includes your OS) into the pull request description.
- Architecture decisions go in a new ADR under `docs/adr/`.
- Schema changes need updated fixtures and regenerated types (`pnpm gen:types`).

## Ground rules

- No accounts, hosted services or default telemetry (ADR-001).
- Never commit secrets, keys or personal data.
- Treat agent hooks, marketplace repositories and activity code as sensitive or untrusted input.
- Games and activities use original names and art; generic mechanics are welcome, copies of branded games are not.

## License

By contributing, you agree your contribution is licensed under the [Apache License 2.0](LICENSE).
