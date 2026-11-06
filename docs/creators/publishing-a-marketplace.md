# Publishing a marketplace

A marketplace is any Git repository, public or private, with `.lullshift/marketplace.json` at its root (ADR-002). There is nothing to register and nothing to upload. Users add your repository in the app and install from it with their own Git access.

Users can install **content packs** and **code activities** (`kind: app`) from third-party marketplaces. A code activity is listed as a "Third-party program". Before it installs, the user sees a warning, "This is someone else's program", and must tick "I understand and want to install this program". The warning lists the permissions your manifest declares and what Lullshift has verified on their platform. They tick it again for every update that changes your code (a new commit), so push code changes deliberately. See [Sandbox rules](sandbox.md#known-limits).

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
- `description` (up to 500 characters) and `owner` (up to 100) are plain text. That means no control characters, line or paragraph separators, bidirectional controls, zero-width or other invisible characters, and no more than four combining marks in a row. The schema, `lullshift validate` and the app all apply the same rule. `owner` is shown as self-declared and unverified.

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

- `git` is `https://host/path` or GitHub shorthand `owner/repo`. `ssh://` and `git@host:path` are refused for an entry: Lullshift checks where a name resolves and pins an https fetch to those addresses, and ssh can't be pinned. (People can still add a whole marketplace by an ssh address they type themselves.) No passwords or tokens in the address, no IP addresses, `localhost` or local network names. A user can confirm that a whole marketplace lives on their own network, but an entry can never send their app to a private address.
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
- for content packs: anything besides the manifest, the data file (at most 256 KiB), the preview image (at most 256 KiB), `.md`/`.txt` notes and the pictures listed in `images` (PNG or WebP in `images/`, at most 20, each at most 512 KiB and 2048 × 2048 pixels, still, without metadata; see [Engines and content packs](engines-and-content-packs.md#pictures)).

Never commit secrets. Lullshift ignores Git hooks, filters and LFS in your repository.

## 5. Push

Commit and push. Users add the repository in **Activities**, **Marketplaces**, then **Add marketplace**, with `owner/repo` or a full Git URL.

For a private repository, each user needs their own Git access to it (credential manager, SSH key or `gh`). Lullshift never handles credentials.

Lullshift does not let Git follow HTTP redirects. If you move the repository:

- **Same server**, for example a rename on GitHub: when a fetch fails, the app asks the server once where the repository went, without credentials. It offers users the new address. They can switch with **Change address**, which keeps their installs and progress.
- **Different server**: users see the new address and must add it deliberately.

Either way, keep every commit that users installed reachable at the new address. **Change address** checks each pinned commit there. Installs whose commit is missing are flagged "Not found at the new address".

Users can also add a marketplace hosted on their own computer or network after an explicit confirmation. Plain `http://` is never accepted.

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
