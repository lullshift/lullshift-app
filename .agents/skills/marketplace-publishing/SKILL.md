---
name: marketplace-publishing
description: Set up or update a Git-repository Lullshift marketplace (public or private), including the built-in marketplace in this repo, and validate activities before they are listed.
license: Apache-2.0
compatibility: Needs a clone of github.com/lullshift/lullshift-app with pnpm installed, and Git. Works with Claude Code, Codex, Cursor and OpenCode.
---

# Marketplace Publishing (Git repositories)

This skill is shared by every supported AI tool. Claude Code finds it through `.claude/skills/marketplace-publishing`; Codex, Cursor and OpenCode read it from `.agents/skills/marketplace-publishing`. Run the CLI as described in the `activity-creator` skill (from the `lullshift-app` clone, or with `pnpm --dir <clone>`).

1. Read ADR-002 and ADR-015 in `docs/adr/`, `reference/MARKETPLACE_V1.schema.json` and `docs/creators/publishing-a-marketplace.md`.
2. `.lullshift/marketplace.json` validates against the schema. Each entry's `id` matches its `manifest.json`; `source.path` folders exist; external `git` sources use https and a tag or commit where possible.
3. Every listed activity passes `pnpm lullshift validate` and `pnpm lullshift simulate` with no errors or warnings, and has a licence and asset inventory. Report the real command output.
4. Repository hygiene: no submodules, LFS, symlinks that leave the folder, Git hooks or oversized files in activity folders; nothing secret committed.
5. **Third-party marketplaces:** public or private repositories. Users add them in the app by `owner/repo` or Git URL and reach private ones with their own Git credentials. Lullshift never handles tokens. Content packs install normally. A code activity from a third-party marketplace is shown as a "Third-party program" and installs only after the user reads a risk warning and confirms; every update that changes the code asks again (ADR-015). Tell the user this when they list a code activity, and keep its permissions to what it really uses.
6. **Built-in marketplace (the `lullshift-app` repository itself):** entries point to `activities/...`; changes land by reviewed pull request, meet the activity gates in `sprints/DEFINITION_OF_DONE.md`, and ship with the app release.
7. Never push, tag, change repository visibility or open pull requests without the user's explicit confirmation.
