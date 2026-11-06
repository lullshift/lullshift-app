# Security policy

## Reporting a vulnerability

Please report security problems privately through GitHub's private vulnerability reporting:

1. Go to the repository's **Security** tab on GitHub.
2. Choose **Report a vulnerability**.
3. Describe the problem, the affected version or commit, your operating system and the steps to reproduce it.

Please don't open a public issue, discussion or pull request for a vulnerability until it has been fixed.

## What is in scope

Lullshift treats these as security boundaries. Reports about them are especially welcome:

- **The activity sandbox:** an activity reaching the network, files, clipboard, Tauri IPC, another activity's data or the app window (ADR-012).
- **Marketplaces:** a Git repository or index that can escape the cache, run code during fetch or install, bypass commit pinning or digest checks, or install a third-party code activity (ADR-015).
- **Agent hooks:** an install or uninstall that loses or corrupts a user's agent settings, or a hook that changes what the agent does (ADR-010, ADR-021).
- **Local IPC:** events accepted without the per-install token (ADR-009).
- **Companion:** a provider key leaving the OS keychain, or context sent without the user's confirmed preview (ADR-020).
- **Privacy:** any network use that the user didn't start, or task data written to logs.

## Known limits

Some limits are already documented and are not new reports: the remaining WebRTC gaps in the sandbox (ADR-012, "WebRTC is off in the webview"), and the text-only check for local network addresses in marketplace URLs (ADR-015). The remaining WebRTC gaps are local overrides of the webview's arguments, and macOS and Linux not yet tested. Ways to exploit them beyond what is described are still welcome. A way for an activity to use WebRTC despite these settings is a new report.
