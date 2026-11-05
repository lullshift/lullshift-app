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

## Sprint 06 – Game kits and first games

### M06-1: Play every new game by keyboard and with a screen reader (S06-A02)

**Why manual:** the simulator checks interruption, saving and that something takes focus, but not whether a person can actually play, or what a screen reader says.

1. In **Activities**, open each new game: Moss Paths, Fog Garden, Stone Slide, Petal Grid, Ridge Walk, Tide Pairs, Firefly Catch and Kite Drift.
2. Play a full round with the keyboard only, following the game's **How to play**.
3. Repeat one round of each with a screen reader on (Narrator, VoiceOver or Orca).
4. Pause half way, close the app, reopen it and resume.

**Expected:**

- Step 2: every action works from the keyboard, the focus ring is always visible, and the controls sit on one row.
- Step 3: each cell, card or control is read with a useful name (for example "Stone 7, can slide"), and wins and important changes are announced once.
- Step 4: the game comes back exactly as it was, and the paused view shows the game's stats.
- Nothing looks like a copy of a known branded game.

**Result:** \_\_\_

### M06-2: Real-time games pause, count down and respect reduced motion

**Why manual:** timing and motion need a person watching the real app.

1. Start Firefly Catch, then trigger an agent "needs you" event (or press **Pause**).
2. Wait 10 seconds, then resume.
3. Turn on the OS reduced-motion setting and repeat with Kite Drift.

**Expected:**

- Step 1: the game stops at once; the timer and fireflies don't move while paused.
- Step 2: a 3-2-1 countdown runs before play continues.
- Step 3: no countdown animation; a static "Ready" prompt waits for Enter or Space, and there is no parallax or pulsing.

**Result:** \_\_\_

### M06-3: Games on macOS and Linux

**Why manual:** the games were simulated on Windows (Microsoft Edge) only.

1. On macOS and Linux, run `node tests/activities/check-bundled.mjs`.
2. Open two of the new games in the app and play a few moves.

**Expected:** every activity passes validate and simulate, and the games render and play as on Windows.

**Result:** \_\_\_

## Sprint 07 – Engines and content packs

### M07-1: Play one pack of every engine by keyboard and with a screen reader (S07-T05)

**Why manual:** the accessibility review was static and the simulator does not hear a screen reader. The fixes (announced hints and answers, card text read on Next, plain-text typing target, toy cursor position) need a person to confirm them.

1. In **Activities**, open one pack per engine, for example Gentle Box Breathing, Eye Rest, Three-Minute Energiser, Kitchen Break, Computing Word Origins Quiz, Lateral Thinking, Small Wins, Calm Sentences, Dusk Ripples and Desk Stretches.
2. Use each with the keyboard only, following its **How to play**.
3. Repeat with a screen reader on (Narrator, VoiceOver or Orca).
4. Zoom the app to 200% and narrow the window.

**Expected:**

- Step 2: everything works from the keyboard and focus is always visible, including after the last step of a timed pack (focus moves to a sensible button, never disappears).
- Step 3: moving to the next card reads its text; a teaser's hint and answer are read out; typing never reads back what you typed; the toy says where the cursor is.
- Step 4: the buttons wrap onto a second line; nothing scrolls sideways.
- Text boxes (Reflection notes, Typing) have a clearly visible border in light and dark themes.

**Result:** \_\_\_

### M07-2: Timed packs stop when interrupted

**Why manual:** timing needs a person watching the real app.

1. Start Gentle Box Breathing, press **Begin**, then trigger an agent "needs you" event (or press **Pause**).
2. Wait 10 seconds, then resume.
3. Repeat with a Guided Pause and a Movement Routine pack.

**Expected:** the timer stops at once, comes back stopped with the remaining time kept, and only continues when you press Continue. Movement never moves to the next move by itself.

**Result:** \_\_\_

### M07-3: Read the content

**Why manual:** a content review was done by an agent; the product owner should read a sample.

1. Read through at least one pack per engine, plus the style guide (`docs/CONTENT_STYLE_GUIDE.md`).

**Expected:** the tone is calm and plain; no health claims; no stock riddles or quotes; teaser and quiz answers are correct; nothing looks like a copy of a known puzzle book, game or site.

**Result:** \_\_\_

### M07-4: Broken and missing pack data

**Why manual:** the app's error card for a broken pack is covered by tests, not by a person looking at it.

1. Copy a pack folder inside `activities/content/`, give it a new `id`, and add an unknown field to its data file. Rebuild and open it.
2. Delete the data file and open it again.
3. Remove the copy.

**Expected:** the pack does not run; a clear, friendly error card names the problem (for example "surprise isn't a known field"), and the rest of the app keeps working.

**Result:** \_\_\_

### M07-5: Packs on macOS and Linux

1. On macOS and Linux, run `node tests/activities/check-engine-compat.mjs` and `node tests/activities/check-bundled.mjs`.
2. Open two packs in the app.

**Expected:** all checks pass, and the packs render as on Windows.

**Result:** \_\_\_

## Sprint 08 – Git marketplaces

### M08-1: Install from a public and a private test repository (S08-A01, S08-T04)

**Why manual:** this needs real repositories on a Git host and your own Git sign-in; the automated tests use local repositories only.

1. Create a public test repository with `.lullshift/marketplace.json` (name, for example, `my-test-shelf`) listing one content pack folder copied from `activities/content/` with a new `id`. Push it.
2. In **Activities > Marketplaces**, add it by `owner/repo`. Browse it and press **Install** on the pack.
3. Check the review screen, then install. Open the pack from the Library.
4. Make the repository private (or create a private copy), remove the marketplace, and add it again with the private URL, first in a shell where `git clone` of it works, then after signing Git out of that host.
5. Search the app data folder and logs for your user name, token or the word `password`.

**Expected:**

- Step 3: the review shows the marketplace, source URL, full commit, "Content pack", the engine, permissions, size and the "Unverified" notice; after install the pack plays and its card shows the marketplace name.
- Step 4: with Git access it works the same as public; without it, the message says Git couldn't access the repository with your current Git sign-in, and nothing hangs. Your credential manager may show its own sign-in window; Lullshift never asks for a password itself.
- Step 5: no credentials anywhere in Lullshift's files.

**Result:** \_\_\_

### M08-2: Updates are never silent (S08-A03)

1. In the test repository, add `activity.storage` to the pack's permissions, change its version and push.
2. Restart Lullshift and wait a minute: nothing should change.
3. Under **Manage**, press **Check for update** on the pack.

**Expected:** Step 2: no update and no network use. Step 3: the review shows the old and new version and commit, with `activity.storage` marked as new and needing approval; nothing installs until you press the button; the pack keeps its on/off setting.

**Result:** \_\_\_

### M08-3: Built-in activities offline on first launch (S08-A04)

1. Disconnect from the network. Rename the app data folder (to start fresh) and launch Lullshift.
2. Open three built-in activities, including one content pack.
3. Under **Manage**, turn one off and on again. Restore your app data folder afterwards.

**Expected:** everything works without a network; a turned-off activity disappears from the Library and comes back with its progress.

**Result:** \_\_\_

### M08-4: Marketplaces on macOS and Linux

1. On macOS and Linux, run `cargo test -p marketplace` and repeat M08-1 steps 2 and 3.

**Expected:** all tests pass (they include the Unix-only checks), and installing works as on Windows.

**Result:** \_\_\_

## Sprint 09 – Companion (BYOK)

### M09-1: A real provider with your own key (S09-A01)

**Why manual:** the automated checks use a local fake provider; a real key and real provider responses need you.

1. In **Companion > Settings**, add an Anthropic provider with a current model name and your key. Press **Test connection**.
2. Repeat with OpenAI (or an OpenAI-compatible service you use) if you have a key.
3. Start a conversation and send two messages with **No context**.
4. Enter a wrong key, test again; then set a tiny monthly budget (for example 50 tokens) and send.
5. Search the app data folders (`%APPDATA%\app.lullshift.desktop` and `%LOCALAPPDATA%\app.lullshift.desktop` on Windows) for a distinctive part of your key.
6. Remove the provider and check the system keychain (Credential Manager, Keychain Access, Seahorse) has no Lullshift entry.

**Expected:** replies arrive and show as plain text; a wrong key says the key was rejected; the tiny budget blocks sending with a clear message; the key is found nowhere in Lullshift's files; removal deletes the keychain entry. The OpenAI request format was written without the official page to hand, so a failure there is a real bug to report.

**Result:** \_\_\_

### M09-2: Context is only shared when you choose it (S09-A02)

1. With an agent session that has notes and checklist items, send a message with **Session summary**, then one with **Selected notes** (tick one note and one item).
2. In each preview, read the exact text, then send.
3. Expand **Shared with** on each sent message.

**Expected:** nothing is sent until you press **Send with this context**; the text matches the preview; the choice goes back to **No context** after each message; nothing else from your sessions (captured prompts, other sessions) appears.

**Result:** \_\_\_

### M09-3: Keyboard, screen reader, offline

1. Use Companion with the keyboard only, then with a screen reader: settings, key entry, the context preview dialog, sending, cancel, Try again.
2. Disconnect from the network and send.

**Expected:** every control is reachable with visible focus; the screen reader announces sending, replies and errors; offline gives "Can't reach <host>", the message stays as not sent, and **Try again** works once you reconnect.

**Result:** \_\_\_

### M09-4: Keychain on macOS and Linux

1. On macOS and Linux, run `cargo test -p companion -- --ignored` and repeat M09-1 steps 1 and 6.
2. On a Linux desktop without a Secret Service running, open Companion settings.

**Expected:** the keychain test passes and leaves nothing behind; without a Secret Service, Companion says the system keychain isn't available and does not offer to store the key elsewhere.

**Result:** \_\_\_
