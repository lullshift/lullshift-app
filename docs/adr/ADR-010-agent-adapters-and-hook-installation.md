# ADR-010: Agent adapters, hook forwarder and hook installation

- **Status:** Accepted (Sprint 02)
- **Date:** 2026-11-05
- **Owner:** Implementation lead
- **Related:** ADR-008 (event contract), ADR-009 (IPC), `docs/adapters/CLAUDE_CODE.md`

## Context

Lullshift observes agents through the hooks they offer. Hooks run inside the agent's process tree with the user's permissions, and a misbehaving hook can block or steer the agent (for example, exit code 2 blocks an action; stdout from some hooks becomes model context). Editing an agent's settings is sensitive user configuration (CLAUDE.md rule 5).

## Decision

### Forwarder: the Lullshift executable in hook mode

- The installed hook command is the Lullshift executable itself, `lullshift-desktop hook <adapter> --data-dir <dir>`. Hook mode never starts the app UI. This avoids packaging a second binary and its path drifting from the app.
- **Zero-effect contract:** hook mode always exits 0, writes nothing to stdout or stderr, and exits 0 even on panic (panic hook). It reads at most 1 MiB of stdin and finishes within 2 seconds in all cases. Events the app would reject are dropped locally.
- Hooks are registered in **exec form** (no shell) with `async: true`, so the agent never waits for Lullshift.
- The event time is taken when the hook process starts, before stdin is read, so separate hook processes keep the order in which the agent ran them.
- Each hook run gets a fresh random `eventId`. Duplicate protection for users comes from state: notifications fire only when a session changes into `needs_attention`, and at most once per session in 15 seconds. That window covers a `working` event landing between a permission request and Claude Code's permission notification.
- Only enum-like hints are forwarded; prompts, messages, tool data, paths and transcripts are never read into Lullshift types. Exception since Sprint 04: when the user opts in to task-context capture, the first prompt and the agent's question text are sent beside the event, cleaned and capped (ADR-013). Tool data, paths and transcripts are still never read.

### Installer

- **Consent first:** install runs only from an explicit action in the app, after a panel that names the exact file, the events and what is and isn't sent. The install commands are granted to the `main` window only; activity webviews must never receive that capability. After the panel, Rust shows a native confirmation dialog that names the settings file, and installs only if the user chooses **Connect** (S03-T07, Sprint 03). A script in a webview therefore can't install hooks on its own.
- **Scope:** Claude Code user settings (`~/.claude/settings.json`, or `$CLAUDE_CONFIG_DIR/settings.json`), so all projects are covered. Project and managed settings are never touched.
- **Safety:**
  - A settings file that isn't a JSON object, or whose `hooks` value isn't an object of arrays, is refused and left untouched.
  - The original bytes are backed up under `<app data>/adapters/claude-code/backups/` before the first change; backups are never deleted automatically.
  - Writes go to a uniquely named temporary file in the same folder, keep the original permissions, are synced, then renamed over the file. They abort if the file changed since it was read. A change made in the instant between that check and the rename is lost; avoiding that needs file locking that Claude Code doesn't use.
  - Symlinked, non-regular and read-only settings files are refused, never replaced. Backups are owner-only on Unix.
  - Key order, every unrelated setting and a UTF-8 byte order mark are preserved. The file is re-serialized, so spacing and formatting may change; the consent panel says so.
- **Idempotent and upgradable:** installing again first removes Lullshift's own handlers (exec-form `lullshift*` executables with `hook claude-code` arguments), then adds current ones: one handler per event.
- **Uninstall:**
  - If the file is byte-identical to what Lullshift last wrote, and has been at every repair since the first install, the original is restored byte for byte (or removed, if it didn't exist). A missing backup falls back to removal.
  - Otherwise only Lullshift's handlers, and the keys Lullshift itself created, are removed; the user's later edits stay.
- **Record:** the undo information (paths, hashes, created keys; never file content) is stored in SQLite (`adapter_installations`, migration 0002) and must be saved for the install to count; if saving fails, the install is undone. Status, repair and uninstall act on the recorded settings path, since the app's environment (`CLAUDE_CONFIG_DIR`) can differ between launches. Without a record, uninstall still removes Lullshift's handlers. Install and uninstall never run concurrently.

### Protocol addition

`session_ended` / `ended` joins the event contract (ADR-008): the provider closed the session without claiming the task was done. It is terminal and distinct from `completed`.

### Windows pipe access (S02-T06, from the Sprint 01 review)

Keep the default pipe DACL for v1. Owner-only access needs either `unsafe` Win32 code or a dependency. The `interprocess` crate's safe security-descriptor API was evaluated: pipe creation failed in our environment (error 123, even without a descriptor), so it wasn't adopted.

Pipe names can be listed by other local users (for example by listing `\\.\pipe\`), so the per-launch random name prevents guessing but not discovery.

The residual risk stays as in ADR-009: other local users can hold connection slots, but they can't authenticate or read events. Revisit if a multi-user deployment needs it.

## Consequences

- An app move or update changes the executable path; the app shows "Needs repair" and offers to update the hooks.
- If Lullshift is removed or moved without disconnecting, Claude Code keeps trying to run a missing executable. The consent panel warns about this; an uninstaller cleanup step is tracked for the release work (S11-T06). Manual cleanup: delete the handlers whose `args` start with `hook`, `claude-code` from the settings file.
- Sessions in folders Claude Code hasn't trusted send nothing (Claude Code holds hooks back), and interrupts send no event. The UI shows these sessions as stale, not as finished.
- Every hook run starts a short-lived Lullshift process. Exec form and the 2-second cap keep this cheap.
