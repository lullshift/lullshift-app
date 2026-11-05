# Universal Lullshift Activity Generator Prompt

Use this prompt with Claude Code, Codex, Cursor or another coding LLM. The official Claude Code skill is `.claude/skills/activity-creator/SKILL.md`.

> You are implementing a third-party **Lullshift static web activity**. Build the requested experience using the Lullshift Activity SDK and `reference/ACTIVITY_MANIFEST_V1.schema.json`.
>
> Activity idea: [DESCRIBE GAME/ACTIVITY].
>
> Requirements: rounds should be short and interruption-friendly; persist serializable game state; implement pause/resume; support keyboard and reduced motion; work without network; never access arbitrary local files, shells, Tauri APIs, agent tokens or raw project context. Declare only `activity.storage` if needed. Provide full source, manifest, static assets, tests, screenshot description and third-party license list. Run actual available creator CLI checks and simulation of `needs_attention`, duplicate stop events, crash/reload and offline mode. If the toolchain is missing, identify that as a blocker rather than pretending a `.lullpack` has been built. Do not publish without explicit user consent.

## Suggested templates

- Puzzle: original Sudoku-like generator or grid logic game; no protected branding/assets.
- Break: calming timed breathing or stretching prompt with instant pause/resume.
- Decision card: simple preference/choice canvas with local state only.

## Sample expected output

`manifest.json`, `src/`, `index.html`, `assets/`, `tests/`, `LICENSES.md`, `PUBLISHING.md`. Final `.lullpack` only after actual local validation.
