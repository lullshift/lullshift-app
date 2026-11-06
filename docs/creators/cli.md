# CLI reference

`lullshift` is the creator CLI (ADR-017), version 0.2.0. It runs locally with no network and no account. It lives in `tools/lullshift-cli`.

## Getting it

**From npm, after the first release.** The owner has approved publishing the CLI as `@lullshift/cli` (decision 29), but it isn't on npm yet. Once it is, you need only Node.js 22.12 or newer:

```bash
npx @lullshift/cli create my-activity --name "My Activity" --id dev.you.my-activity
npx @lullshift/cli validate my-activity
```

or install it once with `npm install --global @lullshift/cli` and run `lullshift <command>`. The npm package carries the bundled engines and kits of its release, so it works offline after the install. The CLI is not part of the desktop app's installer.

**From a clone, until then** (and for working on Lullshift itself). You need Node.js 24 and pnpm (see [CONTRIBUTING.md](../../CONTRIBUTING.md)):

```bash
pnpm lullshift --help
```

`pnpm lullshift` runs from the repository root but resolves folders from where you typed the command. From another folder, use `pnpm --dir <path to clone> lullshift <command>`.

The examples below use `pnpm lullshift`; with the npm package, type `npx @lullshift/cli` or `lullshift` instead.

Exit codes: `0` ok, `1` problems found, `2` usage or environment error (for example, no browser found).

## Commands

### `create`

```text
lullshift create <folder> [--kind app|content] [--name "Name"] [--id reverse.dns.id]
                          [--author "You"] [--engine lullshift.cards@1]
                          [--kit grid|card|arcade]
```

Writes a small working activity into `<folder>`, then validates it.

- `--kind app` (default) copies `tools/lullshift-cli/templates/app` and the bridge.
- `--kind content` copies `tools/lullshift-cli/templates/content`. `--engine` defaults to `lullshift.cards@1`, and the starter data fits that engine.
- `--kit` copies a kit file and loads it after the bridge.
- Without `--id`, the id is `dev.local.<name>`. Use your own reverse-DNS id before publishing.

### `validate`

```text
lullshift validate <folder> [--engines <folder>]...
```

Checks the manifest against the schema, the files present, the bridge and kit copies, the help and preview, sandbox rules it can see in the code, and, for content packs, the data against the engine's schema and every picture listed in `images` (type, size, still, no metadata; PNG pixel data is fully checked, WebP structure only, so the app may still refuse a damaged WebP). Bundled engines in `activities/engines` are found automatically; `--engines` adds more folders. Treat warnings as failures before you publish.

```bash
pnpm lullshift validate activities/games/fog-garden
```

```text
✓ Fog Garden (app.lullshift.games.fog-garden@0.1.0): valid
```

### `simulate`

```text
lullshift simulate <folder> [--browser <path>] [--engines <folder>]... [--json]
```

Validates first, then loads the activity in a headless browser already on your computer (Microsoft Edge, Google Chrome or Chromium) and runs these scenarios: fresh start, keyboard focus, save, restore (save, restore, save gives the same state), working, completed, failed and cancelled events, `needs_attention` with duplicate and out-of-order events, resume, `needs_attention` before ready, start after an early pause, close, crash and recover, reduced motion and dark theme, paused-view stats, uncaught errors, and offline (no network requests). `--json` prints machine-readable results.

Pick a browser with `--browser <path>` or the `LULLSHIFT_BROWSER` environment variable. If none is found, `simulate` exits with code 2 and says so; that is not a pass.

### `dev`

```text
lullshift dev <folder> [--browser <path>] [--engines <folder>]...
```

Validates, then serves the activity in a host-like harness and opens it in a visible browser window. Files are read fresh on every load, so edit and press Start or resume. Ctrl+C stops it. If no browser is found, it prints the harness address to open yourself.

### `pack`

```text
lullshift pack <folder> [--out <file.lullpack>]
```

Validates, then writes a deterministic `.lullpack` (a ZIP of stored entries in sorted order with fixed timestamps) and prints its size and SHA-256. The default name is `<id>-<version>.lullpack`. It refuses to pack a folder that fails validation. The app installs from Git marketplaces, not from `.lullpack` files.

### `marketplace init`

```text
lullshift marketplace init [<repo folder>] --name <name> [--owner "You"]
```

Writes `.lullshift/marketplace.json` with no entries. The name is 2 to 40 characters of `a-z`, `0-9` and `-`. `lullshift` is reserved. It refuses to replace an existing file.

```text
Wrote ...\my-market\.lullshift\marketplace.json. Add activities under "activities", then validate each one.
```

There is no CLI command to add a marketplace to the app or to publish. Users add marketplaces in the app; you publish by pushing to Git.

## Other options

- `--help`, `-h`: usage.
- `--version`: prints the SDK version (`0.2.0`).
