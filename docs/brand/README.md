# Lullshift brand assets

The Lullshift mascot is a small folded-paper card that stands like a tent and keeps one eye open. The teal front is the agent at work; the orange triangle is the inside of the fold, the moment your attention turns. It rests while the agent works and wakes up when it needs you.

All artwork here is original and Apache-2.0 like the rest of the repository.

## Files

| File | Use |
| --- | --- |
| `mascot-watching.svg` | Master app icon: one eye open. Source for every file in `apps/desktop/src-tauri/icons/`. |
| `mascot-asleep.svg` | Agent working, nothing needs you. |
| `mascot-awake.svg` | An agent needs you. |
| `mascot-happy.svg` | Task finished. |
| `tray-asleep.svg`, `tray-awake.svg` | Teal tray icons for Windows and Linux. |
| `tray-asleep-template.svg`, `tray-awake-template.svg` | Black template tray icons for the macOS menu bar. |

The tray versions keep only two faces with oversized eyes, cropped tight, so asleep and awake stay distinct at 16 px. The full-color face is not legible below 24 px.

## Color roles

Each color has one job, so the attention signal never gets lost:

- **Teal is identity.** App icon, tray, the mascot wherever it appears (including inside the app), README and website art. Never used for buttons or other controls.
- **Lavender is interaction.** In-app accent for primary buttons, links, focus rings, selected tabs and the Working state. It is never applied to the mascot. The tokens live in the design system (`docs/DESIGN_SYSTEM.md`).
- **Orange is attention.** "Needs you" in the tray, the window and notifications. Nothing else is orange.

Teal and lavender should not appear side by side as equal controls. The teal mascot inside a lavender interface is fine, because it is an illustration, not a control.

## Colors

| Name | Value | Use |
| --- | --- | --- |
| Teal light | `#52E4CC` | Top of the front gradient |
| Teal deep | `#0F7F93` | Bottom of the front gradient |
| Tray teal | `#1497A5` | One-color tray icon on Windows and Linux |
| Orange light | `#FFB547` | Top of the fold |
| Orange deep | `#C9480F` | Bottom of the fold |
| Attention orange | `#FF9A2E` | Sleep and wake marks |
| Ink | `#0B2E36` | Eyes and mouth |

## Regenerating the app icons

From `apps/desktop`:

```bash
pnpm tauri icon ../../docs/brand/mascot-watching.svg -o <temp dir>
```

Copy only the files that already exist in `src-tauri/icons/`; the generator also writes Android and iOS folders, which v1 does not ship.

For each tray SVG, render a 32 px PNG into `src-tauri/icons/tray/` with the same base name:

```bash
pnpm tauri icon ../../docs/brand/tray-asleep.svg -o <temp dir> -p 32
```

`lib.rs` embeds the tray PNGs at build time; a unit test checks that they decode.
