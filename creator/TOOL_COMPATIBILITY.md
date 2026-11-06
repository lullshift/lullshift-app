# AI Tool Compatibility

The prompt, the skills and the SDK guide are provider-independent. Any tool that can read files and run shell commands can follow them.

The tool must be able to run `pnpm lullshift validate <folder>` and `pnpm lullshift simulate <folder>` from the repo root. Without them it cannot claim success.

## Tools with Agent Skills support

The creator skills follow the shared Agent Skills format (`SKILL.md` with `name` and `description`). Their single source is `.agents/skills/`:

- `.agents/skills/activity-creator/SKILL.md`
- `.agents/skills/marketplace-publishing/SKILL.md`

| Tool | Discovery | Invocation |
|---|---|---|
| Claude Code | `.claude/skills/` (the copies there only point to `.agents/skills/`) | `/activity-creator` or automatic |
| Codex | `.agents/skills/` | `$activity-creator`, `/skills`, or automatic |
| Cursor | `.agents/skills/`, also `.claude/skills/` | `/activity-creator` in Agent chat, or automatic |
| OpenCode | `.agents/skills/`, also `.claude/skills/` | Loaded by the agent's `skill` tool when asked or relevant |

Discovery rules were checked against each tool's documentation on 2026-11-06 (code.claude.com/docs/en/skills, learn.chatgpt.com/docs/build-skills, cursor.com/docs/context/skills, opencode.ai/docs/skills). Re-check them when a tool changes its skills support. User guide: `docs/creators/ai-tools.md`.

## Other tools

Give the tool these files:

- `creator/ACTIVITY_CREATOR_PROMPT.md`
- `packages/activity-sdk/README.md`
- `reference/ACTIVITY_MANIFEST_V1.schema.json`

If the model cannot run commands, a person runs `validate` and `simulate` and pastes back the output.

## Rules

Keep the specifications tool-neutral: edit the skills in `.agents/skills/` only, and do not maintain a separate API description per model. Generated output is untrusted whichever tool made it: the sandbox and the CLI checks decide, not the tool.
