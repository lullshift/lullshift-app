# Getting started

## Install

Lullshift targets Windows, macOS and Linux.

- **Release builds** are published on the project's [GitHub Releases](https://github.com/lullshift/lullshift-app/releases) page. Each release's notes say which platforms were tested and whether the installers are code-signed.
- If an installer is not code-signed, Windows SmartScreen or macOS Gatekeeper warns you before it runs. Check that you downloaded it from the Releases page above before you continue.
- **From source:** follow [docs/DEVELOPMENT.md](../DEVELOPMENT.md). You need Node.js, pnpm, Rust and the Tauri system prerequisites. `pnpm desktop:dev` starts the app.

## First launch

- There is no sign-in, registration or licence key. Everything works as soon as the app opens.
- Lullshift offers to connect Claude Code and suggests a first activity. Both are optional. You can skip them and use the app straight away, and connect agents later in **Integrations**.
- Nothing is changed in your agent settings until you confirm in the app and once more in a system dialog.
- Lullshift lives in the system tray. Closing the window keeps it running so it can still tell you when an agent needs you. To stop it, choose **Quit Lullshift** from the tray menu.

The sidebar has five views:

| View | What it is for |
|---|---|
| Sessions | Your agent sessions and their state, such as Working, Needs you, Idle or Ended. Unknown or out-of-date state is labelled as such, never guessed |
| Activities | Games and breaks (Library), turning activities on and off (Manage), and Marketplaces |
| Companion | An optional chat with an AI provider using your own key |
| Integrations | Connecting and disconnecting Claude Code, Codex and Cursor (and OpenCode 2.x, experimental) |
| Settings | Start with your computer, appearance, notifications and task context |

## Offline use

Lullshift works fully offline. Sessions, notes, activity progress and settings are stored on your computer. Bundled activities are part of the app and need no download.

The network is used only when you ask for it. See [Privacy](privacy.md) for the full list.

## Notifications

Open **Settings** and choose **Allow** next to System notifications. Without this, Lullshift can still show state in the window, but it can't alert you when a session needs you. You can choose what you are told about and set quiet hours. During quiet hours you can still be told when a session needs you.

## Next steps

- [Connect your agents](connecting-agents.md).
- [Try an activity](activities.md).
