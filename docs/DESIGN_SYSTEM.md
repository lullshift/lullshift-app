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

- **Shell:** a sidebar holds the mark, the navigation with counts, and a "Local · Offline" note; the content column is up to 760 px wide.
- **Narrow windows:** below 720 px the sidebar becomes a top bar.
- **Sessions view** (layout chosen by the product owner from mockups, 2026-11-05). It is built for several agent tools at once (Claude Code now, Cursor in v1):
  - A bar with **Live** and **History** tabs (with counts), a search box (session ID or tool name) and tool filter chips (All tools, then each tool). The search and chips apply to both tabs.
  - A one-line summary under the bar ("Claude Code needs you · 3 more are still working."), with a dot in the state's colour. It always counts every session, not just the filtered ones.
  - Problems next.
  - **Live:** two columns, **Needs you** and **Working**. They fill the window height and each scrolls on its own, so a long list never pushes the other column out of view. An empty column says so ("Nothing needs you.").
  - **History:** finished, idle and unknown sessions in a full-width table (Tool, Session, State, Last seen) that scrolls inside itself. Click a column header to sort; newest first by default.
  - Below 720 px wide or 520 px tall, the fixed height is dropped and the page scrolls as one; below about 480 px of content width, the two columns stack.
- **Session card:** tool name and state pill on the first line; short session ID and relative time ("2 min ago", exact time on hover) on the second; a reason line when the state isn't directly observed.
- **Cards aren't clickable yet.** They don't look like buttons, so they don't promise an action that doesn't exist. A session detail panel opened from a card is planned for Sprint 04 (S04-T07).

## Review checklist for new UI

- [ ] Uses tokens and `ui/` components; no raw colours.
- [ ] Works in light and dark, at 720 px and at 900 px wide.
- [ ] Keyboard only: every action reachable, focus visible, dialogs trap and return focus.
- [ ] Screen reader: headings in order, live regions for status changes, labels on icon-only controls.
- [ ] Reduced motion: nothing animates.
- [ ] Loading, empty, error, unknown and offline states are designed, not left blank.
- [ ] Copy follows the voice rules above.
