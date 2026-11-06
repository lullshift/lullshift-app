# Uninstalling

## Before you uninstall

1. **Disconnect each agent in Integrations.** Open **Integrations** and choose **Disconnect** on every connected agent (Claude Code, Codex, Cursor, and OpenCode if you connected it). This removes Lullshift's hooks and restores your settings files where it can. If you skip this, an agent may keep trying to run a Lullshift program that is no longer there.
2. **Remove Companion providers** you added (Companion, Settings). This deletes their API keys from your system keychain. Uninstalling the app does not remove keychain entries.
3. **Export anything you want to keep.** Each session's details panel can export Markdown or JSON.

Then quit Lullshift from the tray menu and uninstall it the usual way for your system.

### What the uninstaller does for you

- **Windows** (Settings > Apps) and the **Linux `.deb`** (`sudo apt remove lullshift`): the uninstaller also removes Lullshift's agent hooks, the same way Disconnect does, and keeps your own hooks. An in-place update or a reinstall over an existing install does not remove them. If you cancel a Windows uninstall at the "Lullshift is running" prompt, your agents may already be disconnected; reconnect them in Integrations.
- **macOS** (drag to the Trash) and the **AppImage**: nothing runs on removal, so Disconnect first. An AppImage's hooks point at the AppImage file itself, so if you move it, choose **Repair connection** in Integrations.

Disconnecting first is still the safest route on every system.

## Removing your data

The uninstaller may leave your data folder in place. To remove everything, delete the app data folder after uninstalling:

| System | Folder |
|---|---|
| Windows | `%APPDATA%\app.lullshift.desktop` |
| macOS | `~/Library/Application Support/app.lullshift.desktop` |
| Linux | `~/.local/share/app.lullshift.desktop` |

This deletes sessions, notes, Companion conversations, activity progress, marketplace caches, installed activities, and the backups of your agent settings files. Keep a copy of `adapters/<agent>/backups/` first if you might need the original settings files.

Companion keys are stored under the keychain service name `app.lullshift.companion`. If you uninstalled without removing providers, delete entries with that name in your keychain tool.

## If you already uninstalled

If an agent now reports errors about a missing program, remove Lullshift's hooks by hand. Back up the file first.

| Agent | File | Remove |
|---|---|---|
| Claude Code | `~/.claude/settings.json` (or `$CLAUDE_CONFIG_DIR/settings.json`) | Hook handlers whose `args` start with `"hook", "claude-code"` |
| Codex | `$CODEX_HOME/hooks.json`, else `~/.codex/hooks.json` | Hook commands containing ` hook codex --data-dir ` |
| Cursor | `~/.cursor/hooks.json` | Hook commands containing ` hook cursor --data-dir ` |
| OpenCode | `~/.config/opencode/plugins/lullshift-observer.js` (or under `$XDG_CONFIG_HOME/opencode` or `$OPENCODE_CONFIG_DIR`) | Delete this file if its first line starts with `// Lullshift-owned file` |

Leave every other entry as it is.

Alternatively, reinstall Lullshift, choose **Disconnect** for each agent, and then uninstall again.
