# Agent adapter capabilities

What each supported agent tells Lullshift, through its own documented lifecycle hooks. Anything marked "not available" is shown in the app as unknown, never guessed (PRD: represent unknown state honestly). Researched on 2026-11-06 from the official documentation linked below; re-check when an agent changes its hooks.

| Lullshift state | Claude Code | Codex CLI (hooks) | Cursor (hooks) |
|---|---|---|---|
| Session start | Supported (SessionStart) | Supported (SessionStart) | Supported (sessionStart); partial in the Cursor CLI |
| Prompt submitted / working | Supported (UserPromptSubmit, PostToolUse) | Supported (UserPromptSubmit, PostToolUse) | Supported (beforeSubmitPrompt, postToolUse, postToolUseFailure, subagentStop, preCompact, afterAgentResponse); partial in the CLI |
| Needs input or permission | Supported (PermissionRequest, Notification) | Partial: PermissionRequest only, which fires only for commands that need approval; no idle or question event | Not available: no hook fires when Cursor waits for the user |
| Turn ended | Supported (Stop, StopFailure) | Supported (Stop; Interrupt when the user stops a turn, shown as a turn end, not a cancelled session) | Supported (stop, with completed, aborted or error; all are turn ends) |
| Session ended | Supported (SessionEnd) | Partial: SessionEnd also fires after 30 minutes idle and its reason is always "other" | Supported (sessionEnd, with a reason; reason `error` is shown as failed, other reasons as ended) |
| Title | Opt-in, read from the transcript (ADR-013) | Not available | Not available |

## How Lullshift stays an observer

- **Claude Code:** hooks are registered with `async: true` and write nothing back.
- **Codex:** hooks are registered with `async: true` (2 s timeout), so they "can't block or rewrite". Status shows "hooks turned off in Codex" when `config.toml` has `[features] hooks = false` (only that key is read). The forwarder writes nothing to stdout and always exits 0. SessionEnd always runs synchronously, so it gets an explicit 3 s timeout (as Cursor). The "hooks turned off" status appears only when Lullshift's hooks are installed or partly installed. A relative `CODEX_HOME` is ignored. Codex asks the user to review and trust new or changed hooks in `/hooks` before they run, so installing or repairing needs one approval step inside Codex as well.
- **Cursor:** there is no async option. Lullshift registers only events that can't block: sessionStart, beforeSubmitPrompt, postToolUse, afterAgentResponse, stop, sessionEnd, postToolUseFailure, subagentStop and preCompact (the last three added 2026-11-06; an older install with six hooks shows as partly installed until the user connects again). It never registers subagentStart, preToolUse or the other before* hooks, because those can block (Cursor docs checked 2026-11-06). A failed tool call or a finished subagent maps to working, not failed. On beforeSubmitPrompt it writes nothing to stdout and exits 0, which lets the prompt through. It never registers permission or pre-tool hooks, writes no stdout and sets a short timeout. On Windows, Cursor runs hook commands through PowerShell, so the command line is built from fixed parts with no user-controlled text.
- Lullshift never reads `user_email`, prompts or assistant messages from these payloads; only the session id, the event name and enum hints are kept.

## Known gaps and cautions

- **Cursor also runs Claude Code hooks.** Cursor loads hooks from `~/.claude/settings.json` by default, so Lullshift's Claude Code hooks may fire inside Cursor too. The forwarder recognises Cursor's payloads (a `cursor_version` field) and does not label them as Claude Code sessions. The exact payload Cursor sends to Claude-style hooks is not documented, so these events are dropped, not re-mapped; Cursor's own hooks provide the data (ADR-021).
- **Codex `notify`** (one command in `config.toml`, turn-complete only) is not used: it holds a single command, so installing Lullshift's would replace the user's own.
- **Undocumented interfaces are not used:** transcript formats, thread names, schemas from development branches.

Decisions and the installer rules: ADR-021.

## Sources

- Claude Code hooks: see ADR-010.
- Codex hooks: https://learn.chatgpt.com/docs/hooks and https://learn.chatgpt.com/docs/config-file/config-advanced (formerly developers.openai.com/codex).
- Cursor hooks: https://cursor.com/docs/agent/hooks; third-party hooks: https://cursor.com/docs/reference/third-party-hooks; CLI hooks: https://cursor.com/changelog (CLI, 16 January 2026).
