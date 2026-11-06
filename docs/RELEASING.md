# Releasing Lullshift

How v1 installers and the npm packages are built, what the owner must do by hand, and what is deliberately not done yet. Decision record: ADR-004 (amended 2026-11-05). Hook cleanup on uninstall: ADR-010. npm packages: section 6 and the ADR-017 amendment.

Nothing here runs by itself. There is no push, pull-request, merge or scheduled build. The release workflow never publishes: it creates a **draft** GitHub Release for the owner to review. The npm workflow (section 6) publishes only after the owner approves the run.

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

5. Add `tauri-plugin-updater` (desktop only), register it in `lib.rs`, grant the updater permissions to the `main` window only, and add a manual **Check for updates** button. **Never call `check()` on startup or in the background unless the user has turned on the opt-in "Check for updates once a day" setting** (off by default; ADR-018, decision 26.3). The PRD only allows network use the user starts. Show version and source and wait for the user to confirm the install.
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

An uninstall must not leave Claude Code, Codex, Cursor or OpenCode calling a missing executable. `lullshift-desktop --uninstall-hooks` (no UI, no output, always exit 0, idempotent) runs the same logic as **Disconnect** for all four agents (for OpenCode it deletes the plugin file `lullshift-observer.js`, only if its first line is Lullshift's header), on the settings file recorded at install time (`lullshift.db` in the app data folder). The original file comes back byte for byte when nothing else changed; otherwise only Lullshift's own entries are removed and the user's other hooks and settings stay. A file that doesn't parse is left untouched. It never creates a database or a settings file. An optional absolute `--data-dir` overrides the app data folder.

| Platform       | How cleanup happens                                                                                                                                                                                                                                                                                                                                            |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Windows (NSIS) | `NSIS_HOOK_PREUNINSTALL` in `apps/desktop/src-tauri/windows/hooks.nsh` runs the flag, unless the uninstaller runs in update mode (`/UPDATE`), so an in-place update keeps working hooks. Every other uninstall removes them, including the one a new setup runs over an existing install; `NSIS_HOOK_POSTINSTALL` then restores them for recorded agents after the new install finished (see below). The install mode is `currentUser`, so this is the account that connected the agents. If a per-machine mode is ever offered, revisit: the hook would act on the elevated account. |
| Linux `.deb`   | `prerm` (`apps/desktop/src-tauri/linux/prerm`, set as `bundle.linux.deb.preRemoveScript`) runs the flag for each normal user (uid 1000 and up, real login shell) with `runuser`, never as root, and only for `remove` (not `upgrade`).                                                                                                                         |
| Linux AppImage | No uninstaller. Use **Disconnect** in the app before deleting the file. Hooks point at the AppImage file itself (the `APPIMAGE` variable), not the temporary mount, so they survive restarts (S12-T05; see below).                                                                                                                                         |
| macOS          | Dragging the app to the Trash runs nothing. Press **Disconnect** for each agent in Lullshift first; the release notes must say so. Manual cleanup is in ADR-010.                                                                                                                                                                                                |

If the app was removed without disconnecting, delete the hook entries whose command path contains `lullshift` from `~/.claude/settings.json`, `~/.codex/hooks.json` and `~/.cursor/hooks.json`, and delete `~/.config/opencode/plugins/lullshift-observer.js`.

**Windows reinstall keeps the hooks (S12-T05, owner decision 26.8; rebuilt after security review M1).** A new setup run over an existing install first runs the old uninstaller itself, as `uninstall.exe [/P] _?=<install dir>`. Tauri's generated `installer.nsi` (`PageLeaveReinstall`, about lines 322 to 400) does this on the reinstall page, **before** the folder page. The user can still cancel afterwards, or install into a different folder, so the earlier rule "keep the hooks when the uninstaller runs in place" could leave hooks pointing at a deleted program in a user-writable folder (a persistence point). The rule now is:

- **Hooks are removed by every uninstaller except `/UPDATE`.** Update mode replaces the program in the same folder, so the same hooks keep working. `hooks.nsh` tests only `$UpdateMode <> 1`.
- **A reinstall restores them after it finished.** When `$EXEDIR == $INSTDIR` (the uninstaller was run in place by a setup; a real uninstall via Settings > Apps or by hand copies the uninstaller to `%TEMP%\~nsuN.tmp`, so `$EXEDIR` is that folder; verified on 2026-11-06 with the bundled `makensis`, also with `/S` and a trailing backslash) the macro runs `--uninstall-hooks --keep-records`: the hooks are removed exactly as before, but the install records stay in the database and a marker file `hook-restore.pending` (Unix seconds) is written in the app data folder. `NSIS_HOOK_POSTINSTALL` then runs the new program with `--repair-recorded-hooks` (`src/repair_hooks.rs`; silent, no UI, always exit 0). With a marker no older than one hour, for each adapter that has a stored install record whose hooks are missing or point elsewhere, it installs the hooks again for the new program path, with the usual backup, and saves the new record. It deletes the marker first (one shot). Already-correct hooks are not rewritten.
- **What keeps consent is the stored install record** (`adapter_installations`, written by Connect, deleted by Disconnect and by a real uninstall). An adapter without a record is never touched. **If the database was deleted, nothing is re-applied.** Without a fresh marker the mode does nothing at all, so a first install, an update and a plain start never change an agent file.
- **Uninstall then cancel:** the hooks stay removed and the marker is left behind. If a setup finishes within an hour (the user tried again) the hooks are restored; later, the stale marker is deleted by the next run of the mode and the records are marked uninstalled, nothing is restored.
- **Failure modes are safe.** If `$EXEDIR` is ever not equal to `$INSTDIR` for a reinstall, the plain `--uninstall-hooks` runs, the records are deleted and nothing is restored (the user presses Connect again); no case leaves hooks pointing at a deleted program. Running `uninstall.exe _?=<dir>` by hand behaves like a reinstall that was cancelled (hooks removed, records kept for an hour).
- **Linux `.deb`:** unchanged. `prerm` runs only for `remove`, not `upgrade`, and `postinst` does nothing, so an upgrade keeps the hooks (same path) and a removal deletes them.
- **Not run for real:** the installer scripts compile with `makensis`, and the Rust side is tested against temp directories (`repair_hooks::tests`, `uninstall_hooks::tests`, `tests/release_config.rs`); the real installer was not executed on this machine. Manual checks: same version, choose Uninstall on the reinstall page and cancel (no hooks left); upgrade into a different folder (hooks restored to the new path); upgrade with `/P`; a normal Settings > Apps uninstall (hooks and records gone).
- If Tauri's installer template ever changes how it calls the old uninstaller, the `release_config` test only checks that the rules are present in `hooks.nsh`, not the template; re-run the manual reinstall checks after upgrading `tauri-bundler`.

**AppImage hooks (S12-T05, owner decision 26.7).** An AppImage mounts itself at a temporary `/tmp/.mount_...` path. When `APPIMAGE` is set, absolute and names an existing file, Connect and Repair write that file into the hook commands instead of the running executable (`apps/desktop/src-tauri/src/hook_exe.rs`). Conditions: Linux only; the file name must start with `lullshift` (the hook files recognise their own entries by that prefix, so a renamed AppImage falls back to the running executable and shows "Needs repair" after a restart); and if `APPDIR` is set the running executable must be inside it, so an `APPIMAGE` variable inherited from another AppImage is ignored. Known limits: hooks break if the user moves or deletes the AppImage file (the app then shows "Needs repair" the next time it runs, and Disconnect before deleting it); each hook run starts the AppImage, which mounts it first, so hook runs are slower than with the `.deb`, and the forwarder's 2 second limit applies to the whole run. Prefer the `.deb` for agent hooks.

### Known caveats

- **Windows, cancelled uninstall:** the hook cleanup runs before the uninstaller checks whether Lullshift is running. If the user then cancels at the "app is running" prompt, the app stays installed but its agents are disconnected (shown honestly in Integrations; reconnect to restore). Moving the cleanup after the uninstall would lose the backups needed for an exact restore.
- **Linux, per-user settings in shell profiles:** `prerm` runs without the user's shell environment, so a user who sets `CLAUDE_CONFIG_DIR`, `CODEX_HOME` or `XDG_DATA_HOME` in a profile gets the default locations. Nothing is created, but their hooks stay; they should Disconnect before removing the package.
- **Linux, slow homes:** each user's cleanup is limited to 30 seconds so an unreachable home or directory service cannot stall package removal.

### What Claude Code does when the hook executable is missing

From the Claude Code hooks documentation (checked 2026-11-06): a command hook that cannot be started, or exits with a code other than 0 or 2, is a **non-blocking error**. The action goes ahead and Claude Code shows a "hook error" notice with the first line of stderr (for example "No such file or directory"). It blocks only for hooks that set `onFailure: "block"`, which Lullshift never sets, and `onFailure` has no effect on async hooks. Lullshift's hooks are async and exec form. A leftover Lullshift hook is therefore noise, not breakage: Claude Code keeps working, possibly with repeated hook-error notices. The cleanup above is about tidiness and trust. Codex and Cursor behavior with a missing executable is not documented in the sources checked; Cursor hooks are never `failClosed` and Codex hooks are async, so the same conclusion is assumed and should be confirmed by hand.

## 5. Manual checks the owner runs

Not run by automation (this machine must not run the installer): install and uninstall the NSIS setup with a connected test profile, then confirm `~/.claude/settings.json` is restored and the user's own hooks remain; install and remove the `.deb` on a Linux VM; open the dmg on Intel and Apple Silicon Macs; run the update validation (S11-A02) once the updater is enabled.

## 6. npm packages (owner decision 29)

`@lullshift/agent-protocol`, `@lullshift/activity-sdk` and `@lullshift/cli` are published to npm by a separate manual workflow, `.github/workflows/publish-npm.yml`. The CLI is not in the installers. Decision record: ADR-017 amendment (2026-11-06).

Nothing is published yet. The workflow has **not been run**, and the first run needs the owner.

### What the workflow does

1. Starts only by hand: Actions > **Publish npm packages** > **Run workflow**. Choose the packages (`all`, or one), the dist-tag (`latest` or `next`) and whether it is a dry run. **Dry run is on by default.**
2. Runs only on `main`, and waits for approval in the `npm` environment.
3. Installs with `pnpm install --frozen-lockfile`, checks the generated schemas, typechecks and tests the three packages.
4. Builds and packs them (`node scripts/check-npm-packages.mjs`), and refuses any tarball with files outside its allowlist, tests, source maps, `.env`, local paths or `workspace:` specifiers.
5. Installs the tarballs in an empty folder outside the repository and runs `lullshift create`, `validate` and `simulate` with them.
6. Publishes in dependency order (protocol, SDK, CLI) with `pnpm publish --provenance --access public --no-git-checks --tag <tag>`. A version already on npm is skipped. A package whose `@lullshift` dependency isn't on npm yet stops the run.

The npm token reaches only the last step, as `NODE_AUTH_TOKEN`. Each package's `prepublishOnly` guard (`scripts/npm-publish-guard.mjs`) refuses a publish anywhere but this workflow on `main`.

### Owner steps before the first publish

1. **Make sure the repository is public.** npm provenance is not generated for private repositories.
2. **Register the scope.** Sign in to npmjs.com (with two-factor authentication), and create the free organization `lullshift`, which owns the `@lullshift` scope.
3. **Choose how the workflow signs in.** One of these two:
   - **Trusted publisher (preferred, no token):** on npmjs.com, open each package's settings and add a trusted publisher. Choose GitHub Actions, organization or user `lullshift`, repository `lullshift-app`, workflow file `publish-npm.yml` and environment `npm`. A package must exist before it can be configured, so the very first publish of each package needs a token (next option); add the trusted publisher straight after and delete the token. With trusted publishing, leave the `NPM_TOKEN` secret unset.
   - **Granular access token:** npmjs.com > Access Tokens > Generate New Token > Granular Access Token. Give it read and write access to the `@lullshift` packages only (or to the `lullshift` organization for the first publish), and the shortest expiry that suits you. Never put it in `.npmrc`, `.env` or the repository.
4. **Create the environment.** GitHub > Settings > Environments > New environment, named `npm`. Add yourself as a **required reviewer**, limit deployment branches to `main`, and, if you use a token, add it as the environment secret `NPM_TOKEN`. Do not add it as a repository secret.
5. **Bump versions** in the three `package.json` files when needed and merge to `main`. The SDK version also lives in `SDK_VERSION` (`packages/activity-sdk/src/index.ts`). The CLI prints the SDK version for `--version`.
6. Run the workflow once as a **dry run**, read the file lists it prints, then run it again with dry run off and approve it.
7. **After publishing,** check each package page on npmjs.com for the provenance badge, and run `npx @lullshift/cli@<version> --help` on a clean machine.

### Trying it locally

Nothing local can publish. To see exactly what would be uploaded:

```bash
node scripts/check-npm-packages.mjs --keep ../npm-tarballs
npm publish --dry-run ../npm-tarballs/lullshift-cli-0.1.0.tgz
```

`pnpm publish --dry-run` in a package folder is refused by the guard outside the workflow, because pnpm gives lifecycle scripts no way to tell a dry run apart.

### Sources checked (2026-11-06)

- npm, [Trusted publishing for npm packages](https://docs.npmjs.com/trusted-publishers): needs npm CLI 11.5.1 or later and Node 22 or later, GitHub-hosted runners only, `id-token: write`, the workflow file name must match exactly, and provenance is added on its own.
- pnpm, [pnpm publish](https://pnpm.io/cli/publish): `--provenance`; "under trusted publishing, pnpm attaches provenance to a public package from a public repository on its own".
- pnpm, [package.json](https://pnpm.io/package_json): `publishConfig` may override `bin`, `main`, `exports`, `types`, `engines` and other fields when packing.

Not verified without a real run: that pnpm 12.6.0 completes the OIDC exchange with npm, and that headless Chrome on `ubuntu-24.04` runs the simulator step.
