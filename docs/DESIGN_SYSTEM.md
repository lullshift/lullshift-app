# Lullshift design system

Direction chosen by the product owner (2026-11-05): **calm and soft**. Lullshift is a quiet companion that sits beside your agents. It should feel like a deep breath, not a control room. Decision record: ADR-011. Brand colours and the mascot come from `docs/brand/`.

## Colour roles

Each colour family has exactly one job:

- **Lavender is interaction.** `--accent` is for primary buttons, links, focus rings, the selected nav item, switches and the Working state. It's dusky, so it never competes with the orange alert.
- **Orange is attention, and only attention.** `--attention` means "needs you": the state pill, the nav badge and the summary card. Nothing else is orange.
- **Teal is identity, never interaction.** The mascot, app icon and tray are teal. Inside the app, teal appears only as the mascot illustration (sidebar mark, empty states, summary face, and later the about screen and return card). Buttons and controls are never teal.
- Teal and lavender are never placed side by side as equal controls. The teal mascot inside a lavender UI is fine, because it's an illustration.
- Neutrals (warm linen, plum ink) carry everything else. Sage green is for success and rose for errors.

## UX principles

1. **Calm by default.** Nothing moves, glows or turns warm unless it means something. "All quiet" is a good state and looks like one.
2. **What needs me comes first.** The most urgent thing is at the top of every view, in plain words ("Claude Code needs you"), never only as a colour.
3. **Honest states.** Unknown, stale, offline and failed are shown as such, with a short reason. Never imply a task is done because a turn ended.
4. **One clear next step.** Each empty state, error and card offers the one action that helps, as a button.
5. **Ask before changing anything outside the app,** in a focused dialog that names exactly what changes, and confirm afterwards what happened.
6. **Everything by keyboard.** Visible focus, logical order, dialogs that trap focus and close with Esc.
7. **Respect attention.** No badges for things that don't need the user, no nagging, no streaks or scores.

## Voice

- Short, warm, plain: "No agent sessions yet", not "No data available".
- Second person, present tense. Name the agent ("Claude Code"), not internal IDs.
- Errors say what happened, what didn't happen, and what to do: "The settings file changed while Lullshift was updating it. Nothing was written; try again."
- No exclamation marks, no blame, no jargon (no "IPC", "hook payload", "DACL" in the UI).

## Tokens (`apps/desktop/src/styles/tokens.css`)

| Group | Light ("linen dawn") | Dark ("night lull") | Use |
|---|---|---|---|
| `--bg` | warm linen | deep plum night | Window background, with two soft colour glows |
| `--surface`, `--surface-2` | warm white, sand | raised plum, plum | Cards, wells and inputs |
| `--text`, `--text-muted`, `--text-subtle` | plum ink in 3 strengths | moonlight in 3 strengths | Body, secondary and tertiary text |
| `--accent` | dusky lavender | soft lavender | Interaction only: primary actions, focus, Working |
| `--attention` | brand orange, darkened for text | brand orange | Needs you, and nothing else |
| `--success` | sage | sage | Completed, Connected |
| `--danger` | rose clay | rose | Failed, errors |
| `--*-soft` | tinted backgrounds for each of the above | | Pills and banners |

- **Contrast:** text tokens meet WCAG AA (4.5:1) on `--bg`, `--surface` and their own `--*-soft` backgrounds in both themes. They're checked by `apps/desktop/src/contrast.test.ts`.
- **Type:** Figtree Variable, falling back to the system UI font. The scale is 12 / 13 / 14 / 16 / 20 / 28 px, with weights 400, 500, 600 and 700. Times use tabular figures.
- **Spacing:** a 4 px base (`--space-1` … `--space-8`).
- **Radii:** 8, 12 and 18 px, plus fully round for pills and switches.
- **Shadows:** two soft, warm, low-opacity layers. Dark mode uses a hairline border instead of heavy shadow.
- **Motion:** 160 ms for small changes and 240 ms for panels and dialogs, with a gentle ease-out.
  - Working: a slow 2.4 s "breathing" dot.
  - Needs you: a soft glow, no flashing.
  - With `prefers-reduced-motion`, all animation stops and states stay readable.

## Components (`apps/desktop/src/ui/`)

| Component | Notes |
|---|---|
| `Icon` | Inline SVG set, 20 px, `currentColor`, `aria-hidden`. |
| `Mascot` | The brand mascot from `docs/brand/`: the sidebar mark and the empty state. Decorative; never the only signal. |
| `StatePill` | A dot plus a text label for each session state. The tone comes from `stateView`. |
| `Switch` | `button role="switch"` with `aria-checked`. Used for on/off settings instead of checkboxes. |
| `Dialog` | Native `<dialog>` opened modally: focus trap, Esc to cancel, focus returns to the opener. |
| `Banner` | `role="alert"` for problems and `role="status"` for confirmations, with an icon and soft background. |
| Buttons | `.btn` with `.btn-primary`, `.btn-secondary`, `.btn-quiet` and `.btn-danger`. At least a 36 px target. |

## Layout

- **Shell** (ADR-026): a sidebar holds the mark, the navigation and a "Local only" note. Top to bottom: **Home** (the start page), **Sessions**, **Notes** (a grey count of open checklist items, no orange badge; ADR-026 section 7), the group label **While you wait** with **Play**, **Rest**, **Continue** and **Companion**, then **Settings** at the bottom. Play, Rest and Continue show a quiet grey count (Continue only when something is saved); the orange badge stays reserved for "needs you" (Sessions) and Companion replies. Integrations is a section of Settings, not a nav item. Content uses the full window width by default. Settings → Appearance offers "Comfortable", which keeps text views to 760 px and wide views (Sessions) to 1,120 px. The choice is saved with the other interface preferences.
- **Narrow windows:** below 720 px the sidebar becomes a top bar of icons (the group label and counts are dropped; every button keeps its name for screen readers).
- **Home** (ADR-026 section 2, chosen by the product owner from mockups, 2026-11-07). One question: what do I do while my agents work? Top to bottom:
  - **Status with the mascot.** One heading and one line ("Your agents are working", "All quiet", "No agents yet", "Agent status is unknown"); never a claim that a task is done. When something needs you, the status becomes an orange card naming the tool ("Claude Code needs you") with one **Go to session** button.
  - **Agent strip:** up to four live sessions as small chips (state pill, tool, session name or short ID, time), with an **All sessions** link when there are more. A session that is stale or not confirmed says "not confirmed".
  - **Fits your wait:** one suggestion card. Saved activity first (**Resume**); otherwise a rest for short or unknown waits and a game for waits of about 4 minutes or more. The line above reads "Claude Code usually takes about 6 min" (at least 5 finished stretches in 14 days, from stored events), "For a short break", or "If you'd like a break". **Shuffle** walks a date-seeded order. Never a countdown or a finish time.
  - **Three doors:** Play, Rest, Think ahead (opens Companion).
  - **Pick up where you left off:** up to four saved activities with **Resume**; hidden when none.
  - Loading, error, empty and unknown states are designed: skeletons, a retry link, "No activities are turned on" with **Manage activities**.
- **Play and Rest** (ADR-026 section 4): the library split by manifest category (arcade, interactive, productivity are Play; break is Rest). Each page has a header with a quiet **Manage activities** link, a search box (name, description, tags), filter chips from a fixed host-owned tag map (third-party tags are searchable but never shown), a **Continue** row of that page's saved items, and the card grid. The help link reads **How to play** (Play) or **How it works** (Rest) and the dialog title follows. **Manage** and **Marketplaces** open from the quiet link, with **Back to Play** or **Back to Rest**.
- **Continue:** every saved activity, newest first, as rows with the kind, "saved N min ago", **Resume**, **Start over** and the help link. No counts, totals or streaks anywhere.
- **Sessions view** (layout chosen by the product owner from mockups, 2026-11-05). It is built for several agent tools at once (Claude Code now, Cursor in v1):
  - A bar with **Live** and **History** tabs (with counts), a search box (session ID or tool name) and tool filter chips (All tools, then each tool). The search and chips apply to both tabs.
  - A one-line summary under the bar ("Claude Code needs you · 3 more are still working."), with a dot in the state's colour. It always counts every session, not just the filtered ones.
  - Problems next.
  - **Live:** two columns, **Needs you** and **Working**. They fill the window height and each scrolls on its own, so a long list never pushes the other column out of view. An empty column says so ("Nothing needs you.").
  - **History:** finished, idle and unknown sessions in a full-width table (Tool, Session, State, Last seen) that scrolls inside itself. Click a column header to sort; newest first by default.
  - Below 720 px wide or 520 px tall, the fixed height is dropped and the page scrolls as one; below about 480 px of content width, the two columns stack.
- **Session card:** tool name and state pill on the first line; short session ID and relative time ("2 min ago", exact time on hover) on the second; a reason line when the state isn't directly observed.
- **Activity cards** (Play, Rest):
  - A grid of cards. Each shows a kind label (Rest, Game, Focus, Play), the name, a description and "Saved 2 min ago" when there is progress (no played-time totals). The buttons are **Start**, or **Resume** plus **Start over**.
  - A running activity fills the content column. Its title bar has the name, **Pause** and **Close**. The activity's own page sits in a rounded frame.
  - The paused card shows the sleeping mascot, "<Name> is paused" and "Your progress is saved". When an agent caused the pause, the heading says which tool needs you ("Claude Code needs you"), with **Go to Sessions** and **Keep playing**. An activity that stopped answering says so and keeps its last save.
  - Activities follow the app theme and reduced-motion setting, which the host passes to them at start.
- **Session detail panel** (S04-T07, chosen by the product owner from mockups, 2026-11-05):
  - Cards and History rows are buttons. A click, Enter or Space opens the panel; Esc closes it and returns focus to the card. The open card has a lavender outline.
  - **Pinned** (the default) docks the panel on the right. In a window of 1,150 px or more, both Live columns stay. In a narrower one, Live becomes one grouped list: Needs you fixed at the top, Working scrolling below. Each row is two lines: tool and time, then the short ID. History becomes the same compact rows.
  - **Floating** slides the panel over the board, with a soft shadow. Esc, a click outside or the close button closes it.
  - Under 720 px the panel always floats. The pin choice is a saved preference that this doesn't change.
  - A new "needs you" never takes over an open panel.
  - **Content by state:**
    - Needs you: what it's asking, the line "Answer in your Claude Code window", the intent, a "Before you answer" checklist and notes.
    - Working: the intent, notes and Next Move.
    - Ended: the timeline, export, delete and the retention rule.
  - **Return card.** An ended session the user hasn't reviewed shows a lavender **Review** pill and a "Welcome back" card. The card never claims the task is done.
  - **Captured text** carries a small source label ("captured with your OK") and a **Clear captured text** action.

## Tray window (ADR-026 section 6)

A second, small window next to the tray icon (`apps/desktop/src/tray/`, entry `tray.html`): 380 x 560, no frame (a 1px `--border-strong` stands in for it), always on top while shown, hidden on blur and Esc. It uses the same tokens, `ui/` components, `styles.css` classes (`card`, `btn`, `chip`, `banner`, `skeleton`) and theme as the main window; `tray.css` adds only its layout.

- **Order, top to bottom:** mascot for the state and one status line (`home/status.ts`); the **Needs you now** card first when something needs you (attention colours, one button **Go to session**); the "fits your wait" card (`suggest.ts`) with **Play** or **Resume** and **Shuffle**; up to three chips (a rest and a game before more of one kind); **Open Lullshift** (always visible, outside the scrolling area); the footer **Now / Library / Talk**.
- **No lists, no counts, no statistics.** One suggestion, one button. Wording never claims a task is done.
- **It asks, it never runs.** Every action calls `open_main` and the main window does the work. No activity, iframe or bridge exists in this window.
- **Keyboard.** When shown, focus lands on the first useful control (`data-first`: the needs button, else the suggestion's button, else Open Lullshift). Tab order follows the visual order; Esc hides.
- **Honest states.** Loading skeletons, a danger banner with **Try again** when the dashboard or the activity list can't be read, "may be out of date" when only the last good data is left, and "unknown" status copy from `homeStatus`.
- **Reduced motion:** no shimmer or transitions. Light and dark follow the Theme setting (re-read each time the window is shown).

## Review checklist for new UI

- [ ] Uses tokens and `ui/` components; no raw colours.
- [ ] Works in light and dark, at 720 px and at 900 px wide.
- [ ] Keyboard only: every action reachable, focus visible, dialogs trap and return focus.
- [ ] Screen reader: headings in order, live regions for status changes, labels on icon-only controls.
- [ ] Reduced motion: nothing animates.
- [ ] Loading, empty, error, unknown and offline states are designed, not left blank.
- [ ] Copy follows the voice rules above.
