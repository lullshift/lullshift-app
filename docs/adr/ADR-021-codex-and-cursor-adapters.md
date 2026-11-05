# ADR-021: Codex and Cursor adapters

- **Status:** Accepted (Sprint 10, S10-T01)
- **Date:** 2026-11-06
- **Related:** ADR-010 (hook forwarder and installation), ADR-008 (event contract), `docs/ADAPTER_CAPABILITIES.md`

## Context

Only agents with documented lifecycle hooks get an adapter. Codex CLI and Cursor both document user-level hooks. Their gaps differ from Claude Code's (no idle event in Codex, no needs-input event in Cursor), and the UI must show those gaps as unknown, never guess.

## Decision

- **Same forwarder.** `lullshift-desktop hook <adapter> --data-dir <dir>` with `<adapter>` one of `claude-code`, `codex`, `cursor`. Same 1 MiB / 2 s limits, zero-effect contract and redaction. Codex and Cursor never use task-text capture; only `session_id` (Cursor: `conversation_id`), the event name and lowercase enum hints are read. `user_email`, prompts, assistant messages and transcripts are not declared in the payload structs, so they are never held.
- **Shared installer** (`agent_adapters::hookfile`): backup first, atomic write that aborts on concurrent change, symlink/read-only refusal, idempotent merge, exact restore or removal of only Lullshift's entries, refusal (file untouched) when the file doesn't parse or has an unknown shape. Install records reuse the SQLite `adapter_installations` row per adapter id; no schema change.
- **Codex:** `$CODEX_HOME/hooks.json`, else `~/.codex/hooks.json`; never project files, never `notify`. Registers SessionStart, UserPromptSubmit, PostToolUse, PermissionRequest, Stop, Interrupt (all `async: true`, 2 s timeout) and SessionEnd (synchronous, 3 s timeout). `config.toml` is read, never written, for `[features] hooks = false` only, which makes status `hooks_disabled` when Lullshift's hooks are installed or partly installed (nothing installed stays `not_installed`). A relative `CODEX_HOME` is ignored. Hooks/settings files must be regular files of at most 1 MiB, with no duplicate JSON keys and no integers beyond 64 bits, or they are refused untouched (`settings_invalid`). PermissionRequest maps to `needs_attention`, Stop to `turn_ended`, SessionEnd to `session_ended`. **Interrupt maps to `turn_ended`** with `reason=interrupted`: `cancelled` is terminal in the reducer and the Codex session continues. The app tells the user that Codex requires reviewing and trusting new or changed hooks in `/hooks`, also after a repair; Lullshift can't observe that approval.
- **Cursor:** `~/.cursor/hooks.json` with `"version": 1` (another version is refused). Only sessionStart, beforeSubmitPrompt, postToolUse, afterAgentResponse, stop, sessionEnd, each with a 3 s timeout; never permission or `pre*` hooks, never `failClosed`. Nothing is written to stdout, so beforeSubmitPrompt lets the prompt through. Cursor never maps to `needs_attention` and doesn't declare the `attention` capability. `stop` with status `error` is still `turn_ended` (status kept as a hint).
- **Command line quoting.** Cursor (Windows: PowerShell) and Codex take a command string. It is built only from the app's own executable path and data directory, each in single quotes (literal in both sh and PowerShell), with a leading `& ` on Windows. A path is accepted only if it is absolute and every character is a letter, digit, space or one of `_-./\:()@+=,~`; anything else (quotes, `$`, backtick, `;`, `&`, `|`, `%`, `!`, control, bidi or curly-quote characters) makes install fail with `unsupported_path` and writes nothing.
- **Cursor payloads in the Claude Code path.** Cursor also runs hooks from `~/.claude/settings.json`. A payload carrying `cursor_version` is dropped by the Claude Code mapper (event, capture context and transcript reference), so it is never recorded as a `claude-code` session. It is not re-mapped to `cursor`: the shape Cursor sends to Claude-style hooks isn't documented, and the Cursor adapter's own hooks provide the real data.

## Consequences

- Codex hooks in sessions that were not trusted in `/hooks` send nothing; the app shows "Connected" with the trust note rather than claiming events flow.
- On Windows, the Codex shell isn't documented; the PowerShell form is used (same as Cursor). If Codex runs commands through another shell the hook fails silently (async, no effect on Codex) and the manual test in `docs/MANUAL_TESTS.md` should confirm.
- A user who disables Claude Code hooks inside Cursor loses nothing: Cursor's own adapter covers it.
