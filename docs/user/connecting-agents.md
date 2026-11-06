# Connecting agents

Lullshift follows Claude Code, Codex and Cursor through each agent's own documented hooks. It only watches. It never answers prompts, approves permissions or sends commands to an agent.

Connecting is optional and you can disconnect at any time. Open **Integrations** in the sidebar.

## What connecting does

1. You choose **Connect** on the agent's card.
2. A panel shows the exact file Lullshift will change and the events it will listen to.
3. After **Continue**, your system shows a second confirmation dialog. The file is changed only if you confirm there too.
4. Lullshift saves a backup of the file, then adds its hooks. Your own hooks and settings stay. The file's spacing and formatting may change.

Connecting twice changes nothing. If the file isn't valid, Lullshift leaves it untouched and says so.

### Files Lullshift writes

| Agent | File | Backups |
|---|---|---|
| Claude Code | `~/.claude/settings.json`, or `$CLAUDE_CONFIG_DIR/settings.json` if that variable was set when Lullshift started | `<app data>/adapters/claude-code/backups/` |
| Codex | `$CODEX_HOME/hooks.json`, else `~/.codex/hooks.json` | `<app data>/adapters/codex/backups/` |
| Cursor | `~/.cursor/hooks.json` | `<app data>/adapters/cursor/backups/` |

Lullshift reads Codex's `config.toml` only to see whether hooks are turned off (`[features] hooks = false`). It never writes to it. Backups are never deleted automatically. The app data folder is listed in [Privacy](privacy.md#where-your-data-lives).

Each hook runs the Lullshift executable with `hook <agent> --data-dir <app data>`. The hook writes nothing back to the agent and always exits successfully, so it can't change what the agent does.

### What reaches Lullshift

Only the session ID, the event type and the time. Never your prompts, code, file paths, the agent's replies or your email address.

One exception, for Claude Code only, and off by default: **Task context** in Settings. When you turn it on, Lullshift also keeps the first prompt of each session and what Claude Code asks you, so you can pick up where you left off. The consent dialog lists exactly what is kept. Strings that look like secrets are hidden, but that is a safety net, not a guarantee. You can clear captured text at any time.

## What each agent can report

Agents report different things. When an agent doesn't report something, Lullshift shows it as unknown rather than guessing. Full details: [docs/ADAPTER_CAPABILITIES.md](../ADAPTER_CAPABILITIES.md).

| | Claude Code | Codex | Cursor |
|---|---|---|---|
| Session starts and ends | Yes | Yes. Session end can also fire after 30 minutes idle, with no reason | Yes |
| Working | Yes | Yes | Yes |
| Needs you | Yes: permission prompts, questions and idle prompts | Partly: only when it asks to approve a command | No. Cursor never shows as needing you |
| Turn ended | Yes | Yes, including when you stop a turn | Yes |
| Task finished | No agent reports this. A turn ending is not the task finishing | | |

### Claude Code

- Folders Claude Code hasn't trusted yet send no events.
- After you grant a permission, the session stays on "Needs you" until the next step finishes.
- If you interrupt a turn (Esc), Claude Code sends nothing. The session stays on "Working" and is marked as out of date after 10 minutes.
- A project setting with `disableAllHooks` turns the hooks off for that project.

### Codex: trust the hooks in `/hooks`

Codex asks you to review and trust new or changed hooks before they run. After you connect, open `/hooks` in Codex and trust Lullshift's hooks. Until you do, Codex sends no events. Do this again after a **Repair connection**. Lullshift can't see whether you have approved them.

If Codex's `config.toml` turns hooks off, the card shows **Hooks turned off**.

### Cursor: no "needs you"

Cursor has no hook that fires when it waits for you, so you won't get a "needs you" notification for Cursor. Lullshift registers only hooks that can't block Cursor and sets a short timeout.

Cursor also loads Claude Code hooks from `~/.claude/settings.json`. If both are connected, Lullshift recognises Cursor's events there and ignores them, so Cursor sessions are not shown twice.

## Card states

| State | Meaning |
|---|---|
| Connected | Lullshift's hooks are in place |
| Not connected | No Lullshift hooks in the file |
| Needs repair | The hooks are from an older version or point at an old location. Choose **Repair connection** |
| Partly connected | Some hooks are missing. Choose **Repair connection** |
| Can't connect | The file isn't valid, so Lullshift won't change it. Fix the file first |
| Hooks turned off | Codex's `config.toml` has hooks turned off |

## Disconnecting

Choose **Disconnect** on the agent's card, then **Remove hooks**.

- If nothing else changed the file since you connected, it is restored exactly as it was (or removed, if it didn't exist before).
- If you or another tool changed it, only Lullshift's entries are removed and your changes are kept.
- Sessions from that agent are marked "Not tracked: disconnected".

Disconnect each agent before you uninstall or move Lullshift. Otherwise the agent keeps trying to run a program that is no longer there. See [Uninstalling](uninstalling.md).
