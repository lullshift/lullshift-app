# ADR-013: Context Keeper data and opt-in task-context capture

- **Status:** Accepted (Sprint 04)
- **Date:** 2026-11-05
- **Owner:** Product owner (capture decision), implementation lead
- **Related:** ADR-008 (event contract), ADR-009 (IPC), ADR-010 (hook forwarder), PRD 4.2 and 4.3

## Context

Context Keeper and Next Move help the user return to agent work: the original intent, notes, open decisions, a review checklist and follow-up items (PRD 4.2, 4.3). Until now Lullshift deliberately read no task content from agents (ADR-010): the hook forwarder keeps only enum-like hints.

The product owner chose (2026-11-05) to let users **opt in** to capturing a small amount of task text from the agent's hooks: the first prompt as the intent, and the text of what the agent is asking. Everything else stays as before. This ADR fixes where that text may travel, where it is stored, and how it is removed.

## Decision

### What may be captured, and only with consent

- **Off by default,** per tool (adapter). The user turns it on in Integrations, after a panel that says exactly what is kept.
- When on, for Claude Code:
  - The first `UserPromptSubmit.prompt` of a session becomes its **intent**. Later prompts are not kept. A later prompt does clear the stored question, since the agent has had its answer.
  - `Notification.message` (permission prompts, questions, idle prompts) becomes **what it's asking**. For `PermissionRequest`, only `tool_name` is used ("Permission to use Bash"); tool inputs are never read.
- **Never captured,** consent or not: tool inputs and outputs, commands, file paths, the working directory, transcripts, assistant messages, session titles.
- **Limits and cleaning,** applied in the forwarder before anything leaves the hook process, and again in the app:
  - Intent: at most 2,000 characters. What it's asking: at most 300.
  - Text over the limit is cut, not rejected, so the event itself is never lost. Empty text counts as absent.
  - Control, bidi and invisible formatting characters are removed. Line breaks and tabs are kept, so words don't run together.
  - Secret-like strings are replaced with `[hidden]`:
    - private key blocks;
    - common key prefixes followed by at least four key characters: `sk-`, `ghp_`, `gho_`, `github_pat_`, `xoxb-`, `xoxp-`, `AKIA`, `AIza`;
    - `Bearer <token>`;
    - assignments with `=` or `:` to `password`, `passwd`, `pwd`, `secret`, `token`, `api_key`, `apikey` or `api-key`;
    - URL credentials (`scheme://user:pass@host`);
    - key words with any prefix or suffix before `=` or `:` (`aws_secret_access_key=`, `DB_PASSWORD_PROD=`), and the prefixes `sk_live_`, `sk_test_`, `rk_live_`, `npm_` and `glpat-`;
    - runs of 40 or more characters, counting `+ / = - _` as part of the run. A run that contains `-`, `_` or `/` is hidden only if it also has a digit or mixed case, so long slugs and paths stay readable. A standard UUID stays readable too.
  - This is a safety net, not a guarantee, and the consent panel says so. It errs on hiding: "token: expired" in prose hides "expired".

### Where it travels

- **Two gates.** The forwarder sends text only if the consent marker file `<app data>/context-capture/<adapter>` exists. The app also checks the stored setting and discards any text that arrives while capture is off.
  - The app writes and deletes the marker with the setting, and reconciles it with the stored setting at start-up.
  - Marker names come only from the list of known adapter ids, never from user input.
- **Beside the event, not in it.** IPC protocol v1 gains an optional envelope field: `{"v":1,"token":…,"event":{…},"context":{"intent"?,"ask"?}}`. The agent event schema is unchanged, so `events.event_json` never holds task text and the event log stays content-free (ADR-008). Unknown fields in `context` are rejected.
- Nothing is sent anywhere else. There is no network use in any of this.

### Where it is stored

Migration 0004 adds:

- `session_context`: one row per session. User-written intent and notes, captured intent and ask (each with its capture time; the unused `latest_request` column always stays empty), and when the return card was last reviewed. Captured and user-written text are kept in separate columns, so the panel can label where each came from and "clear captured text" removes only what was captured. The panel shows captured intent read-only, and "Your intent" starts empty. Captured text becomes the user's own only through a deliberate "Copy into my intent".
- `session_items`: open decisions, review checklist items and Next Move items (with a category: question, acceptance criterion, test idea or decision), each with text, done flag, position and creation time.
- `session_links`: user-made links between two sessions ("continues", shown both ways).

### Retention and deletion

- **Agent events** are deleted after a configurable number of days: 7, 30 (default), 90 or 365. Purging runs at start-up and then hourly. Sessions keep their last state, so History still lists them; their timeline shows only what is left.
- **Context Keeper data** (notes, items, captured text) is kept until the user deletes it. The panel and Settings say both rules in plain words. Captured text is not tied to event retention; that residual risk is accepted for now and may change.
- **Wiping.** SQLite runs with `secure_delete` on, and the write-ahead log is checkpointed after a clear or a delete. Deleted text therefore doesn't linger in free pages. Backups made before a migration can still hold it, and they are kept in the app data folder.
- **Delete session** removes the session row, all its events, its context, items and links, in one transaction. It can't be undone; the UI asks first.
- **Turning capture off** deletes the marker file at once. The user can also clear captured text per session, or everywhere at once.

### Export

- Per session, as Markdown or JSON, to a file the user picks in a native save dialog opened from Rust. The webview never receives a file path or writes files. In Markdown, captured and typed free text is placed in fenced code blocks, so it can't add headings, HTML or links to the document.
- The export contains session metadata, the timeline, notes, items, links and, labelled as such, captured text. Nothing leaves the device unless the user moves the file.

### Next Move and the agent

- Next Move items are only ever copied to the clipboard by the user, from the main window. Lullshift has no channel to write into an agent, and this ADR adds none. The hook forwarder's zero-effect contract (no stdout, exit 0) holds with capture on.

### UI preference

- The session detail panel can be pinned (docked) or floating. The choice is stored as a preference (`ui`), default pinned. Under 720 px the panel always floats, without changing the stored choice.
- The same preference holds the content width: full (default) or comfortable. Older stored preferences without it read as full width. Each change saves the whole set, so one setting never overwrites another.

## Consequences

- Users who opt in get a useful return card without typing; users who don't, still get notes, decisions, checklist and Next Move, typed by them.
- Captured text is sensitive local data. It is never logged, never part of the event log, never in notifications, and can be cleared or deleted at any time.
- Other adapters (Cursor, Codex) must map their own fields to `intent` and `ask` under the same limits and gates, and update this ADR if they need anything else.
- The redaction list will need maintenance; it is tested with fixtures in `agent-adapters`.
