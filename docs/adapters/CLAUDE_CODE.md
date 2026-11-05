# Claude Code adapter: capabilities and event mapping

Status: Sprint 02. Verified against the official docs on 2026-11-05 (Claude Code 2.1.296 installed locally):
[hooks reference](https://code.claude.com/docs/en/hooks.md), [hooks guide](https://code.claude.com/docs/en/hooks-guide.md), [settings](https://code.claude.com/docs/en/settings.md).
Re-check these pages when Claude Code changes its hooks; the adapter must not rely on anything not listed here.

## Capability matrix

| Capability (architecture §6.1) | Claude Code | Evidence and limits |
|---|---|---|
| `canObserveLifecycle` | Yes | `SessionStart` (`source`: startup, resume, clear, compact, fork), `SessionEnd` (`reason`: clear, resume, logout, prompt_input_exit, other). |
| Working signal | Yes, coarse | `UserPromptSubmit` starts a turn; `PostToolUse` after each tool call. No event fires after a permission is granted until the tool finishes. |
| `canObserveAttention` | Yes | `PermissionRequest` (about to ask), `Notification` with `notification_type` `permission_prompt` (after about 6 s), `idle_prompt` (about 60 s after Claude finished with no typing), `elicitation_dialog`. |
| Turn end | Yes | `Stop` runs when the main agent finishes responding: **per turn, not task completion**. It does not run after a user interrupt (Esc). `background_tasks` lists work still running. `StopFailure` reports API errors. |
| `canObserveCompletion` | **No** | No event says the task is done. Lullshift never maps any Claude Code event to `completed`. |
| `canProvideTaskSummary` | Not used | `prompt`, `session_title`, `last_assistant_message`, tool inputs and transcripts exist but are task content. The adapter never reads or forwards them (PRD privacy rules). |
| `canRemoteApprove` | No (out of scope) | Hooks could answer permission requests; Lullshift never does. |

Known gaps, shown honestly in the UI:

- After the user grants a permission, the session stays "Needs you" until the next tool call finishes.
- An interrupted turn (Esc) sends no event; the session stays "Working" and turns stale after 10 minutes.
- Sessions in folders Claude Code hasn't trusted yet send nothing: hooks are held back until the folder is trusted.
- Hooks are configured in user settings. A project or managed setting with `disableAllHooks` turns them off for that project.

## Event mapping

| Claude Code hook | Lullshift event | Notes |
|---|---|---|
| `SessionStart` | `session_started` → Idle | `metadata.source` |
| `UserPromptSubmit` | `working` | prompt text never read |
| `PostToolUse` | `working` | tool name and input never read |
| `PermissionRequest` | `needs_attention` | `metadata.reason = permission` |
| `Notification` `permission_prompt` / `idle_prompt` / `elicitation_dialog` | `needs_attention` | `metadata.reason`; other notification types are ignored |
| `Stop` with no background tasks | `turn_ended` → Idle | never `completed` |
| `Stop` with background tasks | `heartbeat` | Claude paused for background work; state unchanged |
| `StopFailure` | `failed` | an API error ended the turn; the next prompt reopens the session |
| `SessionEnd` | `session_ended` → Ended | `metadata.reason`; ended is not completed |

Fields forwarded: `session_id` (as `providerSessionId`), the event type, the forwarder's clock time and the small `metadata` hints above. Never forwarded: `transcript_path`, `cwd`, prompts, titles, assistant messages, tool names, inputs or responses.

Replays and duplicates: each hook run gets a fresh random `eventId`. A repeated signal for the same state doesn't change state, and notifications fire only on a change into `needs_attention`, so replays never produce duplicate notifications.

## Hook registration

- **Where:** user settings `~/.claude/settings.json` (or `$CLAUDE_CONFIG_DIR/settings.json` when that variable is set), so every project is observed. Precedence elsewhere is managed > command line > local project > project > user; identical handlers from several files run once.
- **Form:** exec form (`command` = absolute path of the Lullshift executable, `args` = `["hook", "claude-code", "--data-dir", "<app data dir>"]`). No shell, so no profile banner can leak into stdout.
- **`async: true`** on every handler: Claude Code doesn't wait for Lullshift. Claude Code doesn't enforce a timeout on async hooks, so the forwarder has its own 2-second deadline.
- **Zero-effect contract:** the forwarder always exits 0, never writes to stdout or stderr, never exits 2 and catches all failures, including panics. With exit 0 and empty stdout, no event (including `SessionStart` and `UserPromptSubmit`, whose stdout would become context) is affected.
- **Live reload:** Claude Code picks up settings edits through its file watcher; sessions started before the install begin reporting once they reload.

## Install and uninstall

See ADR-010. Install needs explicit consent in the app and shows the exact file and events first. The original settings file is backed up before any change, writes are atomic, installing twice changes nothing, and uninstalling restores the original file byte for byte when nothing else changed it (otherwise only Lullshift's entries are removed).
