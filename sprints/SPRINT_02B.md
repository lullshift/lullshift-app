# Sprint 02b - Design system and UX pass

**Goal:** Lullshift looks calm, soft and polished, and every screen has one obvious next step, before Sprint 03 adds more UI.

**Requested by:** product owner, 2026-11-05 ("very beautiful in terms of styles ... a nice and perfect UX"; direction: calm and soft; timing: now, before Sprint 03).

**Depends on:** Sprint 02.

## Work items

- [x] S02B-T01: Record the design decision (ADR-011) and the design system with UX principles, voice and review checklist (`docs/DESIGN_SYSTEM.md`); add PRD principle 9
- [x] S02B-T02: Tokens for light and dark themes, bundled Figtree font, base styles; contrast test for text tokens
- [x] S02B-T03: Components: icons, state pill, switch, dialog, banner, buttons
- [x] S02B-T04: App shell with sidebar navigation and counts; narrow-window layout
- [x] S02B-T05: Sessions view: summary card, grouped session cards, agent names and short IDs, relative times, designed empty, loading and error states
- [x] S02B-T08 (product owner, from mockups): multi-tool Sessions layout: Live and History tabs, search and tool filter chips, Live columns (Needs you, Working) that scroll on their own, sortable History table
- [x] S02B-T06: Integrations view with consent and disconnect in modal dialogs; Settings view with switches
- [x] S02B-T07: Check light and dark in the running app; keyboard and screen-reader pass (screen-reader pass with a real reader is owed: `docs/MANUAL_TESTS.md` M02B-1)

## Acceptance criteria

- [x] S02B-A01: All three views follow `docs/DESIGN_SYSTEM.md` in light and dark mode in the running app
- [x] S02B-A02: Text colour tokens meet WCAG AA contrast (automated test)
- [x] S02B-A03: Every action works by keyboard only; dialogs trap focus, close with Esc and return focus
- [x] S02B-A04: Nothing animates with reduced motion; no network requests for fonts or assets

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- No weakening of the CSP; no remote fonts, images or scripts.
- Represent unknown agent status honestly; never lose local user data.

## Closeout

```text
Sprint: 02b
Completed tasks (commits): "Add calm design system and multi-tool Sessions layout (Sprint 02b)"
  ADR-011 and docs/DESIGN_SYSTEM.md (principles, voice, tokens, components, review checklist);
  PRD principle 9. Tokens for light ("linen dawn") and dark ("night lull") themes, with the
  colour roles the product owner set with the branding session: lavender for interaction,
  orange only for "needs you", teal only for the mascot and icons (ADR-011).
  Bundled Figtree Variable font. Components: Icon, Mascot, StatePill, Switch, Dialog, Banner,
  buttons. Sidebar shell with counts and a narrow-window top bar. Sessions view (layout picked
  from three mockups, then refined): Live and History tabs, search and tool chips, one-line
  summary, Live columns Needs you and Working that fill the window and scroll on their own,
  History table sortable by tool, session, state and last seen; agent names, short IDs,
  relative times, skeleton loading, designed empty state. Integrations: consent and
  disconnect in modal dialogs, hook events in plain words. Settings: switches.
Commands run and results:
  pnpm check: all 11 steps pass. Desktop tests: 55 pass (18 contrast, 21 format, 16 earlier).
  Debug app driven over the WebView debug port (scratch script, not committed):
  screenshots of all three views and both dialogs in light and dark at 900 px, and at 600 px;
  7-9 requests per load, none leaving the app; Figtree loads from the bundle; Enter opens the
  consent dialog with focus on Cancel, Tab stays inside, Esc closes and returns focus to the
  Connect button; with reduced motion the pill animation is "none".
  Sessions layout, same method, at 1280, 900 and 600 px in light and dark: two columns at the
  default 900 px; the page doesn't scroll at 900 and 1280 (History scrolls inside: 373 of
  638 px visible); ArrowRight and Home move between tabs; sorting by Session sets
  aria-sort; typing "smoke" filters History to 9 rows; the Cursor chip shows "No past
  sessions match." and its Clear button resets search and chip. Fixed on the way: the
  narrow top bar stretched to ~260 px when the content was short.
Tests not run and why:
  Real screen readers (Narrator, VoiceOver, Orca): need a person (M02B-1). macOS and Linux
  rendering: no machines (M02B-2). Empty state and error banners were not seen in the running
  app because the local database already had sessions (M02B-3). No live (Needs you or
  Working) session existed, so cards in the Live columns, their own scrolling with many
  sessions, and Cursor sessions weren't seen (M02B-4; no Cursor adapter yet).
Security/privacy review: no new permissions or commands; CSP unchanged; font and mascot are
  bundled; no network requests (checked).
Remaining defects and risks: the screenshot/keyboard script is a scratch tool, not an automated
  test in the repo (end-to-end UI automation is still open from Sprint 00).
Acceptance checks passed/failed: A01-A04 passed as described above.
Manual checks owed: docs/MANUAL_TESTS.md M02B-1 to M02B-4.
Recommended next action: commit, then start Sprint 03.
```
