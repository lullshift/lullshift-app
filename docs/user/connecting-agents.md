# Connecting agents

Lullshift follows Claude Code, Codex and Cursor through each agent's own documented hooks, and, as an experiment, OpenCode 2.x through a small plugin. It only watches. It never answers prompts, approves permissions or sends commands to an agent.

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

| OpenCode (experimental) | `~/.config/opencode/plugins/lullshift-observer.js` (also on Windows), or under `$XDG_CONFIG_HOME/opencode` or `$OPENCODE_CONFIG_DIR` if one was set when Lullshift started | none: this is a new file that Lullshift owns |

Lullshift reads Codex's `config.toml` only to see whether hooks are turned off (`[features] hooks = false`). It never writes to it. Backups are never deleted automatically. The app data folder is listed in [Privacy](privacy.md#where-your-data-lives).

Each hook runs the Lullshift executable with `hook <agent> --data-dir <app data>`. The hook writes nothing back to the agent and always exits successfully, so it can't change what the agent does.

### What reaches Lullshift

Only the session ID, the event type and the time. Never your prompts, code, file paths, the agent's replies or your email address.

One exception, off by default and separate for each tool: **Task context** on the Claude Code, Codex, Cursor and OpenCode cards in Integrations. When you turn it on for a tool, Lullshift also keeps the first prompt of each session (for Claude Code also what it asks you), so you can pick up where you left off. For Codex, Cursor and OpenCode nothing else is kept: not their replies, tool inputs, commands, files or your email. The consent dialog lists exactly what is kept. Strings that look like secrets are hidden, but that is a safety net, not a guarantee. You can clear captured text at any time.

## What each agent can report

Agents report different things. When an agent doesn't report something, Lullshift shows it as unknown rather than guessing. Full details: [docs/ADAPTER_CAPABILITIES.md](../ADAPTER_CAPABILITIES.md).

| | Claude Code | Codex | Cursor | OpenCode (experimental) |
|---|---|---|---|---|
| Session starts and ends | Yes | Yes. Session end can also fire after 30 minutes idle, with no reason | Yes | Start yes. End only when the session is deleted |
| Working | Yes | Yes | Yes | Yes |
| Needs you | Yes: permission prompts, questions and idle prompts | Partly: only when it asks to approve a command | No. Cursor never shows as needing you | Partly: only when it asks for a permission |
| Turn ended | Yes | Yes, including when you stop a turn | Yes | Yes, including when you stop a turn; a failed turn shows as failed |
| Task finished | No agent reports this. A turn ending is not the task finishing | | | |

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

### OpenCode: experimental, 2.x only

- **It only works with OpenCode 2.x.** OpenCode has two plugin systems, and the file Lullshift writes is for version 2. Lullshift can't tell which version you run. On OpenCode 1.x it does nothing, and OpenCode may show a plugin error until you disconnect.
- Lullshift writes one file, `lullshift-observer.js`, into OpenCode's global plugin folder. It changes none of your OpenCode settings and never replaces a file with that name that it didn't write.
- The plugin only watches. It sends the session ID and the kind of event (session started, working, asked for a permission, turn ended or failed, session deleted), never your messages, files or tool inputs, and it can't block or change OpenCode. If Lullshift isn't running, it does nothing.
- OpenCode loads plugins when it starts. Restart OpenCode after connecting. The card says **Unknown** until the first OpenCode event actually reaches Lullshift, then **Connected**.
- Sessions that OpenCode starts for helper agents are not shown as separate sessions.
- This rests on OpenCode 2.x's source, not on its written documentation, so a later OpenCode 2.x update may break it. Connected tells you events are arriving; if they stop, the card does not notice.

## Card states

| State | Meaning |
|---|---|
| Connected | Lullshift's hooks are in place. For OpenCode: events have arrived |
| Unknown | OpenCode only: the plugin file is in place, but no event has arrived yet |
| Not connected | No Lullshift hooks in the file |
| Needs repair | The hooks are from an older version or point at an old location. Choose **Repair connection** |
| Partly connected | Some hooks are missing. Choose **Repair connection** |
| Can't connect | The file isn't valid, so Lullshift won't change it. Fix the file first |
| Hooks turned off | Codex's `config.toml` has hooks turned off |

## Disconnecting

Choose **Disconnect** on the agent's card, then **Remove hooks** (**Remove plugin** for OpenCode, which deletes `lullshift-observer.js`; restart OpenCode afterwards).

- If nothing else changed the file since you connected, it is restored exactly as it was (or removed, if it didn't exist before).
- If you or another tool changed it, only Lullshift's entries are removed and your changes are kept.
- Sessions from that agent are marked "Not tracked: disconnected".

Disconnect each agent before you uninstall or move Lullshift. Otherwise the agent keeps trying to run a program that is no longer there. See [Uninstalling](uninstalling.md).
