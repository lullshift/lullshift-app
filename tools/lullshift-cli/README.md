# @lullshift/cli

`lullshift` is the creator CLI for [Lullshift](https://github.com/lullshift/lullshift-app) activities (ADR-017). It creates, checks, simulates and packs activities on your computer. It never uses the network and needs no account.

`simulate` and `dev` need Microsoft Edge, Google Chrome or Chromium already installed.

**From a clone** of [lullshift/lullshift-app](https://github.com/lullshift/lullshift-app) (Node 24 and pnpm): `pnpm install`, then `pnpm lullshift <command>`.

**From npm, only once it has been released there.** This package is published only by the lullshift/lullshift-app repository's publish workflow, with npm provenance. Before running it, check that its npmjs.com page links to `github.com/lullshift/lullshift-app` and shows a provenance badge; if it doesn't, it isn't ours. Node 22.12 or newer:

```bash
npx @lullshift/cli create my-activity --name "My Activity" --id dev.you.my-activity
npx @lullshift/cli validate my-activity
npx @lullshift/cli simulate my-activity
npx @lullshift/cli pack my-activity
```

Or install it once with `npm install --global @lullshift/cli`, then run `lullshift <command>`.

| Command | What it does |
|---|---|
| `create <folder>` | Writes a small working activity (`--kind app` or `--kind content`, `--kit grid\|card\|arcade`). |
| `validate <folder>` | Checks the manifest, files, bridge and kit copies, help, preview and sandbox rules. |
| `simulate <folder>` | Runs the interruption scenarios in a headless browser. |
| `dev <folder>` | Opens the activity in a host-like harness to play and interrupt by hand. |
| `pack <folder>` | Writes a deterministic `.lullpack` after validation passes. |
| `marketplace init` | Writes an empty `.lullshift/marketplace.json`. |

Exit codes: `0` ok, `1` problems found, `2` usage or environment error. Run `lullshift --help` for every option.

The package carries the bundled engines and kits of the Lullshift version it was built from, so `validate` and `simulate` work offline for content packs too.

Full reference: [docs/creators/cli.md](https://github.com/lullshift/lullshift-app/blob/main/docs/creators/cli.md). Licence: Apache-2.0.
