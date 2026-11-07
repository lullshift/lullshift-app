# Privacy

Lullshift has no accounts, no Lullshift servers and no telemetry. Your data stays on your computer.

## What stays on your computer

- Agent sessions: the agent's name, session ID, event types, times and states.
- Your notes, intents, decisions, checklists and Next Move items.
- Captured task context, only if you turned on **Task context** for a tool (Claude Code, Codex, Cursor or OpenCode; each is off by default).
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
| You press **Test connection**, **Send**, **Retry** or **Approve** (for a lookup) in Companion | Your provider: `api.anthropic.com`, `api.openai.com`, or the base URL you entered (plain http only for your own computer) |
| You press **Load model list from models.dev** or **Refresh** in Companion settings | `models.dev`: one request for its public model list (`https://models.dev/api.json`). No keys, messages or other data are sent; the list is kept in memory until you quit. Like any web request, models.dev (and any proxy in between) sees your IP address and the time, and the request says it comes from Lullshift (user agent `Lullshift-Companion`) |

During Git actions, your own Git configuration applies: credential helpers, proxies and SSH settings. Lullshift doesn't change them.

If Git isn't installed, Lullshift's built-in Git contacts the same hosts, at the same moments, for public https marketplaces only. It reads no Git configuration, uses no proxy and sends no credentials; a repository that asks for a sign-in is left for installed Git.

**Updates:** Lullshift does not check for updates on its own. Any update check runs only when you ask for it.

**Outside Lullshift's control:**

- On Windows, the installer may download the WebView2 runtime from Microsoft if it is missing.
- The WebView2 runtime and your operating system have their own services, such as SmartScreen, component updates and certificate revocation checks.

### Check it yourself

Run Lullshift with no marketplaces added and no Companion provider set up, and watch your firewall log. You should see no connections from Lullshift.

## Companion lookups

If you turn on **Let the Companion look things up** (off by default), the provider you chose can ask for three read-only lookups: your sessions' tool, state and time in state; one suggested activity; or one installed activity's help. You see the exact text of every lookup and press Approve or Decline before anything is sent. The lookups never send keys, file paths, Git addresses, session ids, other conversations or notes. A session label is included only for a tool where you turned on task text, and then it may be the name you gave the session, the agent's title for it or, if there is neither, the first line of the captured task prompt (cut short); you see it before you approve. Approved lookups are stored with the conversation on your computer, together with the host they went to, and are deleted with it. The last context choice you used in Companion is stored locally so the next message can start with it; it holds ids only, not the text.

## Secrets

- **Companion API keys** go straight to your system keychain: Windows Credential Manager, macOS Keychain, or the Secret Service on Linux. They are never written to Lullshift's database, logs or screens.
- **Git credentials** stay with your Git tools. Lullshift never asks for, stores or sends them.

## Agents

Agent hooks send Lullshift only the session ID, event type and time. Prompts, code, file paths and replies are never sent, unless you turn on Task context for a tool. For Claude Code it keeps the first prompt and what Claude Code asks you; for Codex, Cursor and OpenCode it keeps only the first prompt of each session. Each tool has its own switch on its card in Integrations, and nothing is kept for a tool until you turn its switch on. See [Connecting agents](connecting-agents.md).

**Import sessions** is the one exception you start yourself. Only when you open it, Lullshift reads the start and the end of each agent's own session logs on your computer (never the whole file) to list sessions with their project folder, title and a guess at their state. Nothing is uploaded. The titles are shown in the dialog only; Lullshift keeps an imported session's title only if Task context is on for that tool. See [Importing sessions already running](connecting-agents.md#importing-sessions-already-running).

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
