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

## Sprint 10: More adapters and content

### M10-1: Codex

On Windows, use the installed release build (not a debug build): the release app is a GUI program and hooks run through PowerShell may behave differently.

1. With Codex installed, set `CODEX_HOME` to a scratch folder, start Lullshift, open Integrations and choose **Connect Codex**. Confirm both dialogs.
2. Start Codex with the same `CODEX_HOME`, open `/hooks` and trust the Lullshift hooks. Send a prompt, ask for a command that needs approval, interrupt a turn, then quit.
3. Set `[features] hooks = false` in that `config.toml` and reopen Integrations.
4. Disconnect.

**Expected:** hooks.json gains six async entries plus a synchronous SessionEnd; before trusting, no events arrive; afterwards the session shows working, needs you at the approval, an ended turn after interrupt, and ended on quit; step 3 shows "Hooks turned off"; Disconnect restores the file exactly; `config.toml` is never changed.

**Result:** \_\_\_

### M10-2: Cursor (Windows and macOS/Linux)

On Windows, use the installed release build (not a debug build): the release app is a GUI program and hooks run through PowerShell may behave differently.

1. Back up `~/.cursor/hooks.json`. Choose **Connect Cursor** and confirm.
2. Use Cursor's agent: send a prompt, let it run a tool and finish, then close the session.
3. Check that your prompt is still sent normally and that no hook output appears in Cursor's Hooks panel or output.
4. With Lullshift's Claude Code hooks also installed, run one prompt in Cursor.
5. Disconnect.

**Expected:** six hooks with `"version": 1`; sessions appear as Cursor (never "needs you"); the prompt is never blocked; the Cursor run in step 4 creates no Claude Code session; Disconnect restores the file.

**Result:** \_\_\_

### M10-3: New games and packs with a screen reader and keyboard only

1. With NVDA (Windows) or VoiceOver (macOS) running, open each new game from Sprint 10: Breeze Bounce, Glow Trail, Teacup Tidy (was Tea Sort), Pebble Merge, Sea Glass, Thread Picture, Brook Links, Island Bridges, Harvest Pairs, Clock Patience and Frost Steps. Play a few moves with the keyboard only.
2. In each grid game, choose the daily option twice on the same day (Fog Garden: **Today's field**).
3. Open three or four of the new content packs (for example a quiz, a teaser set and a calm-cards deck) and go through them with the keyboard.
4. Turn on reduced motion in the OS and open Breeze Bounce, Glow Trail and Pebble Merge.

**Expected:** every control and cell is announced with a meaningful name and state; nothing depends on colour alone; the daily puzzle is the same both times; reduced motion removes animation without breaking play.

**Result:** \_\_\_

### M10-4: Clock Patience auto-play pauses with the host

1. Start Clock Patience and press **Turn** so it auto-plays.
2. While cards are turning, trigger a host pause (an agent needs you, or press the pause control), then resume.

**Expected:** auto-play stops while paused and continues from the same card after resume; no cards are turned while paused.

**Result:** \_\_\_

## Sprint 11

### M11-1: First-launch welcome

Use a fresh profile (delete the app data folder, or a new OS user), then start Lullshift.

1. A "Welcome to Lullshift" dialog opens over Sessions. Focus is on **Skip for now**; Tab stays inside the dialog.
2. Read it: it says there is no account, everything stays on this computer, and it works offline; it names Claude Code, Codex and Cursor and their hooks or settings file, says a backup is saved first, and says nothing changes until you confirm here and in a system dialog.
3. Press **Esc**. The dialog closes and focus lands on the Sessions heading. Quit and start again.
4. Repeat from a fresh profile, but choose **Connect an agent**: Integrations opens with every supported agent and nothing has been written yet. Start Connect for one agent, cancel the consent dialog, and check its hook file is unchanged. With every agent already connected, the welcome shows no connect button (S12-T07).
5. Repeat once more and choose **Try ...** (or **Browse activities**): the Activities library opens.

**Expected:** the welcome never appears again after step 3, 4 or 5 (including after a restart); the suggested activity name matches one in the library; the backup and file path statements match what Connect really does (compare with the consent dialog and the backups folder under the app data directory).

**Result:** \_\_\_

### M11-2: Screen reader pass (NVDA on Windows, VoiceOver on macOS, Orca on Linux if available)

Go through Sessions (Live and History tabs, open a session's detail panel), Activities (Library, Manage, Marketplaces; start, pause, resume and close an activity; open How to play), Companion, Integrations (open and cancel each consent dialog; connect and disconnect with a throwaway settings file), and Settings.

**Expected:** one `main` landmark and a `navigation` landmark named "Main"; each view announces its `h1` when you switch to it; every control has a name (the nav is still named when it shows icons only in a narrow window); dialogs announce their title and trap focus, and Esc closes them; after a dialog or an activity closes, focus is on a sensible control, not the top of the page; status changes (Saving..., Paused, Connected, errors) are announced once without stealing focus; a pill's state is read as text, not only a colour.

**Result:** \_\_\_

### M11-3: Keyboard-only, zoom and reduced motion

1. Unplug the mouse. Reach and operate every action in each view with Tab, Shift+Tab, Enter, Space, arrows and Esc; the focus ring is always visible.
2. Set the window to 1280x800 and zoom to 200% (Ctrl/Cmd and +): nothing is cut off or overlapped, there is no horizontal scrolling of the page, and the consent dialogs scroll if tall.
3. Turn on the OS "reduce motion" setting, restart, and open a dialog, switch views and run an activity.
4. Check form fields and the switches in both the light and dark OS themes.

**Expected:** no keyboard trap (apart from dialogs, which Esc leaves); layout survives 200% with text still readable; reduced motion removes the dialog pop, shimmer and spinner animations; input borders and the off-state switch are clearly visible in both themes.

**Result:** \_\_\_

### M11-4: No network traffic unless you start it

1. Start the app with no marketplaces added and no Companion profile. Turn on the OS firewall log, or capture traffic with Wireshark filtered to the Lullshift process.
2. Leave it idle for 10 minutes, then play a few games and content packs for 10 more.
3. Optionally, add a public marketplace and send one Companion message, then stop.

**Expected:** no connections or DNS lookups from Lullshift during steps 1 and 2 (WebView2/OS services such as certificate checks are outside the app and may appear under their own processes). Step 3 shows only the marketplace's Git host and the Companion provider.

**Result:** \_\_\_

### M11-5: Release workflow and installers

1. Run **Release** from the GitHub Actions tab (manual only). Check it creates a draft release with the Windows `.exe`, macOS `.dmg`, Linux `.deb` and AppImage, `SHA256SUMS` and provenance attestations, and nothing else.
2. Verify one download with `sha256sum -c SHA256SUMS` and `gh attestation verify <file> --repo lullshift/lullshift-app`.
3. Install each on a clean machine or VM. Note the SmartScreen / Gatekeeper warnings match the release notes.

**Expected:** the workflow never runs on push or PR; all four installers install and launch; checksums and attestations verify.

**Result:** \_\_\_

### M11-6: Uninstalling removes agent hooks

1. Windows: install the `.exe`, connect Claude Code, Codex and Cursor (back up their hook files first), add one hook of your own to each file, then uninstall from Settings > Apps.
2. Linux: same with the `.deb`, then `sudo apt remove lullshift`. Repeat with a second user account that also connected an agent.
3. macOS: confirm the user docs tell you to Disconnect before dragging to the Trash.
4. After uninstalling, run one prompt in each agent.

5. Windows: start an uninstall while Lullshift is running and choose Cancel at the "app is running" prompt; reopen Integrations.
6. Linux: run `sudo apt remove lullshift` with one user whose home is unreachable (for example an unmounted network home).
7. On a real draft release, run `gh attestation verify <file> --repo lullshift/lullshift-app --signer-workflow lullshift/lullshift-app/.github/workflows/release.yml --source-ref refs/heads/main`.

**Expected:** Lullshift's hooks are gone, your own hooks are kept, files untouched since install are restored exactly; an in-place update (Windows `/UPDATE`, `apt upgrade`) keeps the hooks; with a hook left behind by hand, Claude Code shows a non-blocking hook error and carries on (record what Codex and Cursor do); after step 5 the agents show as disconnected and can be reconnected; step 6 finishes within about 30 seconds per user; step 7 verifies.

**Result:** \_\_\_

### M11-7: Signed update (blocked until the updater key exists)

1. After the owner steps in `docs/RELEASING.md` section 3, install version N, publish a draft N+1 with `latest.json`, and choose **Check for updates**.
2. Tamper with one byte of the N+1 installer and repeat.

**Expected:** no check happens until you ask; the genuine update installs; the tampered one is refused.

**Result:** \_\_\_

## Sprint 12: Owner review changes

### M12-1: Codex, Cursor and OpenCode hooks on real installs

1. Cursor: start from an older six-hook install (connected before Sprint 12) and open Integrations. Connect again, then make a Cursor session end with an error.
2. Windows: install Lullshift into a folder whose name contains `'` and `$` (and one with `%`). Connect Codex and Cursor and run a prompt in each. Check whether either agent wraps the command in `cmd.exe /c`.
3. Windows, non-ASCII profile or install path: try `ü`, and letters such as Cyrillic `В`, `Ñ` and `Ó` (accepted since decision 27.1; see M12-10).
4. Windows PowerShell 5.1: try an install folder containing `[` or `]`.
5. OpenCode 2.x:
   1. Choose Connect OpenCode.
   2. Restart OpenCode and run a session, including one permission request and one subagent.
   3. Quit Lullshift and keep using OpenCode.
   4. Disconnect.
   5. Pre-create an unrelated `lullshift-observer.js` and try Connect.
   6. Move the app, or change `OPENCODE_CONFIG_DIR`, and check the card.
   7. Run an OpenCode 1.x instance once and note what it shows.
6. Linux AppImage: connect an agent, quit and restart Lullshift. Time one hook run. Rename the AppImage and restart. Try fish as `$SHELL`.

**Expected:**

- **Cursor (step 1):** shows as partly installed and gets the three new hooks. A session that ends with an error shows as failed.
- **Folders and paths (steps 2 to 4):**
  - The `'` and `$` folders fire hooks normally.
  - `ü` works.
  - `ü`, `В`, `Ñ` and `Ó` work.
  - `[` and `]` are refused with a clear message.
- **OpenCode (step 5):**
  - The dialog names the plugin file and says Experimental / 2.x only.
  - The card shows Unknown until the first event, then Connected.
  - A permission request shows "needs you". Subagents are not listed separately.
  - OpenCode never stalls and shows no plugin error on 2.x.
  - Disconnect deletes the file.
  - The foreign file is left alone and the card says it can't connect.
  - After the move, the card says Needs repair.
- **AppImage (step 6):**
  - Hooks use the AppImage path and stay Installed after a restart.
  - A hook run finishes in under about 2 seconds.
  - A renamed AppImage shows Needs repair.

**Result:** \_\_\_

### M12-2: Task text for Codex, Cursor and OpenCode

1. For each of Codex, Cursor and OpenCode:
   1. Turn Task context on and send two prompts.
   2. Turn it off and send another.
   3. Choose **Clear all captured text**.
2. With Companion lookups on, ask for a session overview, then turn capture off and reopen that thread.

**Expected:**

- **Step 1:**
  - Only the first prompt per session appears on the card and in the panel.
  - Nothing new is stored while capture is off.
  - Clearing removes it all.
  - Cursor's prompt still goes through.
  - Codex doesn't ask you to re-trust its hooks in `/hooks`.
- **Step 2:** the stored lookup shows the removal note.

**Result:** \_\_\_

### M12-3: Windows install, reinstall and uninstall keep hooks right

1. Connect all agents, then run the same setup again. On the reinstall page choose **Uninstall Lullshift**, then cancel the wizard.
2. Reinstall, then upgrade into a different install folder.
3. Upgrade again using the passive `/P` flag, then do one silent run.
4. Delete `lullshift.db` and reinstall.
5. Uninstall from Settings > Apps.
6. Do an in-place update (`/UPDATE`).

**Expected:**

- **Step 1:** no Lullshift hooks remain anywhere.
- **Steps 2 and 3:** hooks come back pointing at the new path, and your own hooks are intact.
- **Step 4:** nothing is restored.
- **Step 5:** hooks and records are gone, and no `hook-restore.pending` file is left in the app data folder.
- **Step 6:** hooks are untouched.

**Result:** \_\_\_

### M12-4: Welcome, skip link, dialogs and paused stats

1. On a fresh profile, press Tab once, then Enter on **Skip to main content**, then Tab again.
2. Welcome: check Skip is focused first and Esc closes the dialog. Choose **Connect an agent**. Check that the button is hidden once every agent is connected, and shown when a status can't be read.
3. With NVDA or Narrator, open each consent dialog: connect, disconnect, uninstall, delete, clear, and the Companion send preview.
4. Pause an activity that reports its own stats. Repeat in dark mode.

**Expected:**

- **Step 1:** the skip link becomes visible and moves focus into the main area. The next Tab reaches its first control.
- **Step 2:** **Connect an agent** opens Integrations listing every agent.
- **Step 3:** each dialog's title is read, then its explanation.
- **Step 4:**
  - A "From <activity name>" heading sits apart from "Played this time" and can be reached as a heading.
  - The skip link and the stat group border have enough contrast.

**Result:** \_\_\_

### M12-5: Changed games

Use the keyboard only, and check each game again after an interruption and a restart.

1. **Size buttons:**
   - Play Fog Garden at all three sizes; check the 12x12 cells are readable and focus is visible.
   - Check that the controls row stays on one line at the narrowest window in Thread Picture, Brook Links, Island Bridges, Fog Garden, Moss Paths and Firefly Catch.
2. **Tide Pairs:**
   - With a screen reader, check how cards are announced.
   - Check arrows, Home/End and PageUp/PageDown on the 4- and 6-column boards.
   - Check the reduced-motion mismatch rule.
3. **Moss Paths:** turn a crossing and undo it, at 4x4, 6x6 and 8x8.
4. **Firefly Catch:** try all four lengths, including untimed.
5. **Kite Drift:** change the pace mid-flight.
6. **Ridge Walk:** play a King on an Ace, deal again, and open an older saved game.
7. **Lantern Pond:** play it, and restore an older save.
8. **Teacup Tidy:** check the new name in the list and in help.

**Expected:**

- No crashes.
- Old saves restore, or start a fresh game cleanly.
- Untimed Firefly Catch shows no timer and no best score, and a timed best survives a restart.
- Brisk Kite Drift gaps are reachable.
- New Ridge Walk deals are instant.

**Result:** \_\_\_

### M12-6: Content packs: listening steps and pictures

1. Read Guided Pause "Near and Far" and the other rewritten packs: Notice Five, four Away packs and two Calm Cards decks.
2. Open Quick Quiz "Shapes and Sides" in light and dark themes, at a small window, and with a screen reader.
3. Delete one picture from a dev copy and open the quiz.
4. Add a real-world PNG and two WebP pictures (lossy and lossless) to a test pack and validate it.
5. Install a third-party pack with pictures, then edit one installed picture on disk and restart.
6. Try installing a pack containing a renamed SVG or an animated PNG.

**Expected:**

- **Step 1:** each step works for someone who can't hear, and the text is in British spelling.
- **Step 2:**
  - Each picture shows with its description.
  - A screen reader reads the alt text.
  - Every question can be answered without the picture.
- **Step 3:** the quiz shows "Picture not available. It shows: …".
- **Step 4:** genuine pictures are not refused.
- **Step 5:** the pack is reported as damaged ("Damaged, reinstall it").
- **Step 6:** the install is refused with a calm message.

**Result:** \_\_\_

### M12-7: Marketplaces: private hosts, moves and progress

1. Add a LAN marketplace (an IP address or `name.local`), first without ticking "This marketplace is on my own computer or network", then with it.
2. Add a public-looking name that resolves to 127.0.0.1 or a private address (for example a `nip.io` name).
3. Rename a GitHub repository you have added as a marketplace, then choose Refresh.
4. Change address to a mirror that is missing one pinned commit, then update the activity.
5. Uninstall a pack and keep its progress. Install the same id from another marketplace. Repeat after changing the first marketplace's address.
6. Run `lullshift validate` on a manifest whose name contains U+2028 or six stacked combining marks, and on a pack whose text has five stacked combining marks or a tag character.
7. Add or Refresh a marketplace whose index lists an ssh source (`ssh://git@host.example.com/o/r`).
8. Add an https marketplace on a name that doesn't resolve (for example `name.invalid`), once without a proxy and once with `HTTPS_PROXY` set.
9. Lock an installed pack's data file (open it exclusively in another program) and start the pack; then release the lock.

**Expected:**

- **Step 1:** refused without the tick. Added with it, and shows the "On your network" badge.
- **Step 2:** refused until confirmed.
- **Step 3:** a "moved on the same server" card appears, and **Use the new address** works.
- **Step 4:** Manage shows "Not found at the new address" and the activity still runs. The update clears the note.
- **Step 5:**
  - The review asks what to do with the old progress, with Delete selected and focused.
  - Keep shows a warning.
  - After the address change it still asks.
- **Step 6:** validation refuses both, and reports the field path for the pack.
- **Step 7:** refused with "an activity's source must be an https:// address or owner/repo".
- **Step 8:** without a proxy, the offline message. With the proxy, it tries through it.
- **Step 9:** "couldn't be read just now, try again" appears, Manage shows "Couldn't be read just now." and the activity stays on. Once released, it starts normally.

**Result:** \_\_\_

### M12-8: Sandbox on macOS, Linux and dev mode

1. macOS and Linux debug builds: run the sandbox probe (extends M03-4 and M03-5).
2. Windows: run `pnpm desktop:dev`, open an activity and use hot reload, then run the probe.
3. Check that a release Windows build works normally (Companion, marketplaces). After any WebView2 or Tauri upgrade, run `node tests/sandbox/run-denial-tests.mjs --attention`.

**Expected:**

- **Step 1:** every WebRTC line says blocked: on macOS "API absent" in the activity frame and in the nested frame. Every other line is still blocked.
- **Step 2:** dev mode loads and WebRTC is still blocked.
- **Step 3:** the release build works normally, and the denial tests pass 48/48.

**Result:** \_\_\_

### M12-9: Companion streaming and lookups (live providers)

1. Against Anthropic, OpenAI and a compatible server (for example Ollama), stream a reply and cancel one mid-stream.
2. With a screen reader running, stream a reply.
3. Turn on "Let the Companion look things up". Ask for a session overview, a break suggestion and help for an activity. Approve one lookup and decline another. Make a reply use all three lookups so the final no-lookup request runs. Try a server without tool support.
4. Quit the app mid-stream.
5. Send with a session summary attached, end that session, then reopen the composer.

**Expected:**

- **Step 1:** text streams, and cancelling frees the conversation at once.
- **Step 2:** you hear the status once, then the final reply once, not every word.
- **Step 3:**
  - Each lookup card shows the exact text before it is sent.
  - The break card starts an activity only when clicked.
  - The final request is accepted.
  - A server without tool support fails with a clear message.
- **Step 4:** no message is left stuck after quitting.
- **Step 5:** the remembered context falls back to "No context", with a note.

**Result:** \_\_\_

### M12-10: Decision 27 changes (folder names, refused lookups, Moss Paths)

1. Windows: use a profile or install folder containing `В`, `Ñ` or `Ó`. Connect Cursor and Codex and run a prompt in each. If you can, try one agent host that runs through the cp1252 code page.
2. Turn Companion lookups on. With a mock or real provider, make a reply ask for an unknown lookup, a malformed one, and more than three lookups. Then close and reopen the thread.
3. Moss Paths: turn a crossing tile, then press Undo.

**Expected:**

- **Step 1:** connecting works and the hooks fire.
- **Step 2:**
  - Each refused lookup shows a plain notice with no button.
  - A screen reader announces it once.
  - The notices are still there after reopening the thread.
- **Step 3:**
  - The turn is announced and Moves doesn't change.
  - Undo takes the turn back without lowering Moves.

**Result:** \_\_\_

### M12-11: Third-party programs (decision 27.5)

1. Windows: add a test marketplace that offers a code activity, and open its review.
2. Use the consent box and Install by mouse, then by keyboard only (Tab, Space). Try pressing Install before ticking the box.
3. With NVDA, read the warning, the consent box and the Install button.
4. Install it, then check the library card and the activity header.
5. Push a new commit to the test marketplace, choose Refresh and review the update.
6. Use Change address on that marketplace, then open Manage.
7. macOS and Linux: open the same review.
8. Open the activity and, from its console in a debug build, send its frame to another activity's page (`location = '/<other id>/index.html'`). Then resume it.
9. In an activity that has `activity.storage`, write in a fast burst, then write 1,025 different keys.
10. Upgrade a database from the previous release (version 13 or 14).

**Expected:**

- **Steps 1 and 2:**
  - The list says "Third-party program".
  - The review starts with "This is someone else's program" and the Windows note.
  - The publisher's description is labelled "From the publisher".
  - The box is last, before the buttons, and starts unticked.
  - Install can be focused while blocked; pressing it moves focus to the box.
- **Step 3:** the box is read with the warning text, and Install with "Tick the box above to continue."
- **Step 4:** both say "Third-party program from <marketplace>".
- **Step 5:** the warning comes back unticked, and Update stays blocked until it is ticked. The new version then runs.
- **Step 6:** Manage shows "Address changed since you accepted".
- **Step 7:** the review says the full sandbox checks haven't been run on that platform yet. If the WebRTC block failed, it shows the red alert instead.
- **Step 8:** the activity is stopped ("… was stopped") and resuming works.
- **Step 9:** the burst gets `rate_limited`, and key 1,025 gets `quota_exceeded`.
- **Step 10:** installed activities keep working and a backup is made.

**Result:** \_\_\_

### M12-12: Sandbox proof with launch keys and native IPC (all OSes)

1. Windows: run `node tests/sandbox/run-denial-tests.mjs --attention` against a debug build. Repeat it against a release build after any WebView2, Tauri or wry upgrade.
2. macOS and Linux debug builds: open the sandbox probe (extends M03-4, M03-5 and M12-8).
3. Run the probe with an installed third-party code activity, not only the bundled ones.

**Expected:**

- **Step 1:** every check passes (59 at the time of writing).
- **Step 2:**
  - Every line says blocked.
  - "native IPC channels reachable" lists `webkit.messageHandlers (ipc)`, and the forged calls are refused: Lantern Pond's saved state survives the run.
  - Scripts and stylesheets from another activity are blocked, and the control script loads.
- **Step 3:** the same results.

**Result:** \_\_\_

### M12-13: Built-in Git (decision 28)

1. Take `git` off PATH (rename `git.exe` or change PATH) and restart Lullshift. Open Marketplaces.
2. Add a public `owner/repo` marketplace, browse it, install a content pack and choose Refresh.
3. Add a private repository and an ssh `git@…` address.
4. Set `HTTPS_PROXY` (to any proxy address), restart, and add a public marketplace.
5. Add a GitHub repository that has been renamed (it redirects).
6. Put Git back and restart. Refresh the marketplace from step 2.

**Expected:**

- **Step 1:** the panel says "Using Lullshift's built-in Git (public https only)".
- **Step 2:** everything works as with installed Git.
- **Step 3:** both say that Git needs to be installed for this marketplace. The text says the server refused the request, that a public host may be limiting requests, and that Git helps only for private repositories, ssh and proxies.
- **Step 4:** the same "install Git" message, and no direct connection is made (check with a network monitor if you can).
- **Step 5:** "Use the new address" is offered.
- **Step 6:** the panel says "Using Git installed on this computer" and the marketplace still works.

**Result:** \_\_\_

### M12-14: npm packages and the publish workflow (decision 29)

Do these in order. Nothing is published until step 6.

1. Register the `lullshift` organization on npm, with 2FA, before the repository and its docs are public.
2. In GitHub, create the environment `npm`: you as required reviewer, deployment branches "Selected branches and tags" with `main` only, and `NPM_TOKEN` as an environment secret (not a repository secret).
3. Install the oldest Node 24 release you can get (24.0 if possible). Run `node scripts/check-npm-packages.mjs --keep <folder>`, install the three `.tgz` files in an empty folder, and run `npx lullshift --help`, `create`, `validate`, `simulate` and `pack`.
4. Start "Publish npm packages" from a branch other than `main`.
5. Start it from `main` with dry run on (the default).
6. Settle decision 31 (versions), then run it with dry run off and approve it.
7. After the first publish: add the trusted publisher on npm for each package (workflow `publish-npm.yml`, environment `npm`; the environment field is required for us), then delete the token.
8. On a clean machine: `corepack enable`, `pnpm --version`, and `npx @lullshift/cli@<version> --help`.

**Expected:**

- **Step 3:** every command works on that Node 24 release.
- **Step 4:** GitHub refuses the deployment to `npm`.
- **Step 5:**
  - The build job prints each tarball's hash.
  - The publish job's npm is 11.5.1 or newer, the checksum check passes, and `npm publish --dry-run` runs for each tarball.
  - The Linux smoke step's `simulate` passes.
- **Step 6:** the three packages appear on npm with a provenance badge, and npm's `dist.integrity` matches the hash in the build log.
- **Step 8:** pnpm 12.6.0 runs, and the CLI prints its help.

**Result:** \_\_\_

### M12-15: Companion model list from models.dev (decision 32)

Watch Lullshift's connections with a network monitor (Resource Monitor, Little Snitch, Wireshark or your firewall log) during steps 1 to 4.

1. Start Lullshift. Open Companion, then Settings, then **Add a provider** (Anthropic). Wait a minute.
2. Press **Load model list from models.dev**.
3. Close the form, open **Edit** on a provider, and look at the model browser. Then press **Refresh**.
4. Turn the network off and press **Refresh**. Then quit Lullshift, start it again offline, and press **Load model list from models.dev**.
5. With the list loaded: type in **Search models**, tick **Supports tools**, **Reasoning** and **Hide models older than a year**, and try each sort. Press Enter in the search box.
6. Choose a model. Save, then press **Test connection**.
7. Switch to **OpenAI-compatible**. Find a provider by part of its name (for example "router") and by its id; choose it.
8. With NVDA (or Narrator), go through the browser by keyboard only: the load button, the filters, the count and the model list.

**Expected:**

- **Step 1:** no connection to models.dev (or anywhere else) from opening settings.
- **Step 2:** exactly one HTTPS request to `models.dev`. The text next to the button says it sends one request and that keys and messages are not sent. "Loaded at HH:MM" and the provider count appear; the Anthropic models are listed with context size and price per 1M tokens ("price unknown" where models.dev has none).
- **Step 3:** reopening shows the list again with no new request. Refresh makes one request; if models.dev answers "not modified", it says "Checked at HH:MM: no changes".
- **Step 4:** an honest error that names models.dev (offline or timeout). After the failed refresh the earlier list is still shown and says when it was loaded. After the restart the list is gone (memory only) and the load fails with the same honest error; typing a model name by hand still works, and so do the suggestions.
- **Step 5:** the list and the count follow each filter; newest first is the default; Enter does not submit the form.
- **Step 6:** the model field holds exactly the chosen id; the profile saves; Test connection gives the real answer (a listed model may still be refused by your key).
- **Step 7:** the provider picker narrows as you type; choosing one shows its models. The base URL field is not filled in or changed.
- **Step 8:** every control has a name; focus is visible; the count is read out politely when it changes; each model button reads its name, id and facts, and the chosen one is announced as pressed. Nothing moves with reduced motion on.

**Result:** \_\_\_

### M12-16: Lullshift started inside a packaged app (decision 33)

Windows 11 with the Claude desktop app (a Store app) and Claude Code connected in Integrations.

1. Quit Lullshift. In the Claude desktop app, open a terminal and start Lullshift from it (for example `& "$env:LOCALAPPDATA\Programs\Lullshift\lullshift-desktop.exe"`, or the installed path).
2. Look at Sessions and Integrations.
3. Quit Lullshift from the tray. Start it from the Start menu or by double-clicking it in File Explorer.
4. In the Claude desktop app's Code tab, send a prompt in a Claude Code session. Then open Integrations.
5. Press **Fix** on the card for Claude.
6. In the Code tab, send another prompt in that session (or start a new session). Open Sessions.
7. Open Integrations again and look for the card. Check `%LOCALAPPDATA%\Packages\Claude_*\LocalCache\Roaming\app.lullshift.desktop\` in File Explorer.

**Expected:**

- **Step 2:** both views show a warning that Lullshift was started from inside another app (Claude) and agents may not reach it, saying to quit and start it from the Start menu or File Explorer. Nothing else is blocked.
- **Step 3:** no packaged warning.
- **Step 4:** the session does not appear in Sessions. Integrations shows "Agents running inside Claude can't reach Lullshift: that app has out-of-date copies of Lullshift's connection files.", what is out of date (old address, access key), and, if the hook tried after this launch, "Claude Code tried to reach Lullshift at HH:MM but couldn't (unreachable)" or "(refused)". The text says Fix removes only those copies, that open sessions may need a restart, and that data from runs inside Claude stays there.
- **Step 5:** a success message; the card goes away.
- **Step 6:** the Claude Code session appears in Sessions (restart it if it doesn't).
- **Step 7:** no card. In that folder `ipc\endpoint.json` and `ipc\token` are gone; `lullshift.db` and any other files are still there.

**Result:** \_\_\_

### M12-17: Import sessions from the agents' logs (decision 34)

Before you start: Lullshift closed. Have sessions open in Claude Code in the desktop app's **Code** tab and in a terminal (one waiting for your next prompt, one asking for a permission, one working on a long task), a Cursor agent chat, a Codex CLI session and, if you have it, an OpenCode 2.x session. Task context **off** for Claude Code at first.

1. Start Lullshift (agents connected). Open **Sessions** and watch a file monitor (Process Monitor filtered on `lullshift`) for a minute: no agent log is opened.
2. Choose **Import sessions**.
3. Check each row: agent, project folder (hover for the full path), title, last activity, guess. Compare with the real sessions.
4. Type in the filter: an agent name, part of a folder, part of a title, the first characters of a session ID.
5. Tick none: **Import (0)** is disabled. Tick one, then several, then **Select all shown** (with and without a filter). Watch the count.
6. Choose **Cancel**. Then open the dialog again, tick the Claude Code sessions and a Cursor and Codex one, and choose **Import (n)**.
7. Look at the imported sessions under **History**. Wait two minutes. Check the tray icon, notifications and a running activity.
8. Open the dialog again.
9. In the Claude Code session that was imported as waiting, send a prompt.
10. Turn Task context **on** for Claude Code, delete one imported Claude Code session, and import it again. Open its detail panel. Then **Clear captured text**.
11. With NVDA (or Narrator) and keyboard only: open the dialog, use the filter, tick rows, hear the count, import. Turn on reduced motion and repeat once.
12. Quit Cursor, rename `~/.cursor/projects` temporarily, and open the dialog.
13. If OpenCode is installed and closed (no `opencode.db-wal` beside `opencode.db`): open the dialog and look at OpenCode's data folder.
14. Open the dialog and choose **Cancel** at once, while it still says "Reading the agents' logs…". Open it again.
15. If you have a mapped network drive: point `CODEX_HOME` at a folder on it (start Lullshift from that shell) and open the dialog.

**Expected:**

- **Step 1:** nothing reads `~/.claude/projects`, `~/.cursor/projects`, `~/.codex/sessions` or OpenCode's `opencode.db` until step 2.
- **Step 2:** "Reading the agents' logs…" then the list, newest first, within about 5 seconds. Process Monitor shows each log opened for at most 64 KiB at the start and 256 KiB at the end (no full reads of large transcripts).
- **Step 3:** Claude Code sessions from the Code tab and the terminal both appear, titled with their saved name (from `/rename` or the app) or their first prompt. Guesses are plausible: the permission one "May need you" (after 30 s of quiet), the long task "Working", the idle one "Waiting for you". Cursor shows the encoded workspace folder name and is never "May need you". Sessions Lullshift already tracks say "Already in Lullshift" and can't be ticked.
- **Step 4:** the list narrows as you type and the count says "n of m shown".
- **Step 5:** the count is read politely and matches the ticks; Select all shown never ticks an "Already in Lullshift" row and leaves ticks on hidden rows alone.
- **Step 6:** Cancel imports nothing. Import closes the dialog and shows "Imported n sessions."
- **Step 7:** each imported session has a **From log** tag and a neutral pill naming its guess; none is in Needs you or Working, the summary line and nav badge don't count them, no notification or sound, the tray icon stays asleep, an activity keeps running. Task context was off: no title is stored (the row shows the tool name and short ID).
- **Step 8:** the imported sessions now say "Already in Lullshift".
- **Step 9:** within seconds the From log tag disappears and the session moves to Live like any tracked session (a later needs-you event notifies as usual).
- **Step 10:** with Task context on, the re-imported session shows its title; Clear captured text removes it.
- **Step 11:** every checkbox is read with its agent, title, folder, time and guess; focus stays in the dialog and returns to the button on close; Esc cancels. Nothing animates with reduced motion.
- **Step 12:** "No Cursor logs found on this computer." while other agents still list. Codex: same with `~/.codex` absent. OpenCode not installed: "No OpenCode logs found on this computer."
- **Step 13:** OpenCode sessions are listed, and no `opencode.db-shm` or `opencode.db-wal` file appears (the database is opened immutable). With OpenCode running (a `-wal` file exists) the list still works; SQLite may then touch the existing `-shm` file.
- **Step 14:** the scan stops; reopening reads again and never says "already reading the logs".
- **Step 15:** "Codex logs are not on a local drive, so Lullshift doesn't read them." and no log file on the network drive is opened or listed (only the folder itself is looked up, to find out where it is).

**Result:** \_\_\_

### M12-19: Settle missed and silently ended sessions (decision 35)

Before you start: Lullshift running with Claude Code connected (and, if you have them, Codex, Cursor and OpenCode). **Catch up from agent logs at startup** off. Start Lullshift from the Start menu, not from inside another app. Have Process Monitor ready, and a Claude Code session in each of: a PowerShell window, a `cmd` window, Git Bash, and the Claude desktop app's **Code** tab.

1. In each Claude Code session send a prompt, and let it finish its turn (the card says Idle or Waiting). Confirm each shows under Sessions.
2. Close the PowerShell window running Claude Code with its **X** button. Wait four minutes (two checks about 45 seconds apart, and two quiet minutes).
3. In the `cmd` session, end the program from Task Manager (**End task** on `claude.exe` or `node.exe`, the one that is the agent). Wait four minutes.
4. Quit Claude Code normally in Git Bash with `/exit` (it reports its own end), and compare with step 2.
5. Keep one Claude Code session open but idle. Wait three minutes.
6. Quit Lullshift (tray menu > Quit). Close a Claude Code session's window while Lullshift is closed. Start Lullshift.
7. In a session that Lullshift showed as Ended (process gone), resume it (`claude --resume`) and send a prompt.
8. Open the details of the session from step 2 and export it as Markdown and as JSON.
9. Open **Settings > Sessions**. Turn on **Catch up from agent logs at startup**. Leave a Claude Code session waiting, quit Lullshift, in that session send a prompt and wait for the reply to finish (so the log moves on while Lullshift is closed), then start Lullshift.
10. Watch Process Monitor (filter `lullshift`) during that start, then look at the session and at Settings > Sessions.
11. With catch-up still on, quit and start Lullshift again without doing anything in the agents. Then repeat once with `~/.claude/projects` temporarily renamed.
12. Turn catch-up off, quit and start Lullshift: confirm no agent log is opened (Process Monitor).
13. Start a stopwatch-free check of the hook: in a terminal run `echo {} | lullshift-desktop.exe hook claude-code --data-dir <data dir>`, from a PowerShell inside Windows Terminal, and read what it prints and its exit code (`$LASTEXITCODE`).
14. With NVDA (or Narrator) and keyboard only, open Settings > Sessions, change the switch, and read the status line. Check Sessions with a process-gone session in the list.
15. A session that needs you: in a Claude Code session, trigger a permission prompt and leave it unanswered, then close that window. Wait six minutes.
16. macOS only: start a Claude Code session with `TZ=Asia/Tokyo claude`, send a prompt, let the turn finish, and leave it open for more than 90 seconds with Lullshift (started normally, in your own time zone) running. Then repeat with `TZ=America/Los_Angeles`.
17. WSL, if you use it: run Claude Code inside WSL with Lullshift installed on Windows, send a prompt and leave it open for six minutes. Also run an agent inside a Flatpak, container or devcontainer on Linux if you have one.
18. Run a Claude Code session through a wrapper that exits early (for example `sudo -E claude` or `timeout 3600 claude` on Linux or macOS) and leave it open for six minutes.

**Expected:**

- **Step 1:** all four sessions are tracked as usual.
- **Step 2:** within about four minutes (the check runs every 45 seconds; it needs two gone looks and two quiet minutes) the PowerShell session moves to History as **Ended (process gone)**, with a neutral pill, no notification and no sound. Its reason line says Lullshift ended it because the program is no longer running and the agent didn't say so itself.
- **Step 3:** the `cmd` session ends the same way, including if the agent was started through `cmd` (the shell is looked through, not mistaken for the agent).
- **Step 4:** a session that reports its own end shows plain **Ended** (no "process gone" wording).
- **Step 5:** the idle session stays as it is; nothing is ended while its program runs.
- **Step 6:** after the next start, once the session has been quiet for two minutes and two checks about 45 seconds apart have found it gone, the session whose window you closed reads **Ended (process gone)**.
- **Step 7:** at the first event after the resume the session leaves Ended and shows its real state; the "process gone" wording is gone.
- **Step 8:** the Markdown says the end was inferred by Lullshift because the process is gone; the JSON has `stateSource: "inferred_process_gone"`.
- **Step 9:** the switch is off before you touch it and on afterwards; the status line says it runs at the next start.
- **Step 10:** Process Monitor shows agent logs opened for at most the start and end windows (as in M12-17), only after Lullshift started. The session now reads as a **From log** guess (for example Waiting for you) and does not notify. Settings says "Last catch-up: updated 1 session ...". No session that Lullshift did not already have appears. Your next prompt in that session replaces the guess and removes the tag.
- **Step 11:** the second start says "nothing to update". With the log folder renamed it names Claude Code as "couldn't read" or the list simply has nothing, and Lullshift starts normally either way.
- **Step 12:** no agent log is opened.
- **Step 13:** it prints nothing and `$LASTEXITCODE` is 0.
- **Step 14:** the switch has its name and state, the status line is read politely, and the Ended (process gone) pill is read in full.
- **Step 15:** the session stays **Needs you**; it is never shown as Ended (process gone), however long the window has been closed.
- **Step 16:** the session is not ended: the time zone of the agent does not matter (the process lookups pin `TZ=UTC0`). Not run on macOS yet.
- **Step 17:** a session inside WSL, Flatpak or a container is not shown as Ended (process gone) while it is alive (on Linux, a process in another PID namespace is ignored). Note what happens for WSL and report it: the sessions may stay as they were, or be ended wrongly, and the next event reopens them. Not tested yet.
- **Step 18:** the session stays alive: the wrapper (`sudo`, `timeout`) is looked through to the real program; it is never shown as Ended (process gone) while the agent runs.

### M12-18: Companion settings, models per message and reply rendering (decision 36)

Use a database from before this change (with at least one provider and a conversation) so the upgrade runs. Watch Lullshift's connections with a network monitor during steps 2 and 3. Repeat steps 4 to 9 in light and dark themes, and once with reduced motion on.

1. Start the new build. Check that the old provider still has its model (now its default) and old answers say "Model unknown". Check that a pre-upgrade backup of the database was made (as for every migration).
2. Open Companion. It shows conversations and a **Provider settings** link, and no settings tabs. Press **Provider settings**: Settings opens with the **Companion** section expanded and its heading button focused. Collapse it, restart, and check it stays collapsed; expand it, restart, and check it stays expanded. Opening Settings or Companion contacts nothing.
3. In the Companion section, press **Add a model…**. The model picker opens and loads the list from models.dev (one HTTPS request; the sentence "Sends one request to models.dev…" is visible). Close it and open it again: no new request. Press **Refresh**: one request. Offline: an honest error, and typing a name under "Use a model name not in the list" still works.
4. In the picker, use only the keyboard: Tab to the search, filters and list; arrows, Home, End, Page Up and Page Down in the list; Enter to choose; Escape to close. Focus returns to **Add a model…**.
5. Add a second model, **Make default**, then **Remove** the other one. Try removing the last model: a clear message, nothing removed.
6. In a conversation, press the model button under the message box and pick the non-default model. The button says "(this message only)". Send with **Enter**: the answer says "Answered by <that model> · <provider>", and the button goes back to the default. Send again: the default model answers.
7. Type text and press **Shift+Enter** (new line, no send) and **Ctrl+Enter** (sends). With a Japanese or Chinese input method, press Enter to confirm a word: it doesn't send.
8. Press **Options…**, choose a session summary, press **Done**. A badge shows the choice in the message box; click it to reopen the options; press its **×** to remove it.
9. In the conversation list, open **⋮** on a conversation with the keyboard (Enter, arrows, Escape) and use **Rename** and **Delete**.
10. Ask for a reply with a code block in Python, one in an unknown language and a link to `https://example.com` and to an internationalised domain (for example `https://exämple.com`). Check highlighting in both themes, **Copy** (and that "Copied" is announced), and that clicking a link shows **Open this link?** with the `xn--` form of the host. **Cancel** opens nothing; **Open in browser** opens the default browser at that exact address. A link to `http://192.168.1.1/` or `http://localhost:3000/` also shows a warning that the address is on your computer or local network; `https://example.com` doesn't.
11. With NVDA (or Narrator), go through the Companion section, the picker, the composer, the menu and a reply with a code block and a link.
12. With an OpenAI provider and a real OpenAI model (the official `openai` kind; use a test key), press **Test connection**, then send a message with streaming and, if the model supports them, one with lookups turned on and approved. Check that the answer arrives. Then send with a model name that doesn't exist: the failure shows "OpenAI said: ..." with OpenAI's reason (for example that the model doesn't exist), shown as plain text with no key in it. If your organisation is not verified for streaming and OpenAI refuses it, check that the reply arrives all at once with a note beside it (not inside the reply text), and that the next message goes out without streaming at once. Check the same on an OpenAI-compatible server (for example a local one) and with Anthropic: replies work as before, and a wrong model name shows "The provider said: ..." or "Anthropic said: ...". Check in your OpenAI usage logs that the request used the Responses API and that no stored response appears.
13. Send a message with a non-reasoning OpenAI model (for example gpt-4o or gpt-4o-mini) and with a reasoning model. Both must answer; with the non-reasoning model OpenAI may refuse the `reasoning` option once (HTTP 400), which Lullshift retries without it, and the next message to that model goes out without it at once. Check this in your OpenAI usage logs if you can.

**Expected:**

- **Step 1:** the upgrade keeps every provider and conversation; old answers show "Model unknown".
- **Steps 2 and 3:** network contact only on the first picker opening per run and on Refresh, as described.
- **Steps 4 to 9:** every control is reachable and named by keyboard; focus is visible and returns where it came from; the per-message pick is used once and then resets; Enter, Shift+Enter, Ctrl+Enter and the input method behave as described.
- **Step 10:** no formatting other than code blocks and links; nothing opens without the confirmation; the address shown is the one opened. Syntax colours are readable in both themes.
- **Step 11:** the expanded state, the menu, the picker list's active option, the "this message only" note and Copy's result are announced.
- **Steps 12 and 13:** OpenAI answers work through the Responses API; a refused request shows the provider's reason, cleaned and without the key; a streaming refusal falls back once with a note; compatible servers and Anthropic are unchanged. Non-reasoning models answer too (the `reasoning` option is dropped for them).

**Result:** \_\_\_

### M12-20: Replies continue in the background

Use a real provider with a long answer (for example "Write 400 words about locks") in two conversations A and B. Repeat steps 1 to 6 in light and dark themes, and once with reduced motion on.

1. In A, send the question. While it is writing, press another conversation B, then press A again. The text so far and **Still writing** are shown, the text keeps growing, and **Cancel** is there. Your message appears once.
2. Send in A again (after it finished), and while it writes, open Sessions, then come back to Companion and open A. The same as step 1.
3. Send in A, open B, and wait. In the list, A shows **Writing…**, then **New reply**. A notice "Companion replied in “<A's title>”" appears at the bottom right and the **Companion** nav item shows a number. The notice has no part of the reply.
4. Press **Open** in the notice (or Tab to it: it is after the page content, and focus is not moved to it when it appears). A opens, the mark, the number and the notice go away, and the full reply is shown. Do step 3 again and leave the notice alone: it goes away by itself after about 8 seconds. Point at it or Tab into it: it stays; leave it: it goes after another 8 seconds. Escape closes it.
5. Send in A and B at once (two questions). Both show **Writing…** and both finish; stacking shows at most three notices.
6. Send in A, switch to another application window (or minimise Lullshift) and wait until it is done. When you return, A shows **New reply** and a notice. With the "Let the Companion look things up" setting on, ask something that makes it look up your sessions, then open Sessions: a notice "Companion needs your approval in…" is announced at once; **Open** shows the approval card; nothing was sent to the provider before you pressed Approve.
7. Send in A, press **Cancel**, and check no notice appears. Send in A and quit Lullshift while it writes; start it again: the message is marked **Not sent** with **Try again**.
8. With NVDA (or Narrator): a finished reply in another conversation is announced politely once ("Companion replied in…"); an approval is announced assertively; the thread marks are read as words ("New reply").

**Expected:**

- **Steps 1 and 2:** leaving never stops or cancels the reply; coming back shows the live text and Cancel.
- **Step 3 and 4:** the marks, the number and the notice agree; the notice carries only the conversation name; nothing steals focus; hover or focus pauses the timer; reduced motion removes the notice's entrance animation.
- **Steps 5 to 7:** several replies at once work; a hidden window or another view gets the notice; your own Cancel is silent; a quit during a reply is shown honestly.
- **Step 8:** polite for news, assertive only for an approval.

**Result:** \_\_\_

## Owner UI review fixes (detail panel, theme, Companion lists, import)

Added with the fixes the owner found in the running app. If another batch added M12-21 or higher at the same time, renumber these when merging.

### M12-21: Session panel width and sideways scrolling

Run in light and dark, at about 900 and 1280 px wide, with a real session open that has a long captured intent (or a long unbroken text such as a URL).

1. Open a session and pin the panel (it docks at the side). Scroll the panel: nothing moves sideways, and long words and URLs wrap.
2. Drag the thin bar between the board and the panel left and right. Then press Tab until the bar has focus, and use Left and Right (16 px each), Home and End. Double-click the bar.
3. Close the app and open it again, open a session, and look at the panel width.
4. Make the window narrower than about 720 px, and unpin the panel.

**Expected:**

- **Step 1:** no horizontal scroll bar in the panel at any width.
- **Step 2:** the panel never gets narrower than about 320 px or wider than about 60% of the area; Left widens and Right narrows; the focus ring is visible; a screen reader reads a "Resize the session panel" separator with its width; double-click returns to the default.
- **Step 3:** the dragged width is remembered.
- **Step 4:** the panel floats as before and has no bar.

**Result:** \_\_\_

### M12-22: Theme setting

1. Settings > Appearance > Theme: choose **Dark**, then **Light**, then **System**, while the computer is set to the opposite theme, and again after changing the computer's theme while System is chosen.
2. With Dark chosen, start an activity, and read the Companion code blocks.
3. Restart the app.

**Expected:**

- **Steps 1 and 2:** Light and Dark win over the computer setting; System follows it, also while the app is open; the activity and code blocks use the same theme as the app.
- **Step 3:** the choice is kept.

**Result:** \_\_\_

### M12-23: Companion thread list, titles and model picker

1. In Companion, press **New conversation** and send a first message with extra spaces and a line break that is longer than 48 characters, such as "How do I fix the flaky test that only fails on CI when two workers share one cache directory?". In another new conversation, send a first message that is one very long word.
2. Rename a new conversation to "My notes", then send the first message.
3. Select threads with the mouse and with the keyboard.
4. Open Settings > Companion > Add a model, and load the model list. Resize the window to about 900 px wide. Use the search, the checkboxes and the sort.

**Expected:**

- **Step 1:** the thread is named from the message: one line, cut at about 48 characters on a word with "…", no line break. The long word is cut with "…".
- **Step 2:** the name stays "My notes".
- **Step 3:** the whole selected row has a soft background (no border on only the title), the ⋮ menu sits inside the row, and the keyboard focus ring is different from the selection.
- **Step 4:** the dialog fits (up to 640 px wide), nothing scrolls sideways, the filters wrap, and each model is a name and id on one line with the id cut by "…" when long.

**Result:** \_\_\_

### M12-24: Import sessions grouped by folder

1. Open **Import sessions** with several sessions from the same folders.
2. Tick one session in a folder, then tick **Select all in this folder**, then untick it. Do this with the keyboard (Tab, Space) too.
3. Type part of a folder name in the filter.
4. With NVDA (or Narrator), move through the headings.

**Expected:**

- **Steps 1 and 2:** folders are listed newest first, each with its name and count; the folder checkbox shows the mixed state when only some are ticked, and skips sessions already in Lullshift; the count line and **Import (n)** agree.
- **Step 3:** only matching sessions and folders stay.
- **Step 4:** each folder is a heading, and its checkbox is read as "Select all in this folder, <name>".
### M12-25: Home and the new sidebar (ADR-026 sections 1 to 3)

Run in light and dark, at about 900 px and 1,280 px wide, and once with reduced motion on.

1. Start Lullshift. It opens on **Home**, not Sessions. The sidebar reads Home, Sessions, the label **While you wait** with Play, Rest, Continue and Companion, and Settings at the bottom. There is no **Integrations** item.
2. With no agent connected and nothing running: Home says **No agents yet**, offers **Connect an agent**, and still shows a suggestion ("If you'd like a break"), the three doors and no "Pick up where you left off".
3. Connect Claude Code (Settings > Integrations). Start a long task. Home says **Your agent is working**, shows the agent as a small chip, and the suggestion line says **For a short break**. Press **Shuffle** a few times: the suggestion changes in a fixed order and comes back to the first. Close and reopen Lullshift the same day: the first suggestion is the same.
4. Let the agent ask for permission. The mascot wakes and the top card reads **Claude Code needs you** with one **Go to session** button. It opens Sessions. The orange number on Sessions is the only orange item in the sidebar; Play, Rest and Continue show quiet grey counts.
5. Pause a game (leave it, or press Pause) so it has a save. Open Home: **Pick up where you left off** shows it with **Resume**, newest first (at most four), and the suggestion is **Resume** for the same activity. Press Resume: the game starts from its save.
6. After several days of real use (at least five finished working stretches for one tool in 14 days) the line reads "Claude Code usually takes about N min". With fewer stretches it never shows a number. It never says "will finish at" and shows no countdown.
7. Stop the agent connection (quit the agent) and wait: the chip says it is not confirmed and the status never claims a task is done.
8. Keyboard only: Tab through Home. Every card, chip, button and link is reachable with a visible focus ring, and Enter or Space on a door opens it.

**Expected:**

- **Steps 1 and 4:** the layout and the single orange item match ADR-026 section 1.
- **Steps 2, 3 and 6:** only honest wording: "No agents yet", "For a short break", "usually takes about" (an estimate about the agent, never about you).
- **Step 5:** the save is resumed, not restarted.
- **Step 8:** nothing is mouse-only; with reduced motion nothing animates.

**Result:** \_\_\_

### M12-26: Play, Rest and Continue (ADR-026 section 4)

Run in light and dark.

1. Open **Play**. It lists games and puzzles (Game, Play and Focus kinds), not breathing or movement breaks. Open **Rest**: it lists breaks only.
2. On each page type a word from an activity's name, then one from its description, then one of its manifest tags (for example "breathing"). The list narrows each time; the status line says how many match. Press **Clear search**.
3. Press a filter chip (Play: Puzzles, Calm, Quizzes, Typing, Toys, Cards; Rest: Breathing, Movement, Step away, Reflection, Pause, Eyes). Only chips with at least one activity show. Press the same chip again to clear it. Chips combine with the search.
4. Install or hand-edit a test activity with an odd tag (for example `evil-tag`). It is found by searching for it but no chip or label ever shows that tag.
5. Open **How to play** on a game: the button and the dialog title say "How to play". Open **How it works** on a break: the button and the title say "How it works". Both are reachable with the keyboard, and Esc closes the dialog.
6. With a saved activity on the page, a **Continue** row shows it at the top (hidden while you search or filter). Open **Continue** in the sidebar: every saved activity, newest first, each with "saved N min ago", **Resume**, **Start over** and the help link. No counts, totals, streaks or "played for" times appear anywhere on these pages. Press **Start over** and confirm the save is gone.
7. Press **Manage activities** (quiet link, top right): Manage and Marketplaces open with the same function as before (turn an activity off and on, add a marketplace), and **Back to Play** returns.
8. Start an activity from Play, Rest, Home and Continue. Each runs in the same host. Press **Pause**, switch to Sessions, and let an agent need you: the activity pauses, the card says which tool needs you and **Go to Sessions** works.
9. Open Settings > Integrations from **Connect an agent** on Home, from the Sessions empty state, and from the welcome: each opens Settings scrolled to Integrations with its heading focused. Connect and Disconnect still ask for consent exactly as before.
10. Start game A, press **Pause**, then start a different activity B from Home (**Resume** or the suggestion) or from Continue. B starts as a fresh launch: its paused card shows only its own time and stats, and A kept its save (check A in Continue). Start A, press Pause, then **Resume** A from Home: it resumes at once, with one click, without showing the paused card.
11. While an activity runs, click **Play**, **Rest** or **Continue** in the sidebar. The activity pauses and saves, the list shows, the clicked item is highlighted, and a line at the top reads "<Name> is paused" with **Resume**. Resume brings the activity back.
12. On the paused card (and anywhere else) check there is no "In total" row or any running total of time.
13. First launch (or after clearing the stored welcome flag): choose **Connect an agent** in the welcome. The dialog closes, Settings opens and focus lands on the Integrations heading once the dialog is gone (not on the Settings heading, not on the page body). Also press **Go to session** on Home when one session needs you: Sessions opens with that session's detail panel.

**Expected:**

- **Steps 1 to 4:** the split follows the manifest category only; chips are the host's own words; third-party tags are searchable text only.
- **Steps 5 and 6:** help wording follows the activity; Continue shows no statistics.
- **Steps 7 to 9:** nothing that worked before is lost; Integrations is a section of Settings.
- **Steps 10 to 13:** each launch is clean, a sidebar click never leaves an activity running behind a list, and the welcome's Connect lands focus on Integrations.

**Result:** \_\_\_

## Tray window

Not run yet: needs a real tray on each OS. Automated coverage: placement maths, preference fallback, `open_main` validation, the capability allowlist and the tray component (`pnpm --filter @lullshift/desktop test`, `cargo test -p lullshift-desktop`).

### M12-27: Quick window from the tray (ADR-026 section 6)

Run on Windows, then macOS, then Linux, in light and dark.

1. Start Lullshift and close the main window. **Left-click** the tray icon: a small window (about 380 by 560) opens next to the icon, on screen, with no title bar and no taskbar button. Click the icon again: it closes. Open it, then click anywhere else (the desktop, another app): it closes. Open it, press **Esc**: it closes. Clicking the icon while it is open closes it and does not flash it open again.
2. **Placement.** Repeat with the taskbar at the bottom (Windows default), then top, left and right; with the icon in the overflow flyout; on a second monitor; at 150% or 200% display scale; and with two monitors at different scales (for example 100% and 200%), opening it from a tray icon on each one: the window is the same size as on a single monitor of that scale, never cut off or oversized. The window is always fully inside the screen, never under the taskbar, next to the icon. On macOS it hangs under the menu bar icon. On Linux some tray backends send no click events: left-click may do nothing there; the right-click menu must still work (see 6). While the quick window is open, bring the main window to the front (click it, or use Alt+Tab): the quick window closes. Also check with a focus lock on (for example Windows focus assist or a game in full screen) that clicking the tray icon again still closes it.
3. **Content.** With nothing running: the mascot asleep, "No agents yet" or "All quiet", one suggestion card with **Play** (or **Resume** when something is saved) and **Shuffle**, up to three chips, **Open Lullshift**, and the footer **Now / Library / Talk**. While an agent works the line reads "<Tool> usually takes about N min" or "For a short break". Let an agent need you: the **Needs you now** card appears first with **Go to session**, the mascot is awake, the tray icon is awake. Switch the app to Dark and back (Settings > Appearance): the quick window follows the next time it opens.
4. **Keyboard.** Open it with the mouse: focus is on **Go to session** when something needs you, otherwise on **Play** or **Resume**. Tab walks needs card, Play, Shuffle, chips, Open Lullshift, Now, Library, Talk, in that order, with a visible focus ring. Enter or Space activates. Esc closes from anywhere inside. With a screen reader, the title and the shuffle announcement are read.
5. **It asks, it never runs.** Press **Play** or **Resume**: the quick window closes, the main window comes to the front on Play or Rest and the activity starts there (host, pause card, sandbox as usual). Press a chip: the same. Press **Go to session**: the main window opens on Sessions with that session's panel. **Library** opens Play; **Talk** opens Companion; **Open Lullshift** opens Home. Activities never appear inside the quick window. Do this with the main window hidden (tray only), minimized and already focused.
6. **Setting and menu.** In Settings > Appearance set **Tray click opens** to **Main window**: a left click now opens the main window as before and the quick window never shows. Set it back to **Quick window**. Right-click the icon: the menu shows **Show Lullshift** and **Quit Lullshift**, unchanged, on every OS.
7. **Welcome first.** On a fresh profile (welcome not yet done) left-click the tray icon: the quick window offers only **Open Lullshift**, no suggestion, needs card or footer. **Open Lullshift** shows the main window with the welcome still open; nothing navigates behind it.
8. **Honest states.** Quit the agent connection or make the data store unreadable (rename the database while the app is closed): the quick window says "Agent status is unknown" or "Lullshift couldn't refresh just now" with **Try again**, never "all quiet" or "done". With reduced motion on in the OS, nothing in the window animates (no shimmer).
9. **Least privilege (developer tools).** In a debug build open the quick window's devtools and run `window.__TAURI_INTERNALS__.invoke("companion_threads")`, `"marketplace_list"`, `"activity_launch"` and `"set_ui_prefs"`: each is refused (not allowed). `open_main` with an unknown activity id or view is refused too. Still in the quick window's devtools, run `window.__TAURI_INTERNALS__.invoke("plugin:event|emit", {event: "open-main", payload: {view: "home"}})` and `invoke("plugin:window|show", {label: "main"})`: both are refused. Run the same two calls from the main window's devtools: also refused. Open an activity from the main window and check that the `lullact` frame loads only there (the quick window has no way to load one).

**Expected:**

- **Steps 1 and 2:** toggles, hides on blur and Esc, always fully on screen next to the icon.
- **Steps 3 and 4:** content in the order above, focus on the first useful control, a sane tab order, no motion with reduced motion.
- **Step 5:** every action opens the main window; nothing runs in the quick window.
- **Steps 6 to 8:** the setting and the menu work; the welcome is never bypassed; unknown or stale states say so.
- **Step 9:** the quick window can only read Home data and open the main window.

**Result:** \_\_\_

### M12-28: Companion chat layout, provider dialog and model picker rows

1. Open Companion with a conversation of at least ten messages (some long, one with a code block). Make the window tall, then short. Scroll the messages, then type a long multi-line message.
2. Widen the window to full screen.
3. Look at your own messages and the Companion's replies, in light and dark themes. Check the small avatars and tab to a message list, then to the message box.
4. Look under the message box. Hover the message box and the model button. Press **Options…**.
5. Send a message with **Session summary** context (preview, then send).
6. In Settings > Companion press **Edit** on a provider, then **Add a provider**. Use Tab, Shift+Tab and Esc.
7. In the provider dialog press **Choose from list…** (or **Add a model…** under a provider) and load the model list. Check a short window (about 600 px high) too.
8. Turn on reduced motion in the operating system and repeat step 6.

**Expected:**

- **Steps 1 and 2:** the message box stays at the bottom and only the messages scroll; the page behind never scrolls; the thread list scrolls on its own; the conversation is a centred column about 800 px wide, never stretched.
- **Step 3:** your messages are right-aligned bubbles (at most about three quarters of the width) with a person icon; replies are left-aligned, full width, with no card and the Lullshift mascot. There is no big uppercase "You" or "Companion"; a screen reader still reads both names. The message list shows a visible focus ring.
- **Step 4:** no help text under the box, no "Sends to ..." badge, no history sentence. The tooltip on the box says Enter sends and Shift+Enter starts a new line; the model button's tooltip shows the destination host. **Options…** says in a plain sentence how many earlier messages go along.
- **Step 5:** the exact-text preview and consent still appear before sending. Afterwards that message shows one small quiet "Shared with ...: Session summary" chip that opens the exact text sent; messages with no context show no line at all.
- **Step 6:** both open in a modal dialog (never an inline form) with the key panel in edit mode; focus stays inside, Esc closes it, and focus returns to the button that opened it.
- **Step 7:** the picker dialog has no scrollbar of its own at normal sizes; only the model list scrolls, the title, search and Cancel stay in place, and every model's second line (context size, price, date, tools) is shown in full, wrapping if needed, with the full text in a tooltip. On the short window the dialog scrolls rather than clipping.
- **Step 8:** nothing relies on animation.

**Result:** \_\_\_

## Website and demo mode

### W-1: Browser demo of the desktop UI

1. Run `pnpm --filter @lullshift/desktop exec vite --port 1420` and open <http://localhost:1420/demo.html>.
2. Visit Home, Sessions (Live and History), Play, Rest, Continue, Companion and Settings (Integrations is a section of Settings). Open `/demo.html?saved` to see saved activities on Home and Continue. Open `/tray-demo.html` (and `?saved`) for the tray window with the same made-up data, in light and dark.
3. Start a game. In the console, run `lullshiftDemo.attention("claude-code")`.
4. Open `/demo.html?welcome`.
5. Run `node apps/desktop/scripts/demo-screenshots.mjs demo-screenshots`.

**Expected:**

- **Step 2:** every screen shows made-up data and none shows an error.
- **Step 3:** the game pauses and the card says "Claude Code needs you".
- **Step 4:** the first-launch welcome shows.
- **Step 5:** 18 PNGs are written in light and dark, and none fails.

**Result:** \_\_\_

### W-2: Live website

On <https://lullshift.github.io> after the first deploy:

1. Switch the theme between light, dark and auto.
2. View the home page at phone width.
3. On the hero gallery, use the tabs with the mouse and the arrow keys, then click a screenshot. Step through the screenshots with the buttons and the arrow keys, then press Escape.
4. Search the guide.

**Expected:**

- **Step 1:** the screenshots follow the theme.
- **Step 2:** there is no sideways scrolling.
- **Step 3:** the lightbox shows all five screenshots and closes back to the one you were viewing.
- **Step 4:** the search finds guide pages.

**Result:** \_\_\_
