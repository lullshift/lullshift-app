# ADR-024: Import sessions from the agents' own logs

- **Status:** Accepted (owner decision 34, 2026-11-07)
- **Date:** 2026-11-07
- **Related:** ADR-008 (session state), ADR-010 (hook forwarder), ADR-013 (task context and consent markers; amended by this ADR for one user-started read), ADR-021 (Codex and Cursor), ADR-022 (OpenCode 2.x)

## Context

Lullshift learns about sessions only from hook events. A session that was already running when Lullshift started (or before the agent was connected) is invisible until it next fires a hook, and some never do. The release notes listed this as a known limit. The owner asked for a user-started **Import sessions** action that lists sessions found in the agents' own local logs and lets the user pick which to add.

Until now the app never opened a transcript (ADR-013: only the hook forwarder reads the Claude Code title, with consent). This ADR adds one more reader, in the app, and fixes what it may read, when, and what it may keep.

## Decision

### Only when the user asks

- **Import sessions** is a button in the Sessions header and a second button on the empty state. Nothing is scanned at start-up, in the background or on a timer: the scan starts when the dialog opens and runs on a blocking thread, never the UI thread. "Try again" scans again. Cancel imports nothing.
- **Import (n)** scans again and imports only the picked sessions that are still in the logs, with what the logs say now (re-validation). Picks are checked first: at most 500, a known agent id, and a session id of 1 to 128 ASCII letters, digits, `-`, `_` or `.` (not starting with `.`). Sessions Lullshift already has are shown as "Already in Lullshift", can't be ticked, and are never changed by an import.
- One scan or import runs at a time (`busy` otherwise).

### What is read

Every log is untrusted input.

- **Bounded windows only:** at most the first 64 KiB (working directory, first prompt, title) and the last 256 KiB (title records, turn state) of each file. The middle of a file is never read; a 340 MB transcript costs the same as a small one. A line cut by a window edge is skipped; a line longer than the window can never be parsed. Malformed lines are skipped.
- **Plain files and folders only:** symlinks, junctions and other reparse points are skipped (checked without following links, and the opened handle is checked again), as are relative and network (UNC) paths.
- **Caps:** at most 20,000 directory entries examined, the newest 500 sessions by last write, and 5 seconds. When a cap or the deadline stops the scan, the dialog says "The list may be incomplete".
- **No path from a log is ever opened.** The working directory is display text only: control and invisible characters removed, cut to 512 characters.
- **Per session, kept for the dialog:** agent, session id, project folder (display), title, last write time and a status guess.
  - **Title:** the agent's own saved title if it has one, else the first line of the first real user prompt (text that starts with `<` is agent markup and skipped). Cleaned like captured text (control, bidi and invisible characters removed, secret-like strings hidden), whitespace collapsed, cut to 80 characters. Shown in the dialog only.
- Nothing is uploaded. There is no network use.

### What is kept, and consent

- An imported session is stored in `sessions` with its guessed state and `last_seen_utc`/`last_received_utc` set to the log's last write (capped at now). Nothing is written to the event log.
- Migration **0016** adds `sessions.imported_utc` (the "from log" mark). The usual backup is made before migrating; a failed step rolls back to version 15 (tests cover upgrade with backup, rollback and recovery from the backup).
- **The title is kept only with Task context on for that agent:** the consent marker `<app data>/context-capture/<adapter>` must exist **and** the stored setting must be on (`context::capture`, the existing second gate). It is stored where agent titles go, `session_context.agent_title`, and is removed by "Clear captured text" like any other captured text. Without consent nothing but the session row is stored. Titles are never logged.

### Status guess (limits)

From the tail of the log and the file's last write time:

| Guess | Rule | Stored state |
|---|---|---|
| Waiting for you | the last turn-related record is a turn end | `idle` |
| May need you | a tool call has no result and the log has been quiet for more than 30 s (and less than a day) | `needs_attention` |
| Working | written in the last 2 minutes and the turn not ended | `working` |
| Unknown | anything else | `unknown` |

A guess, not an observation: a long-running tool looks like "May need you", a crashed agent looks like "Working" for two minutes, and a session closed without a turn end looks unknown. Cursor's transcript has no tool results and Cursor has no needs-you signal (ADR-021), so Cursor is never "May need you". OpenCode's guess is only Working or Unknown (message parts are not read).

### No alerts for imported sessions

- Import is not an ingest: it produces no state change, so no notification, sound or `agent-attention` event (which pauses activities).
- The tray mascot ignores imported sessions (like disconnected ones).
- In the UI an imported session is not "tracked yet": it never counts as Needs you or Working (columns, summary, nav badge), it is listed under History with a neutral pill naming the guess and a **From log** tag, and its reason line says the state is a guess.
- **The first real hook event takes over:** in the same ingest transaction the guessed record is discarded (so even an older event timestamp replaces it), the change counts as a state change (so a needs-you event notifies as usual), and `imported_utc` is cleared. Disconnect and Delete treat imported sessions like any other.

### Where each agent keeps its logs (checked 2026-11-07)

| Agent | Location | Session id | Verification |
|---|---|---|---|
| Claude Code | `$CLAUDE_CONFIG_DIR/projects` (same rule as the adapter), else `~/.claude/projects/<encoded cwd>/<sessionId>.jsonl`; sub-agent files in `<sessionId>/subagents/` are not scanned | file name = hook `session_id` | **Verified** on Windows against 75 real transcripts (CLI and the desktop app's Code tab). Ids Lullshift had tracked from hooks matched transcript names. Title records `custom-title.customTitle` and `agent-name.agentName` (equal in every file seen; `slug` is a random word triple and not used). Turn ends: `assistant` `stop_reason: end_turn`, `system` `stop_hook_summary` / `turn_duration`. |
| Cursor | `~/.cursor/projects/<encoded workspace>/agent-transcripts/<id>/<id>.jsonl` | folder name = file name = conversation id; hooks send `conversation_id` | Format **verified** against 54 real transcripts (folder name equals file name in all). Lines `{role, message:{content:[{type,text}]}}` and `{type:"turn_ended", status}`; the prompt is inside `<user_query>…</user_query>`. No timestamps (file time used), no working directory (the encoded workspace folder name is shown), no tool results. The id match with a live hook event is **docs-based**: no Cursor session had been tracked here to compare. |
| Codex | `$CODEX_HOME` (absolute only, same rule as the adapter), else `~/.codex`; `sessions/YYYY/MM/DD/rollout-<time>-<uuid>.jsonl`; titles from `session_index.jsonl` `{id, thread_name, updated_at}` (tail only, last wins) | the UUID in the file name = `session_meta.payload.id` (= `payload.session_id`) | Format **verified** against 18 real rollouts (Codex CLI, VS Code extension and Codex Desktop were installed on the development machine after all). A file whose `session_meta` names another id is skipped. Turn: `event_msg` `task_started` / `task_complete` / `turn_aborted`; calls: `response_item` `function_call` / `custom_tool_call` / `local_shell_call` and their `*_output` by `call_id`. The hook `session_id` being this thread id is **source-based** (openai/codex), not compared live. |
| OpenCode 2.x | SQLite `opencode.db` in `$XDG_DATA_HOME/opencode`, else `~/.local/share/opencode` (every platform), or an absolute `$OPENCODE_DB`. A pre-release channel's `opencode-<channel>.db` is not looked for. | `session.id` = the plugin's `sessionID` | **Unverified:** OpenCode is not installed here. From source, tag v2.2.5 (`packages/cli/src/database-path.ts`, `packages/util/src/global-roots.ts`, first migration in `packages/core/src/database/migration/`). Opened read-only (`SQLITE_OPEN_READ_ONLY`, no-follow, `query_only`, 500 ms busy timeout); reads `id`, `directory`, `title`, `time_updated` of top-level, non-archived sessions, newest 500, text cut in SQL. Tested with a fixture database only. An unknown schema shows "couldn't be read", never an error. |

These formats are internal to each agent and can change in any release. A changed format means fewer or untitled sessions, or "Unknown" guesses, never a crash.

## Consequences

- Users can bring sessions that were already running into Lullshift, by choice, and see honestly that their state is a guess until the agent reports.
- Privacy: this is the one place the app reads an agent's own files. The read is user-started, bounded, local and shown before anything is kept; titles persist only under the existing Task context consent. The PRD's "metadata only" rule gains this narrow, user-started exception; `docs/user/privacy.md` says so.
- Maintenance: four parsers track undocumented formats. They are fixture-tested (`crates/agent-adapters/src/logscan/tests.rs`) and re-checked by manual check M12-17.
- Performance (development machine, debug build): 147 real sessions from three agents in 2.4 s, well under the 5 s deadline.
