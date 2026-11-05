# AI Tool Compatibility

The prompt and the SDK guide are provider-independent. Any tool that can read files and run shell commands can follow them.

Give every tool these files:

- `creator/ACTIVITY_CREATOR_PROMPT.md`
- `packages/activity-sdk/README.md`
- `reference/ACTIVITY_MANIFEST_V1.schema.json`

The tool must be able to run `pnpm lullshift validate <folder>` and `pnpm lullshift simulate <folder>` from the repo root. Without them it cannot claim success.

- **Claude Code:** invoke `.claude/skills/activity-creator/SKILL.md` in this repository.
- **Codex / Agent Skills:** place or link the same skill body inside the tool's supported project skill directory; verify the installed runtime's current discovery rules.
- **Cursor:** create a project rule pointing at the three files above; confirm the current rule file layout from official docs.
- **Other LLMs:** paste the universal prompt and supply the SDK guide and schema. If the model cannot run commands, a person runs `validate` and `simulate` and pastes back the output.

Keep the specifications tool-neutral. Do not maintain a separate API description per model. Generated output is untrusted whichever tool made it: the sandbox and the CLI checks decide, not the tool.
