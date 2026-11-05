# Decisions and assumptions for product-owner review

Logged while the sprints ran unattended (from 2026-11-05 evening). Each item states what was decided and why, and what changes if you decide otherwise. I'll walk through them with you one by one.

| # | Sprint | Topic | Status |
|---|---|---|---|
| 1 | 05 | OpenCode adapter | open |
| 2 | 05 | Low-risk summary-label spoofing (L4) left unfixed | open |
| 3 | 05 | Sprint 05 commit split | open |
| 4 | 06 | How kits ship (ADR-019) | open |
| 5 | 06 | ADR number for kits | open |
| 6 | 06 | The first eight games | open |
| 7 | 06 | Built-in marketplace lists engines | open |
| 8 | 06 | Game design details chosen by the game builders | open |
| 9 | 07 | Engine lineup and pack count | open |
| 10 | 07 | Content style guide rules | open |
| 11 | 07 | Timed engines wait for Begin | open |
| 12 | 07 | British spelling in bundled content | open |
| 13 | 07 | How the app checks pack data | open |

## 1. OpenCode adapter (Sprint 05)

You asked about Cursor, Codex and OpenCode. Cursor and Codex are in Sprint 10. OpenCode is not in the roadmap or PRD. **Assumption:** nothing changed; I did not add it. **Question:** add an OpenCode adapter to Sprint 10 next to Cursor and Codex?

## 2. Summary labels can mimic host labels (Sprint 05, security review L4)

An activity's paused-view stat label can say "Played" or "In total", like the host's own stats. It is look-alike text only, with no access gained. **Decision:** left as is and recorded as a low risk. **Alternative:** reserve host label words, or draw activity stats in a visually separate group.

## 3. Sprint 05 commits (Sprint 05)

You approved committing. I made five commits by behavior, staging individual hunks where files mixed two features. Only the final tree was fully checked (`pnpm check` all green). The intermediate commits were not built one by one. **Question:** fine, or do you want every commit to build on its own in future?

## 4. How kits ship (Sprint 06, ADR-019)

The architecture says kits are TypeScript libraries "compiled into the activity bundle", but bundled activities have no build step and the sandbox blocks module scripts. **Decision:** kits are written in TypeScript; a deterministic generator emits one classic-script file per kit (`lullshift-grid-kit-v1.js` and so on), committed and checked for drift by `pnpm check`. Activities copy that file in, like the bridge. **Alternative:** introduce a bundler (for example esbuild) and a build step for activities.

## 5. ADR number for kits (Sprint 06)

`TECHNICAL_ARCHITECTURE.md` reserved ADR-018 for app update signing (Sprint 11) and ADR-014/015 for Git and marketplaces (Sprint 08). **Decision:** kits use ADR-019 so the reserved numbers stay as planned.

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

## 7. Engines in the built-in marketplace (Sprint 06)

`.lullshift/marketplace.json` lists every bundled folder, including the Calm Cards engine, so content packs can name an installed engine. The app does not show engines as cards (ADR-016). **Question:** keep engines listed, or mark them differently once the marketplace UI exists (Sprint 08)?

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

## 9. Engine lineup and pack count (Sprint 07)

**Decision:** the nine engines named in the sprint, as `lullshift.<name>@1`: breathing (Breathing Space), pause (Guided Pause), movement (Movement Routine), away (Away Prompts), quiz (Quick Quiz), teaser (Brain Teasers), reflection (Reflection Prompts), typing (Typing Drills) and toy (Creative Toy), plus the existing Calm Cards. Five packs per new engine and four more Calm Cards packs: about 50 packs, above the 40 target. Content packs carry data and a preview only, no other media (finalised in ADR-016). **Alternatives:** fewer, deeper packs; pack media in v1.

## 10. Content style guide (Sprint 07, `docs/CONTENT_STYLE_GUIDE.md`)

Rules I set for all bundled text: no medical or health-outcome claims and always a way out for body content; breath holds no longer than 4 seconds; no streaks or guilt; quizzes and teasers are play (no levels, grades or certificates); reflection notes stay in the pack's saved state on the device and can be cleared; no quotes from real people; plain text only. **Question:** anything to loosen or add?

## 11. Timed engines wait for Begin (Sprint 07)

Breathing Space, Guided Pause and Movement Routine open ready and start when you press Begin (or Start), with Pause/Continue; after an interruption they come back stopped, with the remaining time kept, instead of carrying on by themselves. Movement never moves on to the next move without Next. Reasons: someone returning from "needs you" shouldn't find a timer already running, it meets the WCAG "timing adjustable" need, and it lets the simulator check exact restore. Typing Drills hide words per minute in accuracy mode; quiz scores are never stored. **Alternative:** auto-start on open.

## 12. British spelling in bundled content (Sprint 07)

The packs mixed US and British spelling. **Decision:** user-visible content uses British spelling (colour, favourite), matching the docs. The content review also removed a now-contested place name, a disputed tree-network claim, stock riddles and an implied health claim ("back-friendly" became "Seated Easy Stretches"). **Alternative:** US spelling throughout.

## 13. How the app checks pack data (Sprint 07, ADR-016)

The app must re-check a pack's data against its engine schema before running it. Ajv (used by the CLI) compiles validators with `new Function`, which the app's CSP forbids. **Decision:** a small Rust validator (`data_schema.rs`) that supports a fixed set of schema keywords and refuses schemas using anything else; it uses the existing `regex` crate (already in the lockfile, now a direct dependency). Other choices made with it: a pack gets `activity.storage` only if both the pack and its engine declare it; v1 packs may contain only the manifest, the data file, a preview and `.md`/`.txt` notes; engines stay listed in the marketplace index but hidden in the app; no engine major is removed within v1. **Alternatives:** relax the CSP for Ajv (not recommended), or use a Rust JSON Schema crate (bigger dependency).

## 14. Security review follow-ups left for Sprint 08 (Sprint 07)

The review of the pack checker found no blocking issues. Medium and low items (regex differences, a work budget, exact `multipleOf`, a shared Ajv/Rust test corpus, cleaned terminal output) are fixed with tests in Sprint 07. Two items are deliberately left: (a) the app caches a pack's check result for the session, which is safe while packs are bundled; Sprint 08 must key the cache by installed commit SHA or content hash when marketplace installs land; (b) the shared plain-text pattern still allows the Unicode line and paragraph separators (U+2028/U+2029). Both validators agree, so nothing slips through one side only; banning them means changing every engine schema at once. **Question:** ban them in a later schema pass, or leave as is?

## 15. Third-party code activities stay blocked on every OS (Sprint 08, ADR-015)

Sprint 08 says third-party code is enabled only where the Sprint 03 sandbox proof passed. Windows passed 42/42 but with the open WebRTC gap (an activity can open a peer connection, a possible way to send data out), and macOS and Linux have not been run (M03-5). **Decision:** third-party `kind: app` activities can be browsed and reviewed but not installed on any OS in this release; one constant per OS controls it. Third-party content packs can be installed. **Question:** keep blocked until WebRTC is closed, or allow on Windows now?

## 16. How Lullshift reads marketplace repos (Sprint 08, ADR-015)

**Decisions:** Lullshift clones into a bare cache with no checkout and reads files from the pinned commit with `git ls-tree`/`cat-file`, then writes the files itself after checking them. So Git never writes repository files, `.gitattributes` filters and LFS never run, and symlinks are never created. Executable bits, symlinks and submodules in a pack are refused. Limits per pack: 512 files, 5 MiB per file, 20 MiB in all. Updates are only checked when you press Refresh. Removing a marketplace keeps the activities you installed from it (they are pinned copies) but stops their updates. Uninstalling keeps saved state unless you tick "also delete saved progress". URLs with a password or token in them are refused, and loopback/LAN hosts are refused. **Alternatives:** a regular checkout; auto-check for updates on launch (a network call the user did not start, so not chosen).

## 17. Marketplace URLs are strict (Sprint 08, ADR-015 implementation notes)

**Decision:** a marketplace URL must use a DNS host name with at least two labels (like `git.example.com`). IP addresses, `localhost`, single-word host names and suffixes such as `.local`, `.lan`, `.internal`, `.corp` and `.home` are refused, and so are `%`, `=` and non-ASCII characters. This closes off pointing Lullshift at services on your own machine or network, but it also blocks self-hosted Git servers reached by IP or by an internal-only name. Non-ASCII file names in packs are refused too. **Alternative:** allow private hosts after an extra "this is on your network" confirmation.

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

## 19. Companion providers and boundaries (Sprint 09, ADR-020)

**Decisions:** three provider kinds: Anthropic, OpenAI and "OpenAI-compatible" (any https base URL, or http for a local server on this computer, so local models work). You type the model name; suggestions are only hints. Keys live in the OS keychain only, are sent from the UI once and never shown again (not even the last four characters); if the keychain is unavailable, Companion refuses to store a key anywhere else. Replies are not streamed in v1, never auto-retried, and shown as plain text with no clickable links. Each message has its own context choice, defaulting to "No context", with a preview of the exact text before sending; the choice resets after each message, and what was sent is kept with the message. Budgets are a monthly token limit per provider, counted from the usage the provider reports. Requests carry no tools; a fixed one-line system prompt says it is a chat companion that cannot act. **Questions:** add streaming? allow keeping the last-used context scope?

## 20. Sprint 09 security review fixes: conservative choices taken (ADR-020)

Taken while the product owner was away; all fail closed. Please confirm or change each.

1. **Cancel keeps the thread busy until the HTTP worker ends** (up to the 60 s timeout). Sending, retrying and deleting that thread return "busy" meanwhile. A cancelled call is counted in usage: real numbers if a reply arrives, otherwise an estimate from the request size (so a cancelled call that never reached the provider is still counted; this errs towards the budget). The usage is queued by the worker and written to the store by the next Companion command that checks a budget or reads usage, so it is lost only if the app quits before then.
2. **Preview hash.** Send must carry the SHA-256 of the previewed context; any difference (or a hash given for "no context") refuses the send and shows a new preview.
3. **History never follows a message to another host.** Each stored message records the host it went to or came from. A send includes only earlier messages for the same host; the rest are left out, not refused, and the composer says how many earlier messages go along. Chosen over "refuse and ask for a new conversation" because it keeps changing a base URL usable while never replaying text to a host it was not sent to. Messages with no recorded host are never replayed. Cost: after changing providers the model may lack earlier context. The `host` column is added by migration 0009 (0008 is unchanged, because it already ran on a real database).
4. **Retry is refused if the profile's host changed** since the message was written ("Send the message again to review it").
5. **Reply cleaning uses `agent_core::text::is_invisible`**, which is wider than the list asked for (it also removes e.g. U+034F, U+2060-206F, Mongolian free variation selectors). `companion` now depends on `agent-core`.
6. **Keychain at launch.** The keyring API in use has no metadata-only check, so `hasKey` still reads the secret. Instead the Companion view is no longer mounted at app launch; it mounts on first visit, so the keychain is not read until then.

## 21. Sprint 10 lineup: eleven new games and 21 packs

**Decision:** to reach the 20-game target, eleven original games on the kits: Tea Sort (pour colour layers until each cup is one colour), Pebble Merge (slide and merge pebbles on a 4x4 tray), Sea Glass (swap to make lines of three, 25 swaps a round), Thread Picture (picture-by-numbers), Brook Links (link matching stones with streams that don't cross), Island Bridges (connect numbered islands), Harvest Pairs (pairs that add to 13 from a triangle of cards), Clock Patience (a calm game of chance), Frost Steps (slide on ice to the lantern), Breeze Bounce (keep a seed aloft, no lives) and Glow Trail (a trail that shortens instead of ending the game). All grid puzzles have a daily puzzle (S10-T03). Plus 21 content packs (two per engine, three for Calm Cards) to bring activities to about 100. Mechanics are generic; names, art and wording are original, and user-facing text avoids product names of well-known games. **Question:** any of these you'd rather drop or replace?

## 22. OpenCode still not added (Sprint 10)

Decision 1 is still open, so Sprint 10 researches and builds only Codex and Cursor adapters, as the sprint says.

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

## 24. Sprint 10 content review: choices taken

Taken while the product owner was away. Please confirm or change each.

1. **Tea Sort's sixth tea is now "Sencha"** (was "Matcha"; its pattern also rendered as a solid fill and is now a visible grid), so every tea has its own first letter, as the help promises.
2. **Island Bridges marks finished islands with a double ring** as well as green, so colour is not the only cue.
3. **Fog Garden got a daily field** ("Today's field"): today's seed with the centre tile opened for you, so everyone gets the same field. This adds a fourth button to its controls row.
4. **Size stays a separate `<select>` row** in Thread Picture, Brook Links and Island Bridges. Petal Grid and Stone Slide put a "Size" button in the main controls row instead. This is left inconsistent for now; unifying it is a small follow-up if you want one pattern.
5. **Name clash not checked.** "Tea Sort" describes a common sort-by-pouring mechanic and may be close to existing app-store titles. No web or store search was done; please check before release, or pick another name (for example "Teacup Tidy").
6. **Listening prompts** (Pause: Sounds around you and similar) assume hearing; each pack says any step can be skipped. Left as is.
