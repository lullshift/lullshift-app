# Privacy

Lullshift has no accounts, no Lullshift servers and no telemetry. Your data stays on your computer.

## What stays on your computer

- Agent sessions: the agent's name, session ID, event types, times and states.
- Your notes, intents, decisions, checklists and Next Move items.
- Captured task context, only if you turned on **Task context** for a tool (Claude Code, Codex or Cursor; each is off by default).
- Activity progress, play time and settings.
- Companion conversations, provider settings and monthly usage counts.

None of this is uploaded. Lullshift never reads your clipboard in the background, records your screen, browses your repositories or sends your source code anywhere.

You can delete a session from its details panel, export it as Markdown or JSON, clear all captured text in **Settings**, and choose how long agent events are kept (30 days by default).

## The only network use

Lullshift connects to the network only when you do something that needs it:

| When | What it contacts |
|---|---|
| You add or refresh a marketplace | The Git host you entered (https or ssh). `owner/repo` means github.com. Addresses on `localhost` or IP addresses are refused |
| You open an install review for an entry that lives in another repository | That repository's Git host |
| You press **Test connection**, **Send** or **Retry** in Companion | Your provider: `api.anthropic.com`, `api.openai.com`, or the base URL you entered (plain http only for your own computer) |

During Git actions, your own Git configuration applies: credential helpers, proxies and SSH settings. Lullshift doesn't change them.

**Updates:** Lullshift does not check for updates on its own. Any update check runs only when you ask for it.

**Outside Lullshift's control:**

- On Windows, the installer may download the WebView2 runtime from Microsoft if it is missing.
- The WebView2 runtime and your operating system have their own services, such as SmartScreen, component updates and certificate revocation checks.

### Check it yourself

Run Lullshift with no marketplaces added and no Companion provider set up, and watch your firewall log. You should see no connections from Lullshift.

## Secrets

- **Companion API keys** go straight to your system keychain: Windows Credential Manager, macOS Keychain, or the Secret Service on Linux. They are never written to Lullshift's database, logs or screens.
- **Git credentials** stay with your Git tools. Lullshift never asks for, stores or sends them.

## Agents

Agent hooks send Lullshift only the session ID, event type and time. Prompts, code, file paths and replies are never sent, unless you turn on Task context for a tool. For Claude Code it keeps the first prompt and what Claude Code asks you; for Codex and Cursor it keeps only the first prompt of each session. Each tool has its own switch on its card in Integrations, and nothing is kept for a tool until you turn its switch on. See [Connecting agents](connecting-agents.md).

## Where your data lives

| System | App data folder |
|---|---|
| Windows | `%APPDATA%\app.lullshift.desktop` |
| macOS | `~/Library/Application Support/app.lullshift.desktop` |
| Linux | `~/.local/share/app.lullshift.desktop` |

Inside it:

- `lullshift.db`: the local SQLite database, with backups named `lullshift.db.v<N>-<time>.bak` made before upgrades;
- `ipc/`: the local connection details and a per-install token that agent hooks use to reach the app;
- `adapters/<agent>/backups/`: copies of your agent settings files from before Lullshift changed them;
- `marketplaces/` and `installed/`: marketplace caches and installed activities.

## No monitoring of people

Lullshift has no team, employer or activity reporting features. Play time and stats are shown only to you, on your computer.
