# Creator guide

Build games and activities for Lullshift and share them from a Git repository. Everything here runs locally. There is no account, upload or central review.

| Page | What it covers |
|---|---|
| [Tutorial: your first content pack](tutorial-content-pack.md) | Create, validate, simulate and pack a quiz pack, start to finish |
| [SDK and bridge contract](sdk-and-bridge.md) | What every code activity must do: start, save, restore, pause, summary, keyboard, reduced motion |
| [Kits](kits.md) | Grid, card and arcade kits for games |
| [Engines and content packs](engines-and-content-packs.md) | The bundled engines and their data schemas |
| [CLI reference](cli.md) | `create`, `validate`, `simulate`, `dev`, `pack`, `marketplace init` |
| [Sandbox rules](sandbox.md) | What an activity can't do, and why |
| [Publishing a marketplace](publishing-a-marketplace.md) | `.lullshift/marketplace.json`, pinning and review |
| [Using an AI tool](ai-tools.md) | Creator skills for Claude Code, Codex, Cursor and OpenCode |

## Three kinds of activity

Choose the lightest kind that fits (ADR-003). Kits and engines are optional conveniences.

| Kind | What you ship | Runs code? | Manifest |
|---|---|---|---|
| **Content pack** | JSON data, a preview image and optional notes, for one of the bundled engines | No | `"kind": "content"`, with `engine` and `data` |
| **Code activity on a kit** | A static web app whose game rules use the grid, card or arcade kit | Yes, in the sandbox | `"kind": "app"` |
| **Custom code activity** | Any static web app (HTML, CSS, classic-script JavaScript) that implements the bridge | Yes, in the sandbox | `"kind": "app"` |

**Who can install what today.** Content packs and code activities from any marketplace can be installed in the app. Code from a third-party marketplace is shown as a "Third-party program" and is installed only after the user reads a risk warning and ticks "I understand and want to install this program" (owner decision 27.5, ADR-015). The warning says it runs in Lullshift's sandbox with no file, network, clipboard or app access, lists the permissions your manifest declares, and says honestly what is verified on the user's platform. Users consent again for each update that changes your code. It then runs exactly like a bundled activity, in the same sandbox. Bundled activities, contributed by pull request (see [CONTRIBUTING.md](../../CONTRIBUTING.md)), are reviewed by the project and need no warning.

Context Keeper, Next Move and Companion are app features, not activities. Activities never see agent sessions, notes or keys.

## The creator journey

Describe, generate, preview (`dev`), simulate, validate, pack, then publish to a Git repository that users add.

## Rules for every activity

- Works offline and declares `"offline": true`.
- Can be paused at any moment and loses nothing: the app, not the activity, controls pausing.
- Fully playable with the keyboard, with visible focus and text labels.
- Honours reduced motion and the light or dark theme.
- Has `help` and a `preview` image in its manifest.
- Declares only the permissions it uses. Today the only permission is `activity.storage`.
- Uses original names and art. Generic mechanics are fine; copies of branded games (names, characters, art, level designs, look) are not.
- You must hold the rights to what you publish and declare the licence of your activity and any third-party assets. Bundled activities are Apache-2.0.

## Using an AI tool

Generated code is untrusted, whichever tool made it. The sandbox and the CLI checks decide, not the generator.

- **Claude Code, Codex, Cursor and OpenCode:** use the `activity-creator` skill, and `marketplace-publishing` for marketplace repositories. Both live in `.agents/skills/`. See [Using an AI tool](ai-tools.md) for how each tool finds and runs them.
- **Other tools:** give them `creator/ACTIVITY_CREATOR_PROMPT.md`, `packages/activity-sdk/README.md` and `reference/ACTIVITY_MANIFEST_V1.schema.json`.

The tool, or you, must run `validate` and `simulate` and read the real output. Don't treat an activity as done until both pass.

## Reference material

- SDK guide: [packages/activity-sdk/README.md](../../packages/activity-sdk/README.md)
- Kits guide: [packages/activity-kits/README.md](../../packages/activity-kits/README.md)
- Manifest schema: [reference/ACTIVITY_MANIFEST_V1.schema.json](../../reference/ACTIVITY_MANIFEST_V1.schema.json)
- Marketplace schema: [reference/MARKETPLACE_V1.schema.json](../../reference/MARKETPLACE_V1.schema.json)
- Writing style for content: [docs/CONTENT_STYLE_GUIDE.md](../CONTENT_STYLE_GUIDE.md)
- Decisions: ADR-003 (activity model), ADR-012 (sandbox and bridge), ADR-015 (marketplace install and pinning), ADR-016 (engines and packs), ADR-017 (CLI and simulator), ADR-019 (kits) in [docs/adr/](../adr/)
