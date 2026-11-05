# ADR-011: UI design system

- **Status:** Accepted (design pass before Sprint 03)
- **Date:** 2026-11-05
- **Owner:** Product owner (direction), implementation lead
- **Related:** `docs/DESIGN_SYSTEM.md`, PRD principles 8 and 9

## Context

The product owner asked for a very beautiful app with a polished UX, in a **calm and soft** visual direction, before more screens are added in Sprint 03 onward. The app must stay offline (no web fonts or CDNs), pass a strict CSP (`style-src 'self'`), and stay keyboard- and screen-reader-accessible with reduced motion.

## Decision

- **Own design tokens in plain CSS:** colours, type, spacing, radii, shadows and motion are CSS custom properties in `apps/desktop/src/styles/tokens.css`, with light and dark themes following the OS setting. Components use tokens only, never raw colours.
- **No UI framework or CSS-in-JS:** a small set of in-house React components (`src/ui/`) plus plain CSS. This keeps the bundle small, works under `style-src 'self'` and avoids a dependency for a few screens.
- **Bundled font:** Figtree Variable (`@fontsource-variable/figtree`, OFL-1.1), served from the app bundle. No font is fetched from the network. Falls back to the system UI font.
- **Icons and illustrations:** hand-drawn inline SVG in the repo. No icon library, and no third-party brand logos (agents are shown by name and a generic glyph). The Lullshift mascot and brand colours come from `docs/brand/`; the app bundles those SVGs.
- **Colour roles (product owner, 2026-11-05):** lavender is interaction only (`--accent`: primary buttons, links, focus, selected tab, switches, Working). Orange is attention only (`--attention`: Needs you). Teal is identity only: app icon, tray and the mascot illustration, never buttons or controls. Teal and lavender are never shown side by side as equal controls. Details in `docs/DESIGN_SYSTEM.md`.
- **Navigation:** a sidebar with three views (Sessions, Integrations, Settings) held in local state. React Router isn't added until a view needs deep links.
- **Rules every screen follows** are in `docs/DESIGN_SYSTEM.md`: tone of voice, state colours with text labels (never colour alone), WCAG AA contrast for text, motion that stops under reduced motion, and honest unknown or offline states.
- Activities (Sprint 03+) run in their own origin and can't use these styles directly. The SDK may later publish the same tokens for activities that want to match.

## Consequences

- One font package is added (about 50 KB of WOFF2). Its licence ships with the package.
- New UI must use the tokens and components; reviews check this.
- Later visual changes are mostly token edits.
