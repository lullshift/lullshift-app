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

---

## Brand – App icon and tray mascot

### MB-1: App icon and tray mascot on each OS

**Why manual:** tray rendering, template images and taskbar colors differ by OS and theme. Only the image decoding and the face choice are unit-tested.

1. Build and start the app (see "Before you start").
2. Check the app icon in the taskbar or dock, the window title bar and Alt+Tab or the app switcher.
3. Check the tray or menu-bar icon in both the light and the dark system theme.
4. Start a Claude Code session and trigger a permission prompt, so the session needs you.
5. Answer the prompt, so the session goes back to working.

**Expected:**

- The app icon is the folded-paper mascot with one eye open, not the Tauri logo.
- The tray shows the mascot asleep (closed eyes). It is clearly visible on light and dark taskbars; on macOS it follows the menu-bar color.
- At step 4 the tray switches to awake (two round eyes) and the tooltip reads "Lullshift: an agent needs you".
- At step 5, or once that session goes stale and the window refreshes, the tray goes back to asleep.

**Result:** \_\_\_

---

## Sprint 02b – Design system and UX

### M02B-1: Screen reader pass

**Why manual:** keyboard behaviour was checked automatically, but no real screen reader was used.

1. Turn on Narrator (Windows: Ctrl+Win+Enter), VoiceOver (macOS: Cmd+F5) or Orca (Linux).
2. Use only the keyboard: move through the sidebar, Sessions, Integrations and Settings.
3. Open **Connect Claude Code**, read the dialog, then press Esc.
4. Toggle **Start with your computer** and toggle it back.

**Expected:**

- Each view announces its heading when you switch to it.
- Sessions read as "Claude Code, session 3f2a9c1b", then the state and the time.
- The badge on Sessions reads as "1 need you".
- The dialog announces its title, and focus returns to the button after Esc.
- The switch announces "on" and "off".

**Result:** \_\_\_

### M02B-2: Looks right on macOS and Linux

**Why manual:** fonts, scrollbars and blur render differently on each OS; only Windows was seen.

1. On each OS, open all three views in light and in dark system theme.
2. Make the window narrower than 720 px.

**Expected:**

- The Figtree font is used.
- Colours match the Windows screenshots.
- Under 720 px the sidebar becomes a top bar of icons with the attention badge.

**Result:** macOS: \_\_\_ Linux: \_\_\_

### M02B-3: Empty and error states

**Why manual:** the development database already had sessions, so these states weren't seen in the app.

1. Quit Lullshift. Rename the app data folder (see "Before you start") to keep it.
2. Start Lullshift.
3. Quit, restore the folder, and start Lullshift again.

**Expected:**

- At step 2, Sessions shows the mascot, "No agent sessions yet" and a **Connect Claude Code** button that opens Integrations.
- At step 3, your sessions are back.

**Result:** \_\_\_

### M02B-4: Live columns with real sessions

**Why manual:** when the layout was checked there were no Working or Needs-you sessions, so the Live columns were only seen empty.

1. Connect Claude Code. Start four or more Claude Code sessions in different terminals and give each a task, so several are **Working** at once.
2. In one of them, trigger a permission prompt (ask for something that needs approval).
3. Make the Lullshift window short (about 500-640 px tall) at its default width.
4. Scroll the **Working** column. Then type part of a session ID in the search box, and click the **Claude Code** chip and then **All tools**.
5. When the sessions finish, open **History**.

**Expected:**

- At step 2, the card moves to **Needs you** with an orange border, and the summary line says "Claude Code needs you".
- At step 4, only the Working column scrolls; Needs you and the summary stay in view. Search and chips narrow both columns; the counts on the tabs follow.
- At step 5, the finished sessions are in History, newest first.

**Result:** \_\_\_

---

## Sprint 03 – Notifications and activity host

### M03-1: Native install confirmation (S03-T07)

**Why manual:** automated runs never press **Connect** in the native dialog, because doing so would change your real Claude Code settings.

1. Open **Integrations**, then **Connect Claude Code**, and press **Continue**.
2. In the system dialog, check the text, then press **Cancel**.
3. Repeat, and press **Connect** this time.

**Expected:**

- The dialog is a native window, not part of the page. It is titled "Connect Claude Code?" and names the exact settings file.
- After **Cancel**, nothing changes. There is no error, and the settings file is untouched.
- After **Connect**, Integrations shows Claude Code as connected.

**Result:** \_\_\_

### M03-2: Notifications and quiet hours (S03-T01, S03-A02)

**Why manual:** system notifications appear outside the app, and the quiet-hours check depends on your clock.

1. In **Settings > Notifications**, leave everything on. In Claude Code, trigger a permission prompt.
2. Let a Claude Code reply finish without the task ending (a normal turn end).
3. Turn on **Quiet hours** with a window that includes the current time, and turn off "Still tell me when a session needs me". Trigger a permission prompt again.
4. Turn "Still tell me when a session needs me" back on and repeat.

**Expected:**

- Step 1: one notification, "Claude Code needs you".
- Step 2: no notification. The session shows as idle, never as finished.
- Step 3: no notification.
- Step 4: a notification.

**Result:** \_\_\_

### M03-3: Activities with keyboard and screen reader (S03-A04)

**Why manual:** keyboard play, focus and pause/resume were checked automatically on Windows, but no real screen reader was used.

1. Turn on Narrator, VoiceOver or Orca. Using only the keyboard, open **Activities** and start **Lantern Pond**.
2. Tab into the pond. Move with the arrow keys, and flip lanterns with Space or Enter.
3. Press **Pause**, then **Resume**. Then start **Step Away** and turn on the breathing cue.
4. Turn on reduced motion in the OS and start Step Away again.

**Expected:**

- Each lantern reads as "Lantern row 1, column 2, unlit" (or "lit"). Moves and a win are announced.
- After Resume, the pond and move count are as you left them.
- The paused card is announced, and focus lands on its button.
- With reduced motion, the breathing cue doesn't animate its size.

**Result:** \_\_\_

### M03-4: Agent attention pauses a game (S03-A01, end to end)

**Why manual:** the automated check used the hook forwarder only when run with `--attention`; this check uses a real agent.

1. Start Lantern Pond and make a few moves.
2. In Claude Code, start a task that will ask for permission.

**Expected:**

- When the prompt appears, the game is replaced at once by "Claude Code needs you", with **Go to Sessions** and **Keep playing**.
- **Keep playing** brings back the same board.

**Result:** \_\_\_

### M03-5: Sandbox on macOS and Linux (S03-A03)

**Why manual:** the denial runner uses the Chromium debug port, which WebKit doesn't have. Only Windows was run.

1. Build the debug app on macOS or Linux and open **Activities > Sandbox probe (debug builds only)**.
2. Wait for the results list.
3. Press **Pause** and **Resume**.

**Expected:**

- Every line starts with "blocked", except WebRTC, which is a known gap (ADR-012).
- The tick count is restored after Resume.

Record any "ESCAPED" line in ADR-012.

**Result:** macOS: \_\_\_ Linux: \_\_\_

---

## Sprint 04 – Context Keeper and Next Move

### M04-1: Opt-in capture with a real Claude Code session (S04-A04)

**Why manual:** it needs a real agent and your own prompts.

1. In **Integrations**, leave **Task context** off. Start a Claude Code session and give it a task that will ask for permission.
2. Open the session's detail panel.
3. Turn **Task context** on, read the panel that explains what is kept, and choose **Turn on**. Start a new session with a prompt that includes a fake key, such as `sk-test1234567890abcdefghijklmnopqrstuv`, and trigger a permission prompt.
4. Open the new session's panel, then choose **Clear captured text**.
5. Turn **Task context** off and start another session.

**Expected:**

- Step 2: no intent and no "what it's asking" text; only what you type yourself.
- Step 3: the intent shows your first prompt with the key replaced by `[hidden]`, labelled as captured with your OK. "What it's asking" shows the permission text.
- Step 4: the captured text is gone; your own notes stay.
- Step 5: nothing is captured again.

**Result:** \_\_\_

### M04-2: Export and delete (S04-A02)

**Why manual:** the native save dialog is outside the page.

1. Open an ended session in **History** and choose **Export Markdown**, then **Export JSON**. Save both, then open them.
2. Choose **Delete session** and confirm.

**Expected:**

- Step 1: a native save dialog each time. The files contain the timeline, your notes and items, and any captured text, labelled as captured.
- Step 2: the session disappears from History and doesn't come back after a restart.

**Result:** \_\_\_

### M04-3: Detail panel with a screen reader (S04-A05)

**Why manual:** keyboard behaviour is tested automatically, but no real screen reader was used.

1. With Narrator, VoiceOver or Orca on, Tab to a session card and press Enter.
2. Move through the panel, add a Next Move item, copy it, then press Esc.
3. Switch the panel between pinned and floating, and make the window narrow.

**Expected:**

- The panel announces its title, and the session's state is read out.
- **Copy** announces "Copied".
- Esc returns focus to the card you opened.
- Pinned in a narrow window shows the grouped list, with Needs you at the top. Floating overlays the board.

**Result:** \_\_\_

## Sprint 05 – Activity SDK and creator CLI

### M05-1: How to play, paused view and previews (S05-A05)

**Why manual:** the help dialog and paused view are checked by unit tests and the simulator, but not by a person in the real app or with a screen reader.

1. Open **Activities**. Each card shows a preview image and a **How to play** button. Open it with the keyboard (Tab, Enter) for Lantern Pond and Step Away, then close it with Esc.
2. Start Lantern Pond, press a few lanterns, then press **How to play** in the title bar.
3. Close the dialog. Then press **Pause**, and also try leaving the Activities view.
4. With a screen reader on, read the paused card.

**Expected:**

- Step 1: help shows a summary, goal, numbered steps, a controls table and tips. Esc returns focus to the button you used.
- Step 2: the game pauses before the dialog opens.
- Step 3: the paused card shows "Played this time", the game's stats (lanterns lit, moves) and a **How to play** button. It is never an empty card. Step Away shows its time away. An activity that reports no stats shows its preview image.
- Lantern Pond's four buttons sit on one row.
- Step 4: the stats are read as label and value pairs.

**Result:** \_\_\_

### M05-2: Disconnect and session titles (S05-A06)

**Why manual:** this needs the real Claude Code hooks and a real `/rename`.

1. With capture on and a Claude Code session working, run `/rename My test task` in that session, then let it reach a turn end or a question.
2. Rename another session yourself from its detail panel.
3. In **Integrations**, disconnect Claude Code while a session is working.
4. Reconnect, and send a new prompt in that session.

**Expected:**

- Step 1: the card shows "My test task", with the tool name and short id as secondary text.
- Step 2: your own name wins over the agent's title.
- Step 3: a status line says how many sessions are no longer tracked. No Claude Code session stays in Needs you or Working, and History labels them "Not tracked: disconnected".
- Step 4: the session comes back as tracked.

**Result:** \_\_\_

### M05-3: Creator CLI on macOS and Linux

**Why manual:** the simulator was run on Windows with Microsoft Edge only.

1. On macOS and on Linux, with Chrome or Chromium installed, run `pnpm lullshift create /tmp/pebbles`, then `pnpm lullshift simulate /tmp/pebbles`.
2. Run `pnpm lullshift dev /tmp/pebbles`. Play, press **Agent needs you**, then **Start or resume**.
3. Run `pnpm lullshift pack /tmp/pebbles` twice and compare the printed SHA-256.
4. Run the Rust checks for the title reader (see the expected result).

**Expected:**

- Step 1: all scenarios pass, and no `lullshift-sim-*` folder is left in the temp folder.
- Step 2: the activity pauses and resumes where it was.
- Step 3: the same hash both times. Across machines the hash matches only when the files are byte-identical; Git may change line endings on checkout.
- Step 4: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test -p agent-adapters` pass on macOS and Linux. The title reader's Unix branch (`O_NOFOLLOW | O_NONBLOCK`, the FIFO test) was only compiled on Windows so far.

**Result:** \_\_\_
