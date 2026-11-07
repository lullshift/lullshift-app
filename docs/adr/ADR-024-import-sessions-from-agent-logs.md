# ADR-024: Import sessions from the agents' own logs

- **Status:** Accepted (owner decision 34, 2026-11-07)
- **Date:** 2026-11-07
- **Related:** ADR-008 (session state), ADR-010 (hook forwarder), ADR-013 (task context and consent markers; amended by this ADR for one user-started read), ADR-021 (Codex and Cursor), ADR-022 (OpenCode 2.x)

## Context

Lullshift learns about sessions only from hook events. A session that was already running when Lullshift started (or before the agent was connected) is invisible until it next fires a hook, and some never do. The release notes listed this as a known limit. The owner asked for a user-started **Import sessions** action that lists sessions found in the agents' own local logs and lets the user pick which to add.

Until now the app never opened a transcript (ADR-013: only the hook forwarder reads the Claude Code title, with consent). This ADR adds one more reader, in the app, and fixes what it may read, when, and what it may keep.

## Decision

### Only when the user asks

- **Import sessions** is a button in the Sessions header and a second button on the empty state. Nothing is scanned at start-up, in the background or on a timer: the scan starts when the dialog opens and runs on a blocking thread, never the UI thread. "Try again" scans again and moves focus to the status line. Cancel imports nothing; Cancel or closing the dialog while the logs are being read stops the scan (`import_cancel` sets a cancel flag the scan checks between files and folders and that the OpenCode query's progress handler checks), so reopening is not "busy".
- **Import (n)** scans again and imports only the picked sessions that are still in the logs, with what the logs say now (re-validation). Picks are checked first (after Tauri has deserialized the list, which is bounded only by the IPC message size; the check itself costs nothing): at most 500, a known agent id, and a session id of 1 to 128 ASCII letters, digits, `-`, `_` or `.` (not starting with `.`). Sessions Lullshift already has are shown as "Already in Lullshift", can't be ticked, and are never changed by an import.
- One scan or import runs at a time (`busy` otherwise). A scan that panicked never leaves this lock stuck: a poisoned lock is taken over, only a running scan is `busy`. File times outside the calendar (for example a write time past year 9999) are skipped, never converted with a panic.

### What is read

Every log is untrusted input.

- **Bounded windows only:** at most the first 64 KiB (working directory, first prompt, title) and the last 256 KiB (title records, turn state) of each file. The middle of a file is never read; a 340 MB transcript costs the same as a small one. A line cut by a window edge is skipped; a line longer than the window can never be parsed. Malformed lines are skipped.
- **Plain files and folders only, below the root:** every folder and file the scan walks *below* an agent's log root is checked without following links and skipped if it is a symlink, junction or other reparse point; the opened handle is checked again. The root itself and the folders above it are **not** link-checked: they come from the user's environment (home folder, `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `XDG_DATA_HOME`, `OPENCODE_DB`). Relative and UNC paths are refused.
- **Local roots only (Windows):** the root is resolved (every link, junction and drive mapping on the way) and must land on a drive letter (`\\?\X:\…`). A mapped network drive resolves to `\\?\UNC\…` and is refused, and the dialog says that agent's logs are "not on a local drive". Elsewhere only the path form is checked: a network file system mounted inside the home folder is not detected.
- **Caps:** at most 20,000 directory entries examined, split evenly between the three file-based agents (so one agent's huge folder can't hide another), the newest 500 sessions by last write, and 5 seconds. When a cap or the deadline stops the scan, the dialog says "The list may be incomplete" and names each agent whose list was cut short.
- **Ids are cross-checked:** a Claude Code transcript whose first record carrying a `sessionId` names another session than the file is skipped. A Codex rollout is listed only if its first record is a `session_meta` whose id equals the UUID in the file name; one whose first record doesn't fit in the 64 KiB head (so can't be checked) is skipped too. Cursor has no id inside the transcript; the folder and file names must match.
- **No path from a log is ever opened.** The working directory is display text only: control and invisible characters removed, cut to 512 characters.
- **Per session, kept for the dialog:** agent, session id, project folder (display), title, last write time and a status guess.
  - **Title:** the agent's own saved title if it has one, else the first line of the first real user prompt (text that starts with `<` is agent markup and skipped). Cleaned like captured text (control, bidi and invisible characters removed, secret-like strings hidden), whitespace collapsed, cut to 80 characters. The dialog shows either; the two are kept apart (below).
- Nothing is uploaded. There is no network use.

### What is kept, and consent

- An imported session is stored in `sessions` with its guessed state and `last_seen_utc`/`last_received_utc` set to the log's last write (capped at now). Nothing is written to the event log.
- Migration **0016** adds `sessions.imported_utc` (the "from log" mark). The usual backup is made before migrating; a failed step rolls back to version 15 (tests cover upgrade with backup, rollback and recovery from the backup).
- **Title and first prompt are kept only with Task context on for that agent:** the consent marker `<app data>/context-capture/<adapter>` must exist **and** the stored setting must be on (`context::capture`, the existing second gate). The agent's own title is stored where agent titles go, `session_context.agent_title`. The first prompt is what the user asked, not a title, so it is stored as the captured intent (`captured_intent`, labelled "Captured from the agent (opt-in)" in the export), never as a title. Both are removed by "Clear captured text" like any other captured text. Without consent nothing but the session row is stored. Neither is ever logged.
- Like titles and intents captured from hooks, these texts reach the Companion as session context when the user includes a session. They are agent- or user-written text from a file Lullshift doesn't control, so they are a path for indirect prompt injection, with the same handling as existing captured text (ADR-013, ADR-020).

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
- The Companion never presents a guess as a report: its session overview leaves imported sessions out (and says how many it left out), and session facts and the export say the state was "guessed from the agent's log" (JSON export: `stateSource: "guessed_from_log"`, `importedFromLogUtc`).
- In the UI an imported session is not "tracked yet": it never counts as Needs you or Working (columns, summary, nav badge), it is listed under History with a neutral pill naming the guess and a **From log** tag, and its reason line says the state is a guess.
- **The first real hook event takes over:** in the same ingest transaction the guessed record is discarded (so even an older event timestamp replaces it), the change counts as a state change (so a needs-you event notifies as usual), and `imported_utc` is cleared. Disconnect and Delete treat imported sessions like any other.

### Where each agent keeps its logs (checked 2026-11-07)

| Agent | Location | Session id | Verification |
|---|---|---|---|
| Claude Code | `$CLAUDE_CONFIG_DIR/projects` (same rule as the adapter), else `~/.claude/projects/<encoded cwd>/<sessionId>.jsonl`; sub-agent files in `<sessionId>/subagents/` are not scanned | file name = hook `session_id` | **Verified** on Windows against 75 real transcripts (CLI and the desktop app's Code tab). Ids Lullshift had tracked from hooks matched transcript names. Title records `custom-title.customTitle` and `agent-name.agentName` (equal in every file seen; `slug` is a random word triple and not used). Turn ends: `assistant` `stop_reason: end_turn`, `system` `stop_hook_summary` / `turn_duration`. |
| Cursor | `~/.cursor/projects/<encoded workspace>/agent-transcripts/<id>/<id>.jsonl` | folder name = file name = conversation id; hooks send `conversation_id` | Format **verified** against 54 real transcripts (folder name equals file name in all). Lines `{role, message:{content:[{type,text}]}}` and `{type:"turn_ended", status}`; the prompt is inside `<user_query>…</user_query>`. No timestamps (file time used), no working directory (the encoded workspace folder name is shown), no tool results. The id match with a live hook event is **docs-based**: no Cursor session had been tracked here to compare. |
| Codex | `$CODEX_HOME` (absolute only, same rule as the adapter), else `~/.codex`; `sessions/YYYY/MM/DD/rollout-<time>-<uuid>.jsonl`; titles from `session_index.jsonl` `{id, thread_name, updated_at}` (tail only, last wins) | the UUID in the file name = `session_meta.payload.id` (= `payload.session_id`) | Format **verified** against 18 real rollouts (Codex CLI, VS Code extension and Codex Desktop were installed on the development machine after all). A file whose `session_meta` names another id is skipped. Turn: `event_msg` `task_started` / `task_complete` / `turn_aborted`; calls: `response_item` `function_call` / `custom_tool_call` / `local_shell_call` and their `*_output` by `call_id`. The hook `session_id` being this thread id is **source-based** (openai/codex), not compared live. |
| OpenCode 2.x | SQLite `opencode.db` in `$XDG_DATA_HOME/opencode`, else `~/.local/share/opencode` (every platform), or an absolute `$OPENCODE_DB`. A pre-release channel's `opencode-<channel>.db` is not looked for. | `session.id` = the plugin's `sessionID` | **Unverified:** OpenCode is not installed here. From source, tag v2.2.5 (`packages/cli/src/database-path.ts`, `packages/util/src/global-roots.ts`, first migration in `packages/core/src/database/migration/`). The database is untrusted: opened read-only and no-follow, and **immutable** (`file:…?mode=ro&immutable=1`, the path percent-encoded, URI parsing enabled for that one open only) when no `-wal` file exists, so nothing is created beside it; with a `-wal` file a plain read-only open (immutable would ignore recent writes; SQLite may then create a `-shm` file). Defensive mode on, `trusted_schema` off, `mmap_size=0`, `query_only`, 500 ms busy timeout. `session` must be an ordinary table (not a view or virtual table), so no schema-supplied SQL runs; a progress handler aborts the query at the scan deadline or on cancel. Reads `id` (at most 128 bytes, checked in SQL), `directory`, `title`, `time_updated` of top-level, non-archived sessions, newest 500, text cut in SQL. Tested with fixture databases only (including a recursive view and a virtual table named `session`). An unknown or hostile schema shows "couldn't be read", never an error. |

These formats are internal to each agent and can change in any release. A changed format means fewer or untitled sessions, or "Unknown" guesses, never a crash.

## Consequences

- Users can bring sessions that were already running into Lullshift, by choice, and see honestly that their state is a guess until the agent reports.
- Privacy: this is the one place the app reads an agent's own files. The read is user-started, bounded, local and shown before anything is kept; titles persist only under the existing Task context consent. The PRD's "metadata only" rule gains this narrow, user-started exception; `docs/user/privacy.md` says so.
- Maintenance: four parsers track undocumented formats. They are fixture-tested (`crates/agent-adapters/src/logscan/tests.rs`) and re-checked by manual check M12-17.
- Performance (development machine, debug build): 147 real sessions from three agents in 2.4 s, well under the 5 s deadline.
