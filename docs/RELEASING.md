# Releasing Lullshift

How v1 installers are built, what the owner must do by hand, and what is deliberately not done yet. Decision record: ADR-004 (amended 2026-11-05). Hook cleanup on uninstall: ADR-010.

Nothing here runs by itself. There is no push, pull-request, merge or scheduled build, and the workflow never publishes: it creates a **draft** GitHub Release for the owner to review.

## 1. Build a draft release

1. Bump `version` in `apps/desktop/src-tauri/tauri.conf.json` (and the Cargo/package versions) and merge to `main`. No tag is needed; the draft is created for tag `v<version>` and the tag appears only when the owner publishes it.
2. GitHub > Actions > **Release build** > **Run workflow** (`.github/workflows/release.yml`, trigger `workflow_dispatch` only).
3. Three jobs build in parallel, then a fourth creates the draft:

| Platform | Runner           | Output (unsigned)                                                         |
| -------- | ---------------- | ------------------------------------------------------------------------- |
| Windows  | `windows-latest` | NSIS `.exe` setup (per-user install)                                      |
| macOS    | `macos-latest`   | universal `.dmg` (arm64 + x86_64), ad-hoc signed (`signingIdentity: "-"`) |
| Linux    | `ubuntu-22.04`   | `.deb` and `.AppImage` (22.04 keeps the glibc baseline low)               |

4. The draft holds only installers, `SHA256SUMS` and, once the updater is enabled, `.sig` files. Source code and user data are never uploaded. A build-provenance attestation (`actions/attest-build-provenance`, available on the free plan for public repositories) covers every file. Anyone can check a download with `gh attestation verify <file> -R lullshift/lullshift-app --signer-workflow lullshift/lullshift-app/.github/workflows/release.yml --source-ref refs/heads/main` and compare it with `SHA256SUMS`. The `--signer-workflow` and `--source-ref` flags matter: without them, a file built by any workflow or branch of the repository would also verify. The draft job runs only for `main`.
5. Review the draft, install it on each OS (see `docs/MANUAL_TESTS.md`), then publish by hand. Publishing and tagging stay human decisions.

Cost: expect roughly 30 to 60 minutes of runner time per OS. Standard runners are free and unlimited for public repositories; on a private repository macOS minutes count 10 times against the free quota, which is why the workflow is manual only.

### Actions and tools (versions checked 2026-11-06)

All actions are pinned to full commit SHAs with the version in a comment: `actions/checkout` v7.0.1, `actions/setup-node` v7.1.0, `actions/upload-artifact` v7.0.2, `actions/download-artifact` v8.0.2, `actions/attest-build-provenance` v4.2.2. `tauri-apps/tauri-action` (action-v1.0.0) was evaluated and **not used**: calling `pnpm tauri build` directly keeps the third-party surface small, lets the build jobs run with `contents: read` only, and keeps the release token out of the build step. Only the last job has `contents: write`, `id-token: write` and `attestations: write`. Tauri CLI 2.12.x (`@tauri-apps/cli`), Rust 1.99.1 from `rust-toolchain.toml`, Node 24, pnpm 12.6.0 from `package.json`. Update the pins deliberately (look up the SHA of the new tag), never to a moving tag.

The workflow was linted with `actionlint` 1.7.7 and its structure is guarded by `apps/desktop/src-tauri/tests/release_config.rs`. It has **not been run**: the first run needs the owner. Details not verifiable without a run: the `--target universal-apple-darwin --bundles dmg` combination, the Linux package list, and the bundle output folders the "Collect installers" step searches.

## 2. Unsigned installers: what users see

No Windows or Apple certificates exist yet (S11-T01). Say so in the release notes.

- **Windows (SmartScreen):** "Windows protected your PC". Choose **More info**, then **Run anyway**. A code-signing certificate (OV/EV or Azure Trusted Signing) removes it; add it later under `bundle.windows`.
- **macOS (Gatekeeper):** the app is ad-hoc signed, not notarized. The first open is blocked. Open **System Settings > Privacy & Security** and choose **Open Anyway**; if macOS still says the app is damaged, run `xattr -dr com.apple.quarantine /Applications/Lullshift.app`. A paid Apple Developer ID plus notarization removes this.
- **Linux:** no prompt. `chmod +x` the AppImage; install the `.deb` with `sudo apt install ./<file>.deb`.

## 3. Updater: off until the owner supplies a key

Tauri's updater verifies every update against a minisign public key, so it cannot be enabled without one. Today:

- `tauri-plugin-updater` is **not** a dependency and is **not** registered; `bundle.createUpdaterArtifacts` is `false`; `plugins.updater` is absent. There is therefore no update check and no network use. `release_config.rs` fails if any of these changes without a `pubkey`, or if a check-on-start or background setting appears.
- The app has no "Check for updates" action yet. Enabling the updater needs UI work in `apps/desktop/src` (owner follow-up, not part of S11-T01).

### Owner steps to enable it

1. Generate the key pair **on your own machine, outside the repository**: `pnpm --filter @lullshift/desktop tauri signer generate -w <path outside the repo>/lullshift-updater.key` (choose a password).
2. Back up the private key and the password in a password manager. If the key is lost, installed copies cannot be updated and users must reinstall.
3. GitHub > Settings > Environments: create an environment `release`, limited to the `main` branch with you as required reviewer. Add `TAURI_SIGNING_PRIVATE_KEY` (the key file content) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` as secrets of that environment. Then give the build jobs `environment: release` and pass both secrets as `env` on the **Build installers** step only (the workflow deliberately does not read them until the updater is enabled). Never commit the key or put it in `.env`.
4. Put the **public** key (the `.key.pub` content, not a path) in `tauri.conf.json`:

```json
"bundle": { "createUpdaterArtifacts": true },
"plugins": { "updater": {
  "pubkey": "<public key content>",
  "endpoints": ["https://github.com/lullshift/lullshift-app/releases/latest/download/latest.json"]
} }
```

5. Add `tauri-plugin-updater` (desktop only), register it in `lib.rs`, grant the updater permissions to the `main` window only, and add a manual **Check for updates** button. **Never call `check()` on startup, on a timer or in the background** (PRD: network use is user-initiated). Show version and source and wait for the user to confirm the install.
6. `latest.json` must be generated from the `.sig` files and attached to the release (the workflow collects the `.sig` files; assembling `latest.json` is part of that follow-up).
7. Publish the draft, then run S11-A02 (signed update validation) from an older build.

**Network endpoint to list in user docs:** `https://github.com/lullshift/lullshift-app/releases/latest/download/latest.json` (HTTPS GET, only after the user presses Check for updates), plus the installer download from the same release.

### Windows WebView2 install mode

`bundle.windows.webviewInstallMode` is `embedBootstrapper`.

| Mode                                   | Installer size | Network at install                                                                              |
| -------------------------------------- | -------------- | ----------------------------------------------------------------------------------------------- |
| `downloadBootstrapper` (Tauri default) | smallest       | always downloads the bootstrapper from Microsoft                                                |
| **`embedBootstrapper` (chosen)**       | about +1.8 MB  | none if WebView2 exists; otherwise the bundled bootstrapper downloads the runtime from Microsoft |
| `offlineInstaller`                     | about +127 MB  | none                                                                                            |

Reason: Windows 11 and current Windows 10 already include WebView2, so `embedBootstrapper` avoids a needless download without a 127 MB installer. Size figures are from the Tauri docs and not re-measured. Owners who need an air-gapped installer can switch to `offlineInstaller`.

## 4. Uninstall and agent hooks (S11-T06)

An uninstall must not leave Claude Code, Codex or Cursor calling a missing executable. `lullshift-desktop --uninstall-hooks` (no UI, no output, always exit 0, idempotent) runs the same logic as **Disconnect** for all three agents, on the settings file recorded at install time (`lullshift.db` in the app data folder). The original file comes back byte for byte when nothing else changed; otherwise only Lullshift's own entries are removed and the user's other hooks and settings stay. A file that doesn't parse is left untouched. It never creates a database or a settings file. An optional absolute `--data-dir` overrides the app data folder.

| Platform       | How cleanup happens                                                                                                                                                                                                                                                                                                                                            |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Windows (NSIS) | `NSIS_HOOK_PREUNINSTALL` in `apps/desktop/src-tauri/windows/hooks.nsh` runs the flag, unless the uninstaller runs in update mode (`/UPDATE`), so an in-place update keeps working hooks. The install mode is `currentUser`, so this is the account that connected the agents. If a per-machine mode is ever offered, revisit: the hook would act on the elevated account. |
| Linux `.deb`   | `prerm` (`apps/desktop/src-tauri/linux/prerm`, set as `bundle.linux.deb.preRemoveScript`) runs the flag for each normal user (uid 1000 and up, real login shell) with `runuser`, never as root, and only for `remove` (not `upgrade`).                                                                                                                         |
| Linux AppImage | No uninstaller. Use **Disconnect** in the app before deleting the file. An AppImage records a temporary mount path in the hooks, so the app shows "Needs repair" on the next launch; prefer the `.deb` for agent hooks.                                                                                                                                         |
| macOS          | Dragging the app to the Trash runs nothing. Press **Disconnect** for each agent in Lullshift first; the release notes must say so. Manual cleanup is in ADR-010.                                                                                                                                                                                                |

If the app was removed without disconnecting, delete the hook entries whose command path contains `lullshift` from `~/.claude/settings.json`, `~/.codex/hooks.json` and `~/.cursor/hooks.json`.

A reinstall started from a new setup on top of an existing install (not an update) runs the old uninstaller without `/UPDATE`; the hooks are then removed and the user reconnects in the app.

### Known caveats

- **Windows, cancelled uninstall:** the hook cleanup runs before the uninstaller checks whether Lullshift is running. If the user then cancels at the "app is running" prompt, the app stays installed but its agents are disconnected (shown honestly in Integrations; reconnect to restore). Moving the cleanup after the uninstall would lose the backups needed for an exact restore.
- **Linux, per-user settings in shell profiles:** `prerm` runs without the user's shell environment, so a user who sets `CLAUDE_CONFIG_DIR`, `CODEX_HOME` or `XDG_DATA_HOME` in a profile gets the default locations. Nothing is created, but their hooks stay; they should Disconnect before removing the package.
- **Linux, slow homes:** each user's cleanup is limited to 30 seconds so an unreachable home or directory service cannot stall package removal.

### What Claude Code does when the hook executable is missing

From the Claude Code hooks documentation (checked 2026-11-06): a command hook that cannot be started, or exits with a code other than 0 or 2, is a **non-blocking error**. The action goes ahead and Claude Code shows a "hook error" notice with the first line of stderr (for example "No such file or directory"). It blocks only for hooks that set `onFailure: "block"`, which Lullshift never sets, and `onFailure` has no effect on async hooks. Lullshift's hooks are async and exec form. A leftover Lullshift hook is therefore noise, not breakage: Claude Code keeps working, possibly with repeated hook-error notices. The cleanup above is about tidiness and trust. Codex and Cursor behavior with a missing executable is not documented in the sources checked; Cursor hooks are never `failClosed` and Codex hooks are async, so the same conclusion is assumed and should be confirmed by hand.

## 5. Manual checks the owner runs

Not run by automation (this machine must not run the installer): install and uninstall the NSIS setup with a connected test profile, then confirm `~/.claude/settings.json` is restored and the user's own hooks remain; install and remove the `.deb` on a Linux VM; open the dmg on Intel and Apple Silicon Macs; run the update validation (S11-A02) once the updater is enabled.
