# ADR-025: Settle missed and silently ended sessions

- **Status:** Accepted (owner decision 35, 2026-11-07)
- **Date:** 2026-11-07
- **Related:** ADR-008 (session state machine; amended by this ADR), ADR-009 (IPC envelope), ADR-010 (hook mode: zero output, exit 0, 2 s deadline), ADR-012 (the crates that may use `unsafe`), ADR-024 (Import sessions from logs, whose reader the catch-up reuses)

## Context

Hooks are the live signal and stay so. They leave two gaps:

1. A session killed, closed or deleted without a "session ended" event stays "Not confirmed since Lullshift started" forever.
2. Events sent while Lullshift is closed are lost, so after a start the sessions it knew hold old states.

Research (recorded with decision 35): OpenTelemetry in Claude Code and Codex is push-only, has no "waiting for you" or "ended" events and only covers sessions started after it is enabled; Cursor's is Enterprise and cloud-only; none of the four tools supports A2A; Codex and OpenCode have status APIs but attaching to sessions the user started is not documented. So the owner chose two additions to hooks, below.

## Decision

### 1. Process check (always on, no log reading)

**What the hook records.** `lullshift-desktop hook <adapter>` looks up the agent's process and sends it beside the event as a new optional envelope field, `process: {"pid": <u32>, "start": <u64>}` (next to `context`; it is not task text, so it is not behind the Task context consent). The agent process is the first non-shell ancestor of the hook process. Shells (`sh`, `bash`, `zsh`, `fish`, `dash`, `ash`, `ksh`, `mksh`, `csh`, `tcsh`, `cmd`, `powershell`, `pwsh`, `nu`, `xonsh`, with any `.exe` suffix, directory or login dash) are looked through, because on Windows the hook is often run by `cmd` or Git Bash. The walk is bounded to 8 ancestors and gives up (no process, never a guess) on a missing or unreadable parent, a process that is its own parent, too many stacked shells, or a parent that **started after** its child (its PID was reused, so it is not the real parent).

**The start key.** `start` is an opaque number that tells a process from a later one that reuses its PID; only equality means anything. Windows: the creation time (`GetProcessTimes`). Linux: a hash of the boot id and the `starttime` tick in `/proc/<pid>/stat` (the boot id is included because after a reboot the same PID can start at the same tick). macOS and other Unix: a hash of `ps -o lstart=` (one-second resolution). Values are kept to 62 bits so they fit a signed database integer.

**Hook contract unchanged.** The lookup is a handful of system calls (one process snapshot on Windows, a few `/proc` reads on Linux, one `ps` run per ancestor on macOS, each capped at 800 ms by the surrounding 2 s deadline) and runs inside the existing deadline thread, before the send, so a slow lookup eats into the send budget and never extends it. Any failure means "no process". Exit code 0, no stdout or stderr, no marker, no logging of the numbers. The hook only reads process information. A new hook against an older app that rejects the unknown `process` field (the reply is `malformed`) sends the same event once more without it, inside what is left of the budget, so a version mismatch cannot lose an event.

**Untrusted input.** The PID and start key arrive over the authenticated pipe, but are still validated: the PID must be 1 to `i32::MAX` (Unix) or 1 to `u32::MAX - 1` (Windows), the start key 1 to 2^62-1. An out-of-range value is dropped (the event is kept, without a process); a malformed field (wrong type, extra key) is a `malformed` request like any other. The app never signals, kills, opens for write or reads the memory of any process: Windows opens with `PROCESS_QUERY_LIMITED_INFORMATION` only; Linux reads `/proc/<pid>/stat` and `comm`; macOS runs `ps` for one PID. A stored value that is out of range (a tampered database) is skipped.

**What the app does with it.** Each event's process replaces the session's recorded one; an event without one clears it (a stale PID is never acted on). At start-up and about every 45 seconds a background thread, never the UI thread, takes the sessions that are not finished, still tracked (not disconnected) and hold a recorded process (at most 1,000, newest first) and looks at each distinct PID once, outside the store lock:

| Look | Meaning |
|---|---|
| Running with the same start key | alive, no change |
| No such process (Windows `ERROR_INVALID_PARAMETER` or an exited process, Linux no `/proc/<pid>` or a zombie, macOS `ps` finds none or a zombie) | gone |
| Running with a different start key | gone (the number was reused) |
| Anything else: access denied, `/proc` unreadable, `ps` failed or timed out | unknown, **no change** |

Unknown or missing PID means no change, always.

**Ending a session without an event.** A gone session is ended by `Store::end_session_process_gone`, a compare-and-set in one transaction: it applies only if the session still holds exactly the looked-at process, is still tracked and unfinished, and has received nothing since the moment the look began (so a newer event that arrived while processes were being looked at wins). Then `reducer::process_gone` sets the state to `ended` with a timestamp of "now", the stored `ended_reason` to `process_gone`, and clears the process and any imported mark. **No event is written** to the event log, `last_seen` and `last_received` stay as they were (nothing was received), and the dashboard says "Ended (process gone)" and the export says the end was inferred (`stateSource: "inferred_process_gone"`). Already finished sessions (`completed`, `failed`, `cancelled`, `ended`) are never touched: a reported end is a fact.

**A real event reopens it.** `ended` with a reason is an ordinary state with a timestamp, so the normal rule applies: an event with a timestamp at or after the mark replaces it (an event's timestamp is the moment its hook started, capped at receipt, so a hook that starts after the check qualifies) and the reason is cleared in the same transaction. An event older than the mark changes nothing and keeps the reason. A heartbeat, which reports no state but proves the agent is alive, reopens the session to `unknown` ("connected, state not reported"), the honest state. A reported `session_ended` after the mark is a reported end and clears the reason.

**Known limits.** (a) If the "agent process" is not the program that holds the session (for example a short-lived launcher that exits while the agent keeps running), a live session is shown "Ended (process gone)" until its next event, which reopens it. (b) A long-lived host that serves many sessions (Cursor's app) is never "gone" while it runs, so sessions it ended silently stay as they were. (c) Hooks run in a different PID namespace than the app (containers, WSL) record PIDs the app cannot see: they look "gone" or collide with another process. A live session can then be ended wrongly, and its next event reopens it. (d) Process start keys on macOS have one-second resolution.

### 2. Log catch-up at start-up (opt-in, off by default)

A setting, **Catch up from agent logs at startup**, in Settings under **Sessions**, stored as the preference `session_tracking_prefs` (`catchUpFromLogs`, missing or unreadable means off). It is read once at start-up. When on, after the first process check, the worker runs one scan with the ADR-024 reader, with all of its hardening, unchanged: bounded head and tail windows, plain files below the root only, local roots only, entry and session caps, 5 s deadline, cancel flag. It shares Import's one-at-a-time lock (it is skipped as `busy` if an import or scan is running) and has its own cancel flag, so Import's Cancel never stops it and no UI waits on it. It runs on the background thread, so the UI is never blocked.

**Scope.** It only updates sessions Lullshift already tracks (`Store::apply_log_hint` updates an existing row and returns `false` for an unknown one; it never inserts, so adding sessions stays Import's job). It keeps only the agent, session id, last write time and the guess; the shared reader parses titles and prompts in memory as part of its scan, and they are dropped at once, never stored, even when Task context is on. Nothing is written to the event log.

**When a log applies.** A log guess applies only if all of these hold: the log's last write (capped at now) is more than 10 seconds newer than Lullshift's own last word (`last_seen`; the margin covers a log and a hook describing the same moment a few seconds apart); the guess is a state (`working`, `idle`, `needs_attention`, not `unknown`); the session is not finished (logs are written after a session ends, and a reported end is a fact); it was not ended by the process check (a gone process outranks a log); and its tool was not disconnected. Applying a guess sets the state and times from the log and marks the session exactly like an import (`imported_utc`, `FROM_LOG_LABEL`, `isImported`): a guess, not reported, listed under History with a **From log** tag, never notifying, never waking the tray, never pausing an activity. The first real hook event replaces it. The recorded process is kept, so the process check still covers the session, and a gone process turns the guess into "Ended (process gone)". Running it twice changes nothing.

**Honest about failures.** Settings shows the last outcome of this run: sessions updated, agents whose logs exist but could not be read or are on a network drive, "older sessions may not have been checked" when a cap or the deadline cut the scan short, skipped because an import was running, or local data unavailable. A turned-on setting that has not run yet says it runs at the next start.

## Schema

Migration **0018** adds `sessions.process_pid`, `sessions.process_start` and `sessions.ended_reason` (0017 is reserved for the Companion branch; the migration list is keyed by version, so the two merge by adding their array entries). Tests: upgrade from version 16 with the usual backup (the backup is a working version 16 database and recovery is opening it), a failed step (a clashing column in the last statement) rolls all three back to 16 with the data kept, and the new columns read as unknown for old rows, so an upgraded session is never ended by the check.

## `unsafe` and dependencies

The Windows lookups need Win32 calls, so a third crate, `crates/process-probe`, joins `macos-webrtc` and `windows-package` as allowed to use `unsafe` (this amends the list in ADR-012). `windows-sys` is pinned to `=0.61.2`, already in the lockfile. Its Windows module has exactly eight `unsafe` blocks (a handle close, `OpenProcess`, `GetLastError`, `GetExitCodeProcess`, `GetProcessTimes`, `CreateToolhelp32Snapshot`, `Process32FirstW`, `Process32NextW`), each with a `SAFETY` comment; the crate denies undocumented blocks and the guard test in `apps/desktop/src-tauri/src/webrtc.rs` fails if there is any unsafe code outside the three crates or a different number here. The Linux and Unix modules are safe code. They were type-checked for `x86_64-unknown-linux-gnu` and `aarch64-apple-darwin` and their parsers are unit-tested everywhere with fixture lines; they have not been run on those systems.

## Consequences

- Sessions whose agent exited without goodbye stop looking live forever and read "Ended (process gone)", and come back if the agent was alive after all.
- Users can opt in to catching up after a closed Lullshift; what it fills in is marked as a guess.
- Privacy: the first part reads only whether a process exists; the second reads agent logs at start-up, but only because the user turned it on, with times and turn state only. The PRD sentence that logs are read only when the user opens Import gains this opt-in exception, and `docs/user/privacy.md` says so. The PRD itself is for the owner to amend.
- Maintenance: three platform lookups and a PID-reuse rule to keep correct; the walk and the verdicts are fixture-tested.
