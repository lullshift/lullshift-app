# Manual test checklist

Checks that automated tests can't cover, or that need a person, an account or another OS. The product owner runs them together once all sprints are done, before release (S11-T02).

Each sprint adds its manual checks here as it closes. A check stays unticked until someone runs it, writes the date, OS and result in its **Result** line, and ticks the related sprint item if one is still open.

## Before you start

Build and start the app (Windows, PowerShell, from the repo root):

```powershell
pnpm install
pnpm check
cd apps/desktop; pnpm tauri build --debug --no-bundle; cd ../..
Start-Process .\target\debug\lullshift-desktop.exe
```

On macOS and Linux, run the same commands in a terminal and start `./target/debug/lullshift-desktop`.

App data lives in `%APPDATA%\app.lullshift.desktop` (Windows), `~/Library/Application Support/app.lullshift.desktop` (macOS) or `~/.local/share/app.lullshift.desktop` (Linux).

Once installers exist (Sprint 11), repeat the app checks with the installed build as well as the debug build.

---

## Sprint 00 – Foundations

### M00-1: Quality gate on macOS and Linux

**Why manual:** there were no macOS or Linux machines during development. Only Windows has run.

1. On each OS, run `pnpm install` and then `pnpm check`.
2. Start the app and check that the window opens and the dashboard loads.

**Expected:** all steps of `pnpm check` pass and the app starts.

**Result:** macOS: \_\_\_ Linux: \_\_\_

### M00-2: App works fully offline

**Why manual:** this was verified by configuration only (the content security policy and no HTTP code), never with the network off.

1. Turn off Wi-Fi and unplug the network cable.
2. Start the app. Use the dashboard and settings.

**Expected:**

- No sign-in prompt and no errors about the network.
- Every screen works.

**Result:** \_\_\_

---

## Sprint 01 – Local core and dashboard

### M01-1: Start at sign-in

**Why manual:** turning it on writes an OS startup entry, so it was never turned on during development.

1. In **Settings**, turn on start at sign-in.
2. Sign out of the OS and back in.

**Expected:**

- Lullshift starts in the tray with no window shown.
- Turning the setting off and signing in again doesn't start it.

**Result:** \_\_\_

### M01-2: Stale session after 10 minutes

**Why manual:** reducer and store tests cover this, but nobody has waited it out in the app.

1. Send one event with the dev client:

   ```powershell
   cargo run -p agent-ipc --example send_event -- "$env:APPDATA\app.lullshift.desktop" tests/fixtures/agent-event/valid/working-minimal.json
   ```

2. Leave the app open for 11 minutes.

**Expected:** the session changes from **Working** to a stale state, and the app doesn't claim it is still working.

**Result:** \_\_\_

### M01-3: Local IPC on macOS and Linux

**Why manual:** the Unix socket code was compiled and linted for both platforms but never run.

1. On each OS, start the app.
2. Send an event with the dev client, using that OS's app data path.
3. Check the socket's permissions with `ls -l <app data>/ipc`.

**Expected:**

- The reply is `{"ok":true}` and the session appears on the dashboard.
- The `ipc` folder is `drwx------` (0700) and the socket is `srw-------` (0600).

**Result:** macOS: \_\_\_ Linux: \_\_\_

---

## Sprint 02 – Claude Code adapter

### M02-1: Real Claude Code session (S02-A01)

**Why manual:** this needs your signed-in Claude Code and an interactive session. A "needs attention" event only happens interactively.

**Note:** connecting edits your real Claude Code user settings. Clicking the button is your consent. Any Claude Code session already running reloads the settings and starts reporting too, which is harmless.

1. Record the settings file's hash:

   ```powershell
   Get-FileHash $HOME\.claude\settings.json | Select-Object Hash
   ```

   (On macOS or Linux: `shasum -a 256 ~/.claude/settings.json`.)

2. In Lullshift, go to **Agent integrations → Connect Claude Code…**. Check that the path shown is your settings file, then click **Add hooks**.
3. In a new terminal, in a folder Claude Code already trusts, run `claude` and ask: `create hello.txt containing hi`.
4. While Claude waits for your permission, check the dashboard and your notifications.
5. Approve. Wait for the reply.
6. Type `/exit`.
7. In Lullshift, click **Disconnect… → Remove hooks**.
8. Run the hash command from step 1 again.

**Expected:**

- After step 3 the session shows **Working**.
- At step 4 it shows **Needs attention**, with **exactly one** OS notification and no console window flashing.
- After step 5 it goes to **Working** and then **Idle**.
- After step 6 it shows **Ended**.
- At step 7 the app says the file was restored exactly.
- At step 8 the hash matches step 1.

**Result:** \_\_\_

### M02-2: Repair after an app move keeps your edits

**Why manual:** this is covered by tests against temporary files; this checks it on a real setup.

1. Connect Claude Code.
2. Add a harmless setting to the settings file by hand, for example `"cleanupPeriodDays": 30`.
3. Copy the app executable to another folder and start it from there. The card should say **Needs repair**.
4. Click **Repair hooks… → Add hooks**.
5. Click **Disconnect… → Remove hooks**.

**Expected:**

- At step 5 the app says your later changes were kept.
- Your setting is still there and no Lullshift entries remain.

**Result:** \_\_\_

### M02-3: Settings file protections on macOS and Linux

**Why manual:** this test only runs on Unix.

1. On macOS and on Linux, run `cargo test -p agent-adapters`.

**Expected:** `unix_permissions_are_kept_and_symlinks_refused` passes.

**Result:** macOS: \_\_\_ Linux: \_\_\_

### M02-4: Release build hook mode (no console window)

**Why manual:** release builds use a different Windows subsystem from the debug build tested so far.

1. Connect Claude Code using an installed release build.
2. Run a short Claude Code session.

**Expected:** no console window flashes during the session, and events appear as in M02-1.

**Result:** \_\_\_

### M02-5: Uninstalling Lullshift removes the hooks (with S11-T06)

**Why manual:** this needs the OS uninstaller from Sprint 11.

1. Connect Claude Code.
2. Uninstall Lullshift with the OS uninstaller.
3. Run `claude` once.

**Expected:**

- No Lullshift entries remain in the settings file.
- Claude Code shows no hook errors.

**Result:** \_\_\_
