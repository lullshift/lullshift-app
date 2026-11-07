# Release notes

## Lullshift 0.1.0 (draft, not released)

> Draft for the product owner. Nothing is published until the owner approves the release (S11-T05). The version number is the owner's choice.

Lullshift watches your coding agents and gives your attention somewhere calm to go while they work. It is open source (Apache-2.0), needs no account, and works offline.

### What's in it

- **Agent sessions.** Follows Claude Code, Codex and Cursor through their own documented hooks (and OpenCode 2.x through an experimental plugin): what is working, what needs you, what has finished. Unknown or stale status is shown as unknown, never guessed. What each agent can and can't report is listed in [ADAPTER_CAPABILITIES.md](ADAPTER_CAPABILITIES.md).
- **Import sessions.** Sessions that were already running can be brought in from the agents' own logs on your computer (Claude Code, Cursor, Codex; OpenCode 2.x experimental). Only when you open it, Lullshift reads the start and end of each log and lists sessions newest first with their project folder, title and a guessed state; you filter, tick and import. Imported sessions are marked "From log", never alert you, and normal tracking takes over at the agent's next hook event. Titles are kept only with Task context on.
- **Sessions that end silently, and catching up.** When an agent's program exits without saying goodbye, its session now reads "Ended (process gone)" (Lullshift only checks whether the program still exists; the agent's next update reopens it). In Settings you can turn on, off by default, "Catch up from agent logs at startup": it reads the start and end of each agent's log at start-up, only to update sessions Lullshift already tracks, shown as a "From log" guess.
- **Connecting an agent is your choice.** Lullshift asks first, names the exact file it changes, backs it up, and Disconnect restores it. Lullshift only observes: its hooks never block or answer an agent.
- **Activities.** 20 original games and 73 breaks and content packs, all playable offline with the keyboard, paused and saved whenever an agent needs you. Grid puzzles have a daily puzzle. Content packs may include checked still pictures.
- **Git marketplaces.** Add any public or private Git repository with a `.lullshift/marketplace.json` index. Installs are pinned to a commit and only update when you choose. Lullshift stores no Git credentials. Without Git installed, a built-in Git reads public https marketplaces; private and ssh marketplaces need Git installed. Third-party marketplaces can supply content packs and code activities. A code activity from a third-party marketplace (a "third-party program") is installed only after you read a risk warning and tick "I understand and want to install this program", at each install and at each update that changes its code. It runs in the same sandbox as the built-in activities. The warning says what Lullshift has verified on your platform: on Windows the sandbox checks passed on a test computer but the WebRTC block can't be read back; on macOS and Linux the WebRTC block is in place but the full sandbox checks haven't been run yet.
- **Companion (optional).** A separate conversation space using your own AI provider key (Anthropic, OpenAI or an OpenAI-compatible server). Keys stay in your system keychain. Replies stream as they are written. You choose exactly what context is shared, with a preview. If you turn on "Let the Companion look things up" (off by default), it can ask for three read-only lookups: a session overview, a break suggestion and activity help. You approve each one before anything is sent. It can't act, run tools or reach your agents. Companion settings live in a Companion section of Settings. Each provider can have several models with a default, and you can pick another model for a single message; every answer says which model wrote it. A model picker lets you browse and filter the models.dev list (tools, reasoning, age, price, context size) or type any name; the first time you open it after starting Lullshift it sends one request to models.dev, with no keys or messages, and the list is kept in memory only. The list may be out of date, so Test connection is the real check. Replies show code blocks with highlighting and a Copy button, and links open in your browser only after you confirm the exact address. Enter sends and Shift+Enter starts a new line. A reply keeps being written when you leave the conversation (another conversation or another part of Lullshift): the conversation list shows "Writing…" or "New reply", the Companion item gets a badge, and a small notice (the conversation's name only, never the reply) tells you when it is done, fails or needs your approval.
- **Creator tools.** An activity SDK, three game kits, ten engines for code-free content packs and the `lullshift` CLI to create, validate, simulate and pack activities. See the [creator guide](creators/README.md).

### Privacy

No telemetry, no accounts, no Lullshift servers. Lullshift reads an agent's own session logs only when you open Import sessions or, if you turn it on in Settings (off by default), at start-up to catch up, and only the start and end of each. It also asks the system whether the program behind each session still exists, without touching it. Lullshift only goes online when you ask it to: fetching a marketplace you added, sending a Companion message to the provider you set up, or opening the Companion model picker (which loads the model list from models.dev once per run, or again when you press Refresh). WebRTC is turned off in the app, so activities can't open peer-to-peer connections (tested on Windows; macOS and Linux checks are still to run). Details in the [privacy page](user/privacy.md).

### Installing

Installers are not code-signed yet:

- **Windows:** SmartScreen may say the publisher is unknown. Choose **More info**, then **Run anyway**.
- **macOS:** Gatekeeper may block the first launch. Right-click the app, choose **Open**, then confirm.
- **Linux:** `.deb` and AppImage.

Check your download against `SHA256SUMS` and its build provenance (see [RELEASING.md](RELEASING.md)).

### Known limits

- App updates: there is no in-app updater yet. Download new versions from the Releases page.
- macOS and the AppImage have no uninstaller step: choose **Disconnect** for each agent before removing the app. Windows and the `.deb` remove Lullshift's hooks for you (a Windows reinstall over an existing install puts them back for the agents you had connected, once it has finished). AppImage hooks point at the AppImage file, so keep it in one place; if you move or rename it, choose **Repair connection**.
- Cursor has no hook for "waiting for you", so Cursor sessions never show as needing you.
- OpenCode support is experimental and works only with OpenCode 2.x. It is built on OpenCode's source, not documented event payloads, and shows "Unknown" until the first event arrives. Only permission requests count as "needs you".
- Codex asks you to trust new hooks in `/hooks` before they run.
- "Ended (process gone)" is Lullshift's conclusion from a program no longer existing, not the agent's report. A long-lived host (Cursor's app) keeps its chats looking alive while it runs, and a short-lived launcher can make a live session look ended until its next update. The process lookups on macOS and Linux have been type-checked but not yet run there.
- Sessions that were already open when you connect an agent aren't tracked until they send a hook event. Use **Import sessions** in the Sessions view to bring them in now (their state is a guess until then); in Claude Code, `/hooks` may also pick up the new hooks.
- Windows: don't start Lullshift from a terminal inside a Microsoft Store app, such as the Claude desktop app's terminal. Windows gives such programs a private copy of AppData, and agents run from that app can then read an out-of-date copy of Lullshift's connection files, so their sessions never appear. Start Lullshift from the Start menu or File Explorer instead. Lullshift now warns when it was started that way, and Integrations finds out-of-date copies left in another app's storage and offers **Fix** to remove them (only those two files; nothing is removed without you pressing it). Sessions already open in that app may need a restart afterwards.
- Tested mainly on Windows 11; macOS and Linux have had automated checks only.
- Third-party programs: the full sandbox checks haven't been run on macOS or Linux, and on Windows Lullshift can't read back the setting that blocks WebRTC. Settings in `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` or the WebView2 registry policy can undo that block. The install warning says so; only install programs from people you trust.

### Feedback

- Bugs and content mistakes: [GitHub issues](https://github.com/lullshift/lullshift-app/issues).
- Questions and ideas: [GitHub Discussions](https://github.com/lullshift/lullshift-app/discussions).
- Security problems: privately, as described in [SECURITY.md](../SECURITY.md).
