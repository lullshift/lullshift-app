# ADR-026: Home, Play, Rest, Continue and the tray window

- **Status:** Accepted (owner decision, 2026-11-07: Direction A "Doors", with B's "fits your wait" suggestion on Home and C's tray window now)
- **Date:** 2026-11-07
- **Related:** ADR-011 (calm design system; layout section amended here), ADR-003 (activity model), ADR-016/017 (activity host and help), ADR-020 (Companion), ADR-008 (session states)

## Context

The owner reviewed the running app and found it mechanical: a control panel of lists and settings rather than a calm companion for the wait. Concretely: games and breaks were one long grid with no search, help said "How to play" on breathing exercises, there was no place for things you had started, the theme could not be chosen, and the first screen was a session board. The PRD's experience model (section 3) already says the app has two moods, **Disconnect** (play, rest) and **Stay in flow** (Next Move, Companion). Three mockups were shown; the owner chose A with two borrowings.

## Decision

### 1. Navigation

Sidebar, top to bottom:

- **Home** (the start page)
- **Sessions** (count)
- group label **While you wait**: **Play** (count), **Rest** (count), **Continue** (count of saved items; hidden count when zero), **Companion**
- **Settings** at the bottom. **Integrations** becomes a section inside Settings (the first-launch onboarding is unchanged). Every link that opened Integrations opens Settings at that section.

The mascot, the "Local" note and the nav badge rules (orange only for "needs you", ADR-011) are unchanged. The app opens on Home; a click on a "needs you" notification still opens Sessions with that session.

### 2. Home

One question: *what do I do while my agents work?* Top to bottom:

1. **Status, in words, with the mascot.** If anything needs you, that comes first as an attention card naming the tool, with **Go to session** as its one button (UX principle 2). Otherwise one heading ("Your agents are working", "All quiet", "No agents yet") and one line; never a claim that a task is done.
2. **Agent strip:** up to 4 live sessions as compact chips (state pill, tool, project folder, time); "All sessions" link when there are more. Unknown and stale states say so.
3. **A suggestion that fits the wait** (below).
4. **Three doors:** Play, Rest, Think ahead (opens Companion; Next Move stays in the session panel).
5. **Pick up where you left off:** up to 4 saved activities, newest first, with **Resume**. Hidden when there are none.

### 3. "Fits your wait" suggestion

- **Estimate.** Lullshift computes, locally and on demand, the median length of finished *working* stretches (working until the next needs-you, idle or ended) per tool over the last 14 days, from the session events it already stores. It needs at least 5 stretches; otherwise there is no estimate. Nothing new is recorded, nothing leaves the computer, nothing is shown per day or as a score (PRD principle 7). It is an estimate about agents, never about the person.
- **Wording.** With an estimate: "Claude Code usually takes about 6 min". Without: "For a short break". Never "will finish at", never a countdown.
- **Choice (deterministic, local).** If something is saved, the first suggestion is to resume the newest save. Otherwise: estimate under 4 minutes, or no agent working: a Rest item (breathing, pause, movement); 4 minutes or more: a Play item. Only enabled, installed activities. **Shuffle** moves to the next candidate in a stable order seeded by the date, so it does not jump around on every render.
- No agent working and nothing saved: the suggestion is still offered, as "If you'd like a break".

### 4. Play, Rest and Continue

- **Play** lists activities whose `category` is `arcade`, `interactive` or `productivity`. **Rest** lists `break`. The split uses the manifest's `category` only.
- Each page: a search box (name, description, tags; case-insensitive, local) and filter chips from a **fixed, host-owned map** of known tags (Play: Puzzles, Calm, Quizzes, Typing, Toys, Cards; Rest: Breathing, Movement, Step away, Reflection, Pause, Eyes). Third-party tags are untrusted strings: they are searchable but never become chips or labels. A **Continue** row at the top shows that page's saved items.
- **Continue** page: every activity with a saved state, newest first, with **Resume** and **Start over**, plus how long ago it was saved. No play counts, totals or streaks are shown anywhere.
- Help: the link reads **How to play** for Play items and **How it works** for Rest items. The dialog title follows.
- **Manage** and **Marketplaces** stay reachable from both pages (a quiet "Manage activities" link), not as tabs above the library.

### 5. Theme

Settings > Appearance gets **Theme: System, Light, Dark** (default System), saved with the interface preferences and applied via `data-theme` on the root. Running activities receive the effective theme at start, as today.

### 6. Tray window

- A second webview window, label `tray`, about 380 x 560, no decorations, always on top while shown, skipped from the taskbar. A **left click** on the tray icon toggles it next to the icon; it hides when it loses focus or on Esc. The tray menu (right click) is unchanged, and keeps "Open Lullshift".
- Content: the mascot for the current state, one status line, the needs-you card first when something needs you, the "fits your wait" suggestion with **Play** (or **Resume**) and **Shuffle**, up to three quick chips, and **Open Lullshift**. Footer tabs **Now**, **Library**, **Talk**: Library and Talk open the main window at Play and Companion.
- **Activities never run in the tray window.** Play/Resume shows the main window and starts the activity there, through the normal host and sandbox.
- **Least privilege.** The tray window has its own capability file granting only: reading the dashboard, the activity list, saved-state presence, the suggestion data, and one new command that shows the main window at a view (and optionally starts an activity by id, validated against installed, enabled activities). It gets no companion, marketplace, hook, settings or storage commands. Its CSP is the same as the main window's.
- A setting under Settings > Appearance, **Tray click opens: Quick window / Main window** (default Quick window), for people who prefer the old behaviour.

## Consequences

- The app's first screen is about the person's wait, not a board; Sessions keeps its full layout (ADR-011, decision S04-T07) for detail.
- One more window and capability to review; the guard test that every registered command is granted (`acl_check.rs`) is extended to the tray capability.
- `docs/DESIGN_SYSTEM.md` layout section and the PRD screenshots change; manual tests are added for Home, the library pages, the theme setting and the tray window.

## Alternatives considered

- **B, Two moods** as the whole layout: closest to the PRD wording, but merging Next Move into a page and a sidebar rail cost more and changed less of the feel. Its suggestion idea is kept.
- **C, Quiet companion** as the main surface: the most "lull", but it still needs A's pages for browsing; kept as the tray window.
