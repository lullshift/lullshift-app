# AI Tool Compatibility

- **Claude Code:** invoke `.claude/skills/activity-creator/SKILL.md` in this repository; reference the canonical prompt and manifest.
- **Codex / Agent Skills:** place or link the same skill body inside the tool's supported project skill directory; verify the installed runtime's current discovery rules.
- **Cursor:** create a project rule pointing at `creator/ACTIVITY_CREATOR_PROMPT.md` and `reference/ACTIVITY_MANIFEST_V1.schema.json`; confirm actual current rule file layout from official docs.
- **Other LLMs:** paste the universal prompt and supply schema and SDK documents.

The canonical specifications remain tool-neutral. Do not maintain drifting API specifications per model.
