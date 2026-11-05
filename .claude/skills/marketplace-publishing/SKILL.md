---
name: marketplace-publishing
description: Set up or update a Git-repository Lullshift marketplace (public or private), including the built-in marketplace in this repo, and validate activities before they are listed.
---

# Marketplace Publishing (Git repositories)

1. Read ADR-002, `reference/MARKETPLACE_V1.schema.json` and the distribution section of `docs/TECHNICAL_ARCHITECTURE.md`.
2. Ensure `.lullshift/marketplace.json` validates; each entry `id` matches its `manifest.json`; `source.path` folders exist; external `git` sources use https/ssh and a tag or commit where possible.
3. Every listed activity passes CLI `validate` and `simulate`, the activity gates in `sprints/DEFINITION_OF_DONE.md`, and has a license and asset inventory.
4. Repo hygiene: no submodules, LFS, escaping symlinks, Git hooks or oversized files in activity folders; nothing secret committed.
5. **Built-in marketplace (this repo):** entries point to `activities/...`; changes land by reviewed PR and ship with the app release tag.
6. **Third-party marketplaces:** public or private repos; users add them by `owner/repo` or Git URL and reach private ones with their own Git credentials. Lullshift never handles tokens.
7. Never push, tag, change repo visibility or open PRs without explicit user confirmation. Report validation commands and results.
