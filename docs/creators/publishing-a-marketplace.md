# Publishing a marketplace

A marketplace is any Git repository, public or private, with `.lullshift/marketplace.json` at its root (ADR-002). There is nothing to register and nothing to upload. Users add your repository in the app and install from it with their own Git access.

In this release, users can install **content packs** from third-party marketplaces. Code activities (`kind: app`) are listed but can't be installed yet; see [Sandbox rules](sandbox.md#known-limits).

## 1. Create the index

```bash
pnpm --dir <path to clone> lullshift marketplace init my-market --name my-market --owner "Your Name"
```

This writes `my-market/.lullshift/marketplace.json`:

```json
{
  "schemaVersion": 1,
  "name": "my-market",
  "description": "Activities for Lullshift.",
  "owner": "Your Name",
  "activities": []
}
```

- `name` is 2 to 40 characters of `a-z`, `0-9` and `-`, and becomes the marketplace's id in the app. `lullshift` is reserved for the built-in marketplace. Keep the name stable: the app refuses a refresh if it changes.
- `description` (up to 500 characters) and `owner` (up to 100) are plain text. `owner` is shown as self-declared and unverified.

The schema is [reference/MARKETPLACE_V1.schema.json](../../reference/MARKETPLACE_V1.schema.json).

## 2. Add entries

Each entry has an `id`, which must equal the `id` in the activity's `manifest.json`, and a `source`. It may set `"featured": true`.

A folder in the same repository:

```json
{ "id": "dev.you.tea-quiz", "source": { "path": "packs/tea-quiz" } }
```

A folder in another Git repository:

```json
{
  "id": "dev.you.desk-moves",
  "source": { "git": "you/lullshift-packs", "ref": "v1.0.0", "path": "desk-moves" }
}
```

- `git` is `https://host/path`, `ssh://host/path`, `git@host:path` or GitHub shorthand `owner/repo`. No passwords or tokens in the address, no IP addresses, `localhost` or local network names.
- `ref` is a tag, branch or commit. Prefer a tag or a commit. Whatever you give, the install is pinned to the commit it resolves to.
- Paths use `/`, at most 12 parts. Each part starts with a letter, digit or underscore, so `..`, `.git` and hidden folders are impossible.
- At most 1000 entries.

## 3. Check every activity

For each listed folder:

```bash
pnpm --dir <path to clone> lullshift validate <folder>
pnpm --dir <path to clone> lullshift simulate <folder>
```

Fix every error and warning. Then check by hand:

- the entry `id` matches the manifest, and the `path` exists;
- licences are declared and you hold the rights to everything you publish;
- original names and art, no copies of branded games.

## 4. Keep the repository clean

The app reads files straight from Git objects and refuses packs that break these rules, so keep activity folders free of:

- symlinks, submodules, executable files and Git LFS pointers;
- non-ASCII file names, or names that differ only by case;
- more than 512 files, files over 5 MiB, or more than 20 MiB per pack;
- for content packs: anything besides the manifest, the data file (at most 256 KiB), the preview image (at most 256 KiB) and `.md`/`.txt` notes.

Never commit secrets. Lullshift ignores Git hooks, filters and LFS in your repository.

## 5. Push

Commit and push. Users add the repository in **Activities**, **Marketplaces**, then **Add marketplace**, with `owner/repo` or a full Git URL.

For a private repository, each user needs their own Git access to it (credential manager, SSH key or `gh`). Lullshift never handles credentials.

Lullshift does not follow HTTP redirects. If you move the repository, users must add it again at the new address.

## How users get updates

Nothing updates automatically. When a user chooses **Refresh**, Lullshift fetches your index from the default branch. When they review an update, they see:

- the old and new version and commit;
- any added or removed permissions;
- a **Source changed** warning if the entry now points at another repository.

So:

- bump `version` in the manifest whenever you change an activity;
- add permissions only when you need them, because users must approve them;
- don't change an entry's `id`. A new id is a new activity.

## Review

There is no central review of third-party marketplaces. The install screen says so. Users see your repository address, the exact commit, the kind of activity and its permissions before they install. The commit pin and the sandbox are the trust baseline; there is no package signing in v1.

The built-in marketplace in this repository is the only reviewed one. To add to it, open a pull request: see [CONTRIBUTING.md](../../CONTRIBUTING.md).
