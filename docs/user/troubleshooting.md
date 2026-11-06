# Troubleshooting

## Agents

**A session doesn't appear in Sessions.**

- Check **Integrations**. The card should say **Connected**. If it says **Needs repair** or **Partly connected**, choose **Repair connection**.
- Claude Code: folders it hasn't trusted yet send no events. A project setting with `disableAllHooks` also turns hooks off. Sessions started before you connected begin reporting once Claude Code reloads its settings.
- Codex: open `/hooks` in Codex and trust Lullshift's hooks. Do this again after every repair. If the card says **Hooks turned off**, turn hooks back on in Codex's `config.toml`.
- If you set `CLAUDE_CONFIG_DIR`, check that the file named in the connect panel is the one your Claude Code uses.
- Lullshift must be running (in the tray) to receive events.

**The card says "Can't connect".** Your agent's settings or hooks file isn't valid JSON, or has a format Lullshift doesn't know. Lullshift won't change it. Fix the file, then check again.

**A session stays on "Working".** Some agents send nothing when you interrupt a turn. After 10 minutes with no update, the session is marked as out of date and shows its last known state.

**No "needs you" notification.**

- In **Settings**, check that System notifications are allowed, and check your quiet hours.
- Cursor never reports that it is waiting for you. Codex reports only command approvals, and OpenCode only permission requests. An OpenCode card that says Unknown has not received an event yet: restart OpenCode, and note that it works only with OpenCode 2.x. See [Connecting agents](connecting-agents.md#what-each-agent-can-report).

**Lullshift was moved or reinstalled and an agent shows errors about a missing program.** Open **Integrations** and choose **Repair connection** for that agent. If Lullshift is gone, see [Uninstalling](uninstalling.md#if-you-already-uninstalled).

## Activities

**An activity says "Didn't start".** Nothing was lost. Try again, or pick another one.

**"This pack's data doesn't match its engine", or "Engine missing" in Manage.** The content pack's data isn't valid for the engine bundled with this version of Lullshift. Update the pack from its marketplace, or uninstall it.

**"Damaged, reinstall it" in Manage, or "This pack's files changed on disk".** A file of the installed activity no longer matches what was installed (Lullshift checks every time it reads one), so it turned the activity off. Uninstall it and install it again.

## Marketplaces

| Message | What to do |
|---|---|
| Git isn't installed, or isn't on your PATH | Install Git, then restart Lullshift |
| Can't reach the repository | Check your network connection |
| Access was denied | For a private repository, set up Git credentials on this computer first (credential manager, SSH key or `gh auth login`) |
| Code activities from third-party marketplaces can't be installed on this system yet | Expected in this release. Content packs can be installed |
| This marketplace hasn't been fetched yet. Refresh it first | Choose **Refresh** on the marketplace, then browse again |
| This review has expired or was already used | Reviews last 10 minutes and work once. Open the review again |

A repository that moved: Lullshift does not let Git follow redirects, but it shows the new address. On the same server, choose **Use the new address** (or **Change address** on the marketplace). On a different server, check the address and add it yourself. A private address (an IP address, `localhost` or a local network name) needs the **This marketplace is on my own computer or network** box ticked.

## Companion

| Message | What to do |
|---|---|
| Keychain unavailable | Your system keychain can't be reached. On Linux, install and unlock a Secret Service such as GNOME Keyring or KWallet |
| The provider rejected the key | Check the key with your provider, then **Replace key** |
| This month's token budget for this provider is used up | Raise the budget in Companion settings, or wait for next month |
| The context changed since you previewed it | Review the new preview, then send again |

## Notifications or the tray don't appear on Linux

The tray icon needs an AppIndicator-compatible tray. Some desktops need an extension for this.

## Reporting a problem

Open an issue on [GitHub](https://github.com/lullshift/lullshift-app/issues). Say which system and agent you use. Don't paste prompts, keys or private file paths. For security problems, follow [SECURITY.md](../../SECURITY.md) instead.
