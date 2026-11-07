# Decisions and assumptions for product-owner review

Logged while the sprints ran unattended (from 2026-11-05 evening). Each item states what was decided and why, and what changes if you decide otherwise. I'll walk through them with you one by one.

| # | Sprint | Topic | Status (reviewed 2026-11-06) |
|---|---|---|---|
| 1 | 05 | OpenCode adapter | change: build it |
| 2 | 05 | Summary-label spoofing (L4) | change |
| 3 | 05 | Commit split | keep |
| 4 | 06 | How kits ship (ADR-019) | keep |
| 5 | 06 | ADR number for kits | keep |
| 6 | 06 | First eight games | change: port Lantern Pond |
| 7 | 06 | Engines listed in marketplace | keep |
| 8 | 06 | Game design details | change |
| 9 | 07 | Pack media | change: images |
| 10 | 07 | Content style guide | keep |
| 11 | 07 | Timed engines wait for Begin | keep |
| 12 | 07 | British spelling | keep |
| 13 | 07 | Pack data checker | change: Rust crate |
| 14 | 07 | U+2028/2029 | change: ban |
| 15 | 08 | Third-party code blocked | change: close WebRTC |
| 16 | 08 | Reading marketplace repos | keep |
| 17 | 08 | Strict marketplace URLs | change: confirm private hosts |
| 18 | 08 | Sprint 08 review choices | change: 18.1, 18.4-18.8 |
| 19 | 09 | Companion design | change: streaming, last scope, tools |
| 20 | 09 | Sprint 09 review choices | change: 20.1 |
| 21 | 10 | Sprint 10 lineup | keep |
| 22 | 10 | OpenCode | change: build it |
| 23 | 10 | Codex/Cursor choices | change: 23.2, 23.4, 23.8 |
| 24 | 10 | Content review choices | change: 24.4-24.6 |
| 25 | 11 | First-launch welcome | change |
| 26 | 11 | Release packaging | change: 26.3, 26.7, 26.8, 26.12 |
| 27 | 12 | Sprint 12 build and review choices | change: 27.1, 27.5, 27.6, 27.8 (built 2026-11-06) |
| 28 | 12 | Bundled Git fallback (gix, public https) | change: build it (built 2026-11-06) |
| 29 | 12 | Publish the creator CLI and SDK to npm | change: build it (built; Node 24+ confirmed 2026-11-07) |
| 30 | 12 | Wording in PRD and CLAUDE.md after 27.5 | applied as suggested (2026-11-07) |
| 31 | 12 | npm package versions before the first publish | open |

## 1. OpenCode adapter (Sprint 05)

You asked about Cursor, Codex and OpenCode. Cursor and Codex are in Sprint 10. OpenCode is not in the roadmap or PRD. **Assumption:** nothing changed; I did not add it. **Question:** add an OpenCode adapter to Sprint 10 next to Cursor and Codex?

**Owner decision (2026-11-06):** add an OpenCode adapter now, observe-only like Codex and Cursor, with a security review. Built after this decision review so the answers to item 23 apply to it.

**Owner follow-up (2026-11-06, after the S12-T01 research in ADR-022):** build an OpenCode adapter for OpenCode v2 only. It is marked experimental and uses the plugin fields seen in OpenCode's source. Status says "unknown" rather than "connected" until an event has arrived.

## 2. Summary labels can mimic host labels (Sprint 05, security review L4)

An activity's paused-view stat label can say "Played" or "In total", like the host's own stats. It is look-alike text only, with no access gained. **Decision:** left as is and recorded as a low risk. **Alternative:** reserve host label words, or draw activity stats in a visually separate group.

**Owner decision (2026-11-06):** change. Show activity-supplied stats in their own group, headed by the activity's name, apart from the host's stats.

## 3. Sprint 05 commits (Sprint 05)

You approved committing. I made five commits by behavior, staging individual hunks where files mixed two features. Only the final tree was fully checked (`pnpm check` all green). The intermediate commits were not built one by one. **Question:** fine, or do you want every commit to build on its own in future?

**Owner decision (2026-11-06):** fine. Commits stay split by feature, and only the final tree must pass the full check.

## 4. How kits ship (Sprint 06, ADR-019)

The architecture says kits are TypeScript libraries "compiled into the activity bundle", but bundled activities have no build step and the sandbox blocks module scripts. **Decision:** kits are written in TypeScript; a deterministic generator emits one classic-script file per kit (`lullshift-grid-kit-v1.js` and so on), committed and checked for drift by `pnpm check`. Activities copy that file in, like the bridge. **Alternative:** introduce a bundler (for example esbuild) and a build step for activities.

**Owner decision (2026-11-06):** keep the generated kit files. The pros and cons were reviewed against a bundler.

## 5. ADR number for kits (Sprint 06)

`TECHNICAL_ARCHITECTURE.md` reserved ADR-018 for app update signing (Sprint 11) and ADR-014/015 for Git and marketplaces (Sprint 08). **Decision:** kits use ADR-019 so the reserved numbers stay as planned.

**Owner decision (2026-11-06):** OK.

## 6. The first eight games (Sprint 06)

Sprint 06 needs at least eight original games across the kits, puzzle and turn-based first. **Decision:** generic mechanics with original names and art, no branded look:

| Game | Kit | Mechanic |
|---|---|---|
| Moss Paths | grid | rotate tiles so water reaches every garden bed |
| Fog Garden | grid | reveal tiles using neighbour-count hints (deduction) |
| Stone Slide | grid | sliding-tile picture puzzle |
| Petal Grid | grid | Latin-square logic puzzle, daily seed |
| Ridge Walk | card | patience: play a card one higher or lower onto the pile |
| Tide Pairs | card | memory pairs |
| Firefly Catch | arcade | move a jar to catch slow fireflies |
| Kite Drift | arcade | one-button glide through gentle gaps |

Lantern Pond stays as written (no kit) for now. **Alternatives:** different games, or porting Lantern Pond onto the grid kit.

**Owner decision (2026-11-06):** keep all eight games, and port Lantern Pond onto a kit.

## 7. Engines in the built-in marketplace (Sprint 06)

`.lullshift/marketplace.json` lists every bundled folder, including the Calm Cards engine, so content packs can name an installed engine. The app does not show engines as cards (ADR-016). **Question:** keep engines listed, or mark them differently once the marketplace UI exists (Sprint 08)?

**Owner decision (2026-11-06):** keep engines listed in the index and hidden in the app.

## 8. Game design details (Sprint 06)

Smaller choices made while building the eight games. Each is easy to change.

- **Card kit use:** Ridge Walk and Tide Pairs use the card kit's decks and seeded shuffle but draw their own layout instead of `Table`/`bindTable`, because their layouts don't fit generic piles. The card kit's table binding is tested in the kit only. Alternative: rebuild one of them on `bindTable` to exercise it in a real game.
- **Daily puzzles:** Moss Paths, Stone Slide, Petal Grid and Tide Pairs have a "today's" puzzle; Fog Garden and Ridge Walk only deal new ones.
- **Ridge Walk:** no King-to-Ace wrap; flipping a stock card is allowed at any time; every deal is checked to be clearable (about 0.25 s per new deal).
- **Fog Garden:** 9x9 with 12 brambles; hitting one reveals all and offers Try again (same field) or New field. No timer, no daily field.
- **Tide Pairs:** with reduced motion, a mismatched pair stays face up until you press another card, so the second card can be seen.
- **Moss Paths:** 6x6 only; every tile counts as a bed (36), and a crossing tile can't be turned.
- **Kite Drift:** touching a cloud puts the kite back in the middle of that gap and resets the streak; never a game over.
- **Firefly Catch:** a round is 90 seconds of game time; missed fireflies fly away, no lives.
- **Best scores:** stored with `activity.storage` where a game keeps one (Moss Paths per day, Stone Slide per size, Tide Pairs per size, Firefly Catch).

**Owner decisions (2026-11-06):**

- 8a: rebuild one card game on the card kit's table binding. Tide Pairs is the planned one.
- 8b: Ridge Walk lets a King wrap to an Ace and an Ace to a King, and drops the winnable-deal check, so deals are instant. Fog Garden gets a Size button with three sizes and a safe first reveal. The daily field keeps its opened centre.
- 8c: Moss Paths gets a Size button (4x4, 6x6, 8x8), and crossing tiles can turn. Firefly Catch gets a round-length choice: 60, 90 or 180 seconds, or untimed. Kite Drift gets a Pace button: slow, normal or brisk. Tide Pairs keeps its reduced-motion rule.
- 8d: keep the best scores.

## 9. Engine lineup and pack count (Sprint 07)

**Decision:** the nine engines named in the sprint, as `lullshift.<name>@1`: breathing (Breathing Space), pause (Guided Pause), movement (Movement Routine), away (Away Prompts), quiz (Quick Quiz), teaser (Brain Teasers), reflection (Reflection Prompts), typing (Typing Drills) and toy (Creative Toy), plus the existing Calm Cards. Five packs per new engine and four more Calm Cards packs: about 50 packs, above the 40 target. Content packs carry data and a preview only, no other media (finalised in ADR-016). **Alternatives:** fewer, deeper packs; pack media in v1.

**Owner decision (2026-11-06):** change. Allow checked images (PNG/WebP) in packs, for example quiz pictures, with size limits and image checks. This needs an ADR-016 amendment and a security review.

## 10. Content style guide (Sprint 07, `docs/CONTENT_STYLE_GUIDE.md`)

Rules I set for all bundled text: no medical or health-outcome claims and always a way out for body content; breath holds no longer than 4 seconds; no streaks or guilt; quizzes and teasers are play (no levels, grades or certificates); reflection notes stay in the pack's saved state on the device and can be cleared; no quotes from real people; plain text only. **Question:** anything to loosen or add?

**Owner decision (2026-11-06):** keep the content rules.

## 11. Timed engines wait for Begin (Sprint 07)

Breathing Space, Guided Pause and Movement Routine open ready and start when you press Begin (or Start), with Pause/Continue; after an interruption they come back stopped, with the remaining time kept, instead of carrying on by themselves. Movement never moves on to the next move without Next. Reasons: someone returning from "needs you" shouldn't find a timer already running, it meets the WCAG "timing adjustable" need, and it lets the simulator check exact restore. Typing Drills hide words per minute in accuracy mode; quiz scores are never stored. **Alternative:** auto-start on open.

**Owner decision (2026-11-06):** keep: timed engines wait for Begin.

## 12. British spelling in bundled content (Sprint 07)

The packs mixed US and British spelling. **Decision:** user-visible content uses British spelling (colour, favourite), matching the docs. The content review also removed a now-contested place name, a disputed tree-network claim, stock riddles and an implied health claim ("back-friendly" became "Seated Easy Stretches"). **Alternative:** US spelling throughout.

**Owner decision (2026-11-06):** keep British spelling.

## 13. How the app checks pack data (Sprint 07, ADR-016)

The app must re-check a pack's data against its engine schema before running it. Ajv (used by the CLI) compiles validators with `new Function`, which the app's CSP forbids. **Decision:** a small Rust validator (`data_schema.rs`) that supports a fixed set of schema keywords and refuses schemas using anything else; it uses the existing `regex` crate (already in the lockfile, now a direct dependency). Other choices made with it: a pack gets `activity.storage` only if both the pack and its engine declare it; v1 packs may contain only the manifest, the data file, a preview and `.md`/`.txt` notes; engines stay listed in the marketplace index but hidden in the app; no engine major is removed within v1. **Alternatives:** relax the CSP for Ajv (not recommended), or use a Rust JSON Schema crate (bigger dependency).

**Owner decision (2026-11-06):** change. Replace the custom Rust checker with an established Rust JSON Schema crate. The CSP is not loosened. The crate needs a dependency audit and must pass the shared Ajv/Rust test corpus.

## 14. Security review follow-ups left for Sprint 08 (Sprint 07)

The review of the pack checker found no blocking issues. Medium and low items (regex differences, a work budget, exact `multipleOf`, a shared Ajv/Rust test corpus, cleaned terminal output) are fixed with tests in Sprint 07. Two items are deliberately left: (a) the app caches a pack's check result for the session, which is safe while packs are bundled; Sprint 08 must key the cache by installed commit SHA or content hash when marketplace installs land; (b) the shared plain-text pattern still allows the Unicode line and paragraph separators (U+2028/U+2029). Both validators agree, so nothing slips through one side only; banning them means changing every engine schema at once. **Question:** ban them in a later schema pass, or leave as is?

**Owner decision (2026-11-06):** (a) cache keying was handled in Sprint 08. (b) Ban U+2028 and U+2029 in every engine schema in one pass.

## 15. Third-party code activities stay blocked on every OS (Sprint 08, ADR-015)

Sprint 08 says third-party code is enabled only where the Sprint 03 sandbox proof passed. Windows passed 42/42 but with the open WebRTC gap (an activity can open a peer connection, a possible way to send data out), and macOS and Linux have not been run (M03-5). **Decision:** third-party `kind: app` activities can be browsed and reviewed but not installed on any OS in this release; one constant per OS controls it. Third-party content packs can be installed. **Question:** keep blocked until WebRTC is closed, or allow on Windows now?

**Owner decision (2026-11-06):** try to close the WebRTC gap now, on every OS. Third-party code activities stay blocked until it is proven closed and the macOS and Linux sandbox checks pass.

## 16. How Lullshift reads marketplace repos (Sprint 08, ADR-015)

**Decisions:** Lullshift clones into a bare cache with no checkout and reads files from the pinned commit with `git ls-tree`/`cat-file`, then writes the files itself after checking them. So Git never writes repository files, `.gitattributes` filters and LFS never run, and symlinks are never created. Executable bits, symlinks and submodules in a pack are refused. Limits per pack: 512 files, 5 MiB per file, 20 MiB in all. Updates are only checked when you press Refresh. Removing a marketplace keeps the activities you installed from it (they are pinned copies) but stops their updates. Uninstalling keeps saved state unless you tick "also delete saved progress". URLs with a password or token in them are refused, and loopback/LAN hosts are refused. **Alternatives:** a regular checkout; auto-check for updates on launch (a network call the user did not start, so not chosen).

**Owner decision (2026-11-06):** keep.

## 17. Marketplace URLs are strict (Sprint 08, ADR-015 implementation notes)

**Decision:** a marketplace URL must use a DNS host name with at least two labels (like `git.example.com`). IP addresses, `localhost`, single-word host names and suffixes such as `.local`, `.lan`, `.internal`, `.corp` and `.home` are refused, and so are `%`, `=` and non-ASCII characters. This closes off pointing Lullshift at services on your own machine or network, but it also blocks self-hosted Git servers reached by IP or by an internal-only name. Non-ASCII file names in packs are refused too. **Alternative:** allow private hosts after an extra "this is on your network" confirmation.

**Owner decision (2026-11-06):** change. Allow private hosts (IP addresses, localhost, internal names) only after an extra "this is on your own network" confirmation that the user ticks.

## 18. Sprint 08 security review fixes: conservative choices taken (ADR-015)

Taken while the product owner was away; all fail closed. Please confirm or change each.

1. **Redirects are off** (`http.followRedirects=false`). A repository that moved (for example a renamed GitHub repo) now fails and must be re-added at its new address.
2. **Git's own object check is on** for every fetch (`transfer.fsckObjects`, `fetch.fsckObjects`). A repository with a malformed tree is refused at fetch with a generic error, before our own pack validation runs.
3. **Fetch cap is 200 MiB of new objects per fetch.** A new cache is fetched into a staging folder and moved into place only on success. An existing cache is fetched into in place (not swapped), because older commits that installs are pinned to live in it; only temporary pack files are removed after a failure, and the fetched ref moves only on success.
4. **Re-adding a removed marketplace name from another address is refused** (`alreadyExists` with a message). There is no "replace source" flow.
5. **Installed files are served from the bytes hashed at verification**, held in memory for the session (a pack keeps only its preview image and its checked data; an app, which can't be installed from third parties in this release, keeps all files, at most 20 MiB). Chosen over re-hashing on every request, which would read the whole pack on each preview.
6. **Saved state follows its source.** Uninstalling with "keep saved state" records the marketplace and source (migration 0007). Installing the id from a different marketplace or source deletes that state at install; the review does not promise it. State with no recorded owner (older than 0007) is kept and the review says "Saved progress from an earlier install of this id will be used." Deleting unowned state would lose user data on a guess.
7. **Hidden-character set widened for manifests too.** Manifest text and index text share one check, so the host is now stricter than the JSON schema and the SDK's `isPlainText` (which still accept U+2028/2029, filler characters and long mark runs). Failing closed here is intended; the schema and SDK can be aligned later.
8. **The "no local hosts" URL rule stays text-only.** DNS names can still resolve to local addresses; noted in ADR-015.

**Owner decisions (2026-11-06):**

- 18.1: change. Follow redirects only when they stay on the same host. Git has no same-host option, so this needs custom handling: detect the redirect, then refetch from the new address on the same host. Recorded in ADR-015.
- 18.2 and 18.3: keep.
- 18.4: change. Add an explicit "change address" flow that shows the old and new addresses and asks for confirmation.
- 18.5: change. Re-check the files on each load instead of keeping the checked bytes in memory.
- 18.6: change. When existing saved progress belongs to another source, the install screen asks whether to delete it or keep it.
- 18.7: align the schema, the SDK `isPlainText` and the CLI with the host's stricter hidden-character check.
- 18.8: also check the resolved address. A public name that resolves to a private address gets the same confirmation as decision 17.

## 19. Companion providers and boundaries (Sprint 09, ADR-020)

**Decisions:** three provider kinds: Anthropic, OpenAI and "OpenAI-compatible" (any https base URL, or http for a local server on this computer, so local models work). You type the model name; suggestions are only hints. Keys live in the OS keychain only, are sent from the UI once and never shown again (not even the last four characters); if the keychain is unavailable, Companion refuses to store a key anywhere else. Replies are not streamed in v1, never auto-retried, and shown as plain text with no clickable links. Each message has its own context choice, defaulting to "No context", with a preview of the exact text before sending; the choice resets after each message, and what was sent is kept with the message. Budgets are a monthly token limit per provider, counted from the usage the provider reports. Requests carry no tools; a fixed one-line system prompt says it is a chat companion that cannot act. **Questions:** add streaming? allow keeping the last-used context scope?

**Owner decisions (2026-11-06):**

- 19a: add streaming replies. Each chunk is cleaned, and cancel and budget accounting cover partial replies.
- 19b: remember the last context choice. The preview is still shown before every send.
- 19c: keep the providers and boundaries, and add three read-only Companion tools. Each tool call is previewed and approved before its result goes to the provider:
  - **Session overview:** state and time in state, with no task text unless that opt-in is on.
  - **Suggest a break:** returns a card that the user clicks to start an activity; the tool never launches anything itself.
  - **Activity help lookup:** reads installed activities' help and controls.

  Nothing else: no files, shell, web or agent control. This changes the "no tools" rule in ADR-020, so it needs an ADR amendment and a security review.

## 20. Sprint 09 security review fixes: conservative choices taken (ADR-020)

Taken while the product owner was away; all fail closed. Please confirm or change each.

1. **Cancel keeps the thread busy until the HTTP worker ends** (up to the 60 s timeout). Sending, retrying and deleting that thread return "busy" meanwhile. A cancelled call is counted in usage: real numbers if a reply arrives, otherwise an estimate from the request size (so a cancelled call that never reached the provider is still counted; this errs towards the budget). The usage is queued by the worker and written to the store by the next Companion command that checks a budget or reads usage, so it is lost only if the app quits before then.
2. **Preview hash.** Send must carry the SHA-256 of the previewed context; any difference (or a hash given for "no context") refuses the send and shows a new preview.
3. **History never follows a message to another host.** Each stored message records the host it went to or came from. A send includes only earlier messages for the same host; the rest are left out, not refused, and the composer says how many earlier messages go along. Chosen over "refuse and ask for a new conversation" because it keeps changing a base URL usable while never replaying text to a host it was not sent to. Messages with no recorded host are never replayed. Cost: after changing providers the model may lack earlier context. The `host` column is added by migration 0009 (0008 is unchanged, because it already ran on a real database).
4. **Retry is refused if the profile's host changed** since the message was written ("Send the message again to review it").
5. **Reply cleaning uses `agent_core::text::is_invisible`**, which is wider than the list asked for (it also removes e.g. U+034F, U+2060-206F, Mongolian free variation selectors). `companion` now depends on `agent-core`.
6. **Keychain at launch.** The keyring API in use has no metadata-only check, so `hasKey` still reads the secret. Instead the Companion view is no longer mounted at app launch; it mounts on first visit, so the keychain is not read until then.

**Owner decisions (2026-11-06):**

- 20.1: change. Cancel frees the thread at once, and a late reply is discarded. Assumption: the cancelled call's usage is still counted towards the budget.
- 20.2 to 20.6: keep.

## 21. Sprint 10 lineup: eleven new games and 21 packs

**Decision:** to reach the 20-game target, eleven original games on the kits: Tea Sort (pour colour layers until each cup is one colour), Pebble Merge (slide and merge pebbles on a 4x4 tray), Sea Glass (swap to make lines of three, 25 swaps a round), Thread Picture (picture-by-numbers), Brook Links (link matching stones with streams that don't cross), Island Bridges (connect numbered islands), Harvest Pairs (pairs that add to 13 from a triangle of cards), Clock Patience (a calm game of chance), Frost Steps (slide on ice to the lantern), Breeze Bounce (keep a seed aloft, no lives) and Glow Trail (a trail that shortens instead of ending the game). All grid puzzles have a daily puzzle (S10-T03). Plus 21 content packs (two per engine, three for Calm Cards) to bring activities to about 100. Mechanics are generic; names, art and wording are original, and user-facing text avoids product names of well-known games. **Question:** any of these you'd rather drop or replace?

**Owner decision (2026-11-06):** keep all eleven games and 21 packs as they are.

## 22. OpenCode still not added (Sprint 10)

Decision 1 is still open, so Sprint 10 researches and builds only Codex and Cursor adapters, as the sprint says.

**Owner decision (2026-11-06):** add an OpenCode adapter now, observe-only like Codex and Cursor, with a security review. Built after this decision review so the answers to item 23 apply to it.

**Owner follow-up (2026-11-06, after the S12-T01 research in ADR-022):** build an OpenCode adapter for OpenCode v2 only. It is marked experimental and uses the plugin fields seen in OpenCode's source. Status says "unknown" rather than "connected" until an event has arrived.

## 23. Codex and Cursor adapters: conservative choices taken (ADR-021)

Taken while the product owner was away; all observe-only. Please confirm or change each.

1. **Codex Interrupt maps to `turn_ended`** (reason `interrupted`), not `cancelled`: `cancelled` is terminal and the Codex session continues.
2. **Cursor `stop` with status `error` is still `turn_ended`** (status kept as a hint), not `failed`, so no "stopped with an error" notification comes from a single hook. Cursor never claims `needs_attention`.
3. **Cursor payloads reaching the Claude Code hook are dropped, not re-mapped to `cursor`.** Their shape is undocumented, and Cursor's own hooks cover it.
4. **Command string, not exec form.** Codex and Cursor take a command string (Codex's `args` is undocumented). Paths are single-quoted and limited to letters, digits, space and `_-./\:()@+=,~`; others refuse install. An app folder with an apostrophe, `$`, `&` etc. in its name can't be connected. On Windows both use the PowerShell form (`& '...'`); Codex's Windows shell is undocumented.
5. **Timeouts:** Codex 2 s for async hooks, SessionEnd (synchronous) 3 s; Cursor 3 s.
6. **Codex "hooks turned off"** is read only from `[features] hooks = false` (or top-level `features.hooks = false`). The deprecated `codex_hooks` alias is not read. Status shows "Hooks turned off" only when Lullshift's hooks are installed or partly installed; with nothing installed it stays "Not installed".
7. **Cursor `version` other than 1 is refused** (file left alone).
8. No task-text capture for Codex or Cursor (capture stays Claude Code only).
9. **Stricter hook-file parsing (all three adapters, after the security review):** files with duplicate JSON keys, integers beyond 64 bits, or any number of 9e18 or more (for example `1e30`) are refused and left untouched rather than rewritten lossily. Files over 1 MiB or that are not regular files are refused too. A relative `CODEX_HOME` is ignored.

**Owner decisions (2026-11-06), item 23:**

- 23.1: keep. Codex Interrupt stays `turn_ended`.
- 23.2: change, after a re-check of the Cursor hook docs on 2026-11-06. `sessionEnd` with reason `error` maps to `failed`. The error message is never read. `stop` with status `error` stays `turn_ended`. "Needs you" is still not available for Cursor: Notification and PermissionRequest are documented as unsupported.
- 23.2b: change. Register the non-blocking hooks `postToolUseFailure`, `subagentStop` and `preCompact` as `working` signals. A tool failure doesn't fail the session.
- 23.3: keep. Cursor payloads that reach the Claude Code hook are dropped.
- 23.4: change. Escape the app path for each shell (PowerShell on Windows, POSIX sh elsewhere) instead of allowing only a fixed set of characters. Assumption: control characters (newlines, NUL) still refuse install, because no shell quoting handles them safely. Needs tests and a security review.
- 23.5: keep (2 s / 3 s / 3 s).
- 23.6: keep. Only `[features] hooks = false` is read.
- 23.7: keep. Cursor `version` other than 1 is refused.
- 23.8: change. Extend the existing Claude Code task-text opt-in (off by default, stored locally) to Codex and Cursor prompts. Needs a privacy review.
- 23.9: keep. Unusual hook files are refused and left untouched.

## 24. Sprint 10 content review: choices taken

Taken while the product owner was away. Please confirm or change each.

1. **Tea Sort's sixth tea is now "Sencha"** (was "Matcha"; its pattern also rendered as a solid fill and is now a visible grid), so every tea has its own first letter, as the help promises.
2. **Island Bridges marks finished islands with a double ring** as well as green, so colour is not the only cue.
3. **Fog Garden got a daily field** ("Today's field"): today's seed with the centre tile opened for you, so everyone gets the same field. This adds a fourth button to its controls row.
4. **Size stays a separate `<select>` row** in Thread Picture, Brook Links and Island Bridges. Petal Grid and Stone Slide put a "Size" button in the main controls row instead. This is left inconsistent for now; unifying it is a small follow-up if you want one pattern.
5. **Name clash not checked.** "Tea Sort" describes a common sort-by-pouring mechanic and may be close to existing app-store titles. No web or store search was done; please check before release, or pick another name (for example "Teacup Tidy").
6. **Listening prompts** (Pause: Sounds around you and similar) assume hearing; each pack says any step can be skipped. Left as is.

**Owner decisions (2026-11-06), item 24:**

- 24.1 to 24.3: keep.
- 24.4: change. Thread Picture, Brook Links and Island Bridges use a "Size" button in the main controls row, like Petal Grid and Stone Slide.
- 24.5: change. Rename "Tea Sort" to "Teacup Tidy". A web search on 2026-11-06 found an App Store game named exactly "Tea Sort" (BilSoft) with the same mechanic. No match was found for "Teacup Tidy".
- 24.6: change. Rewrite the listening steps so each can be done by hearing or by touch and sight.

## 25. First-launch welcome (S11-T03)

Taken while the product owner was away. Please confirm or change each.

1. **A one-time welcome dialog** on first launch: no account, local only, works offline. Three choices: Skip for now (focused first; Esc does the same), Try a suggested break activity, Set up Claude Code. Any choice marks it done in the existing UI preferences (no new table).
2. **Only Claude Code is offered** in the welcome; Codex and Cursor stay in Integrations. "Set up" only opens Integrations, so the existing consent dialogs (exact file path, backup, OS confirmation) still apply.
3. **Existing installs see it once too** (including yours), since it is pre-release. It is not shown when the preferences can't be read.
4. **No skip link** was added (five-item nav plus landmarks); consent dialogs have no `aria-describedby` yet. Both are small follow-ups if you want them.

**Owner decisions (2026-11-06), item 25:**

- 25.1 to 25.3: change. Replace "Set up Claude Code" with "Connect an agent", which opens Integrations listing every supported agent: Claude Code, Codex, Cursor, and OpenCode once built.
- 25.4: do both. Add a "Skip to main content" link, and link each consent dialog's explanation with `aria-describedby`.
- 26.12, which also covers the welcome: hide the connect button when it's not needed. Assumption: it is hidden only when every supported agent is already connected.

## 26. Release packaging (S11-T01, S11-T06)

Taken while the product owner was away. Please confirm or change each. Details in `docs/RELEASING.md`.

1. **ADR-004 amended** (not a new ADR) for the manual-only release workflow; ADR-018 stays reserved for update signing and key custody.
2. **Plain `pnpm tauri build`, not tauri-action**, so build jobs stay read-only and only the final job can write the draft release.
3. **Updater not added yet.** No key exists, so the plugin is not registered and no check can happen. When you add the key, the check must be a manual "Check for updates" button only (a test enforces no background check settings).
4. **Windows WebView2: `embedBootstrapper`.** If WebView2 is missing, the installer downloads it from Microsoft. Alternative: `offlineInstaller` (+127 MB) for offline installs.
5. **Windows installs are per-user** (`currentUser`), which the uninstall hook relies on.
6. **macOS builds are ad-hoc signed** (`signingIdentity "-"`); Gatekeeper still warns. Unsigned is the alternative.
7. **Uninstall removes hooks** on Windows (NSIS, skipped during in-place updates) and Linux `.deb` (prerm, on remove only, run as each regular user). macOS and AppImage have no uninstaller: Disconnect first. AppImage hooks point at a temporary mount path; the docs could recommend the `.deb` for agent hooks.
8. **A fresh reinstall over an existing install** removes the hooks (the old uninstaller runs without `/UPDATE`); reconnect afterwards.
9. **First version number** for the draft release (0.1.0 now) is yours to choose. No MSI or rpm targets.
10. **SECURITY.md** points reporters to GitHub private vulnerability reporting, which you need to turn on for the repository.
11. **After the release security review:** the draft-release job only runs from `main`, and the documented provenance check pins the workflow and `refs/heads/main`. The updater signing secret is not passed to builds until the updater is enabled; when it is, keep it in a `release` environment limited to `main` with you as reviewer.
12. (Decision 25 follow-up) The welcome offers "Set up Claude Code" even when Claude Code is already connected; it then just opens Integrations. Hiding the button in that case is a small follow-up if you want it.

**Owner decisions (2026-11-06), item 26:**

- 26.1 and 26.2: keep. The workflow is manual-only, uses plain `pnpm tauri build`, and its build jobs are read-only.
- 26.3: change. Once the updater exists, add an opt-in setting, off by default, that checks for updates once a day. The manual button stays. Turning the setting on counts as the user starting the network use (PRD: app update checks are user-initiated). ADR-018 should record this.
- 26.4: keep `embedBootstrapper`.
- 26.5: keep per-user installs.
- 26.6: keep ad-hoc signing on macOS for v1.
- 26.7: change. AppImage hooks should point at the AppImage file itself (`$APPIMAGE`), not the temporary mount, so they survive restarts.
- 26.8: change. A reinstall over an existing Windows install should keep the agent hooks, not remove them.
- 26.9: keep 0.1.0 for the first release. No MSI or rpm.
- 26.10 and 26.11: keep. The owner turns on private vulnerability reporting, and later creates the protected `release` environment.
- 26.12: change. See item 25.

## 27. Sprint 12: choices made while building the owner's changes

These were taken during Sprint 12, mostly to fix security review findings. Please confirm or change each.

1. **Some Windows profile paths can't connect Codex or Cursor.** In the PowerShell form, Lullshift refuses a path whose UTF-8 bytes include 0x82 or 0x91 to 0x94, for example Cyrillic В, Ђ, Â, Ñ, Ò, Ó or Ô. A host that reads the command in the legacy cp1252 code page would turn those bytes into quote characters. That could break the command, or in a crafted case run something else. Claude Code is unaffected. **Alternative:** convert the path to its ASCII 8.3 short name, where Windows provides one (ADR-021).
2. **Reinstall restores hooks instead of keeping them** (refines 26.8). Every uninstall except an in-app update removes the hooks. After a reinstall, they are re-applied from the stored install records, but only for agents you connected before and only within an hour. A reinstall after deleting the database restores nothing (ADR-010).
3. **Index sources must be https.** A marketplace index can no longer list ssh sources for its activities, because ssh can't be pinned against DNS rebinding. You can still type an ssh address for a marketplace yourself (ADR-015).
4. **No fetch when the name can't be looked up.** If a marketplace name doesn't resolve and no proxy is configured, Lullshift refuses the fetch as offline instead of fetching unchecked. Behind a proxy there is no address check (ADR-015).
5. **Windows code gate needs a runtime proof.** Third-party code activities now also need WebRTC to be confirmed off at run time. On Windows, the off state is set by browser arguments and can't be read back yet, so Windows stays closed even if its constant is flipped (ADR-012).
6. **Companion lookups are off by default.** Lookups that are unknown, malformed or over the limit get a fixed sentence without asking you, because there is nothing to approve. On the last round, earlier lookups are replayed as plain text (ADR-020).
7. **Pack pictures: some WebP trailing data can't be detected.** Trailing bytes inside a lossy VP8 chunk, or up to 8 bytes in a lossless VP8L chunk, can't be detected with the decoder in use. PNG is checked exactly (ADR-016).
8. **Moss Paths crossings** can be turned. A turn changes nothing visually, but counts as a move, can be undone, and is announced.
9. **The Welcome's "all connected" check ignores OpenCode**, because OpenCode is experimental.

**Owner decisions (2026-11-06), item 27:**

- 27.1: change. Allow Windows paths containing the affected letters for Codex and Cursor, and accept the risk. Document that hooks may not work on these paths, and that a host reading the command as cp1252 could misparse it.
- 27.2: OK.
- 27.3: OK.
- 27.4: OK.
- 27.5: change. Open third-party code activities on all platforms. Each install shows a clear risk warning and asks for consent; there is no extra setting on macOS or Linux. This changes ADR-012 and ADR-015 and the CLAUDE.md rule "only where the sandbox proof passed". The rule changes only through an ADR amendment, and the sandbox itself is not weakened. Claude recommended an extra one-time setting on macOS and Linux until M12-8 passes; the owner chose a consent warning only.
- 27.6: change. Show a small notice in the conversation, with no approve button, when the Companion asks for a lookup that is unknown, malformed or over the limit.
- 27.7: accept, documented.
- 27.8: change. Turning a Moss Paths crossing is announced and undoable but doesn't count as a move.
- 27.9: OK.

## 28. Bundled Git fallback (owner request, 2026-11-06)

**Owner decision:** when Git isn't installed on the computer, Lullshift falls back to a bundled Git library, so marketplaces still work.
- **Library:** gitoxide (`gix`), which is pure Rust. libgit2 was considered and rejected.
- **Scope:** public https marketplaces only. Private and ssh marketplaces still need Git installed, and the app says so.
- **Hardening:** the same as with installed Git: the address pin, no redirects, object checks and the size cap.
- **Installed Git comes first** whenever it is available.
- **Record:** ADR-023.

## 29. Publish the creator CLI and SDK to npm (owner decision, 2026-11-06)

**Owner decision:** publish `@lullshift/cli` and `@lullshift/activity-sdk` to npm, with `@lullshift/agent-protocol`, which the SDK needs. The CLI is not bundled with the desktop installer.
- **Build:** the packages are compiled to JavaScript before publishing, because Node does not run TypeScript from `node_modules`.
- **Release:** a manual GitHub workflow publishes with npm provenance. It uses a protected `npm` environment with the owner as required reviewer, and the token is held only as that environment's secret. Nothing is published without the owner's approval.
- **Owner steps:** register the `@lullshift` scope on npm, create a granular publish token and approve each publish run.
- **Record:** an amendment to ADR-017.
- **Node version (owner decision, 2026-11-07):** the published packages require Node 24 or newer, the version this repo develops and tests on. A Node 22.12 floor was proposed and declined.

## 30. Wording in the PRD and CLAUDE.md after decision 27.5 (open, for the owner)

Decision 27.5 opened third-party code activities on every platform after a warning and consent. Two documents I may not change on my own still say otherwise or say nothing:

- **`docs/PRODUCT_REQUIREMENTS.md` line 252** says the sandbox "must be proven per OS before code activities are installable". It is now out of date. Suggested change: "Third-party code activities are installable on every platform after the review shows a risk warning and the user ticks a consent box, at each install and each update that changes the code. The per-OS sandbox proof is still run, and its result is shown in the warning."
- **`CLAUDE.md`, under "Activities and creators"**, suggested new line: "Third-party code activities (`kind: app` from non-built-in marketplaces) are installable on all platforms only after the review's risk warning and explicit consent at each install and each code-changing update (owner decision 27.5, ADR-015 amendment). The backend must refuse them without `acceptCodeRisk: true`. Never weaken the sandbox, CSP, WebRTC block or permissions model to make third-party code work, and never describe the sandbox as guaranteed."

**Owner decision (2026-11-07):** apply both as written. Done.

## 31. npm package versions before the first publish (open, for the owner)

The CLI package is version 0.1.0, but `lullshift --version` prints the SDK's version, 0.2.0, and the creator docs call the CLI 0.2.0. Once a version is on npm it can't be reused, so this should be settled before the first publish.

- **Option A (suggested):** publish all three packages at 0.2.0, so the CLI, the SDK and `--version` agree.
- **Option B:** keep separate versions, and make `--version` print the CLI's own version.
