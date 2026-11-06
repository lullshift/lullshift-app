# Using an AI tool

This repository ships two creator skills that work in Claude Code, Codex, Cursor and OpenCode. They follow the shared [Agent Skills](https://agentskills.io/specification) format: a folder with a `SKILL.md` file.

| Skill | What it does |
|---|---|
| `activity-creator` | Turns a description into a game, a code activity on a kit, or a content pack. It picks the lightest kind that fits, scaffolds with `lullshift create`, meets the help, layout and paused-view rules, then runs `validate` and `simulate` and reports the real output. |
| `marketplace-publishing` | Sets up or updates a Git marketplace: `.lullshift/marketplace.json`, ids and paths, the checks every listed activity must pass, and repository hygiene. |

## Where each tool finds them

The instructions live once, in `.agents/skills/`. Claude Code reads `.claude/skills/`, so the copies there only point to the shared files.

| Tool | Reads skills from | How to use them |
|---|---|---|
| Claude Code | `.claude/skills/` | Type `/activity-creator`, or describe the activity and Claude picks the skill |
| Codex | `.agents/skills/` | Type `$activity-creator` or choose it from `/skills`, or let Codex pick it |
| Cursor | `.agents/skills/` (also `.claude/skills/`) | Type `/activity-creator` in Agent chat, or let Cursor pick it |
| OpenCode | `.agents/skills/` (also `.claude/skills/`) | Ask for it by name ("use the activity-creator skill") or let OpenCode load it |

Open the tool at the root of your `lullshift-app` clone, so it finds both the skills and the CLI.

## Working in your own repository

The `lullshift` CLI runs from a clone of this repository. To build activities in your own marketplace repository, copy the two folders from `.agents/skills/` into that repository's `.agents/skills/` (and into `.claude/skills/` for Claude Code), or into your user-level skills folder (`~/.agents/skills/`, or `~/.claude/skills/` for Claude Code). The skill then asks for the path to your clone and runs `pnpm --dir <clone> lullshift …`.

## Other tools

Give the tool [`creator/ACTIVITY_CREATOR_PROMPT.md`](../../creator/ACTIVITY_CREATOR_PROMPT.md), [`packages/activity-sdk/README.md`](../../packages/activity-sdk/README.md) and [`reference/ACTIVITY_MANIFEST_V1.schema.json`](../../reference/ACTIVITY_MANIFEST_V1.schema.json). If it can't run commands, run `validate` and `simulate` yourself and paste the output back.

## Whatever tool you use

Generated code is untrusted. The sandbox and the CLI checks decide whether an activity works, not the tool that wrote it. Don't treat an activity as done until `validate` and `simulate` both pass, and review what the tool wrote before you publish it.
