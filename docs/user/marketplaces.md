# Marketplaces

A marketplace is a Git repository with a `.lullshift/marketplace.json` file that lists activities. Anyone can publish one. There is no central store and no upload.

Open **Activities**, then the **Marketplaces** tab.

## The built-in marketplace

`lullshift` is always listed first, marked **Built in**. Its activities are the ones bundled with the app. They are reviewed by pull request in the Lullshift repository, ship with each app release and need no network. You can turn them off in **Manage**, but you can't remove them or the built-in marketplace.

## You need Git

Adding and updating marketplaces runs the `git` already installed on your computer. If Git is missing, Lullshift says "Git isn't installed, or isn't on your PATH." Bundled activities work without Git.

## Adding a marketplace

Type the repository address and choose **Add marketplace**. Lullshift accepts:

- GitHub shorthand: `owner/repo`
- `https://host/path`
- `ssh://host/path` or `git@host:path`

Addresses with a password or token in them, and plain `http://` addresses, are refused.

Lullshift fetches the repository's list and shows its activities. Anyone can publish a marketplace, so add only ones you trust. The owner name shown comes from the repository itself and is not checked.

### A marketplace on your own computer or network

These count as private addresses:

- an IP address;
- `localhost`, or a one-word name like `gitbox`;
- a name ending in `.local`, `.lan`, `.internal`, `.home`, `.corp` and similar.

Lullshift asks before using a private address. Tick **This marketplace is on my own computer or network**, then add it again. Only tick it for a Git server you or your organisation runs. The answer is remembered for that marketplace, which shows **On your network**.

Before each fetch, Lullshift also looks up where a name points. If a normal-looking name points at your own network and you didn't confirm it, the fetch is refused. Some addresses are never used, whatever you answer, such as link-local and multicast addresses.

### If the repository moved

Lullshift doesn't let Git follow redirects. When a fetch fails, it asks the server once whether the repository moved. It sends no passwords or tokens.

- **Same server:** Lullshift shows the old and new address and offers **Use the new address**.
- **Different server:** Lullshift shows the new address and puts it in the address box. Check that it's what you expect before you add it.

### Changing a marketplace's address

Choose **Change address** on a marketplace, type the new address and confirm. Lullshift:

1. fetches the list from the new address. It must have the same name.
2. checks that every commit you installed from it also exists at the new address.

Your installs, pins and saved progress are kept. If an installed commit isn't at the new address, that activity keeps working as installed, and **Manage** shows "Not found at the new address" until you update it. If the new address can't be reached, nothing changes.

### Private repositories

Private repositories work through your own Git setup: your credential manager, SSH keys or the `gh` CLI. Lullshift never asks for, stores or sends Git passwords or tokens. If access is denied, Lullshift says so. Set up Git access for that repository on this computer, then try again.

## Installing

Choose **Browse activities**, pick one, then **Review before installing**. The review shows:

- the marketplace and the source repository;
- the exact commit it will install;
- whether it is a content pack or a code activity, and which engine a pack uses;
- the permissions it asks for, and its size.

Nothing is installed until you confirm. Third-party marketplaces are not reviewed by the Lullshift project.

If this device has saved progress for an activity with the same id from a **different** marketplace or repository, the review asks what to do with it:

- **Delete that saved progress** is chosen by default.
- **Keep it for this install** comes with a warning, because the new activity can read everything the old one saved. Only keep it if you trust both sources.

Progress from the same marketplace and repository is reused without asking.

**Content packs** (data for a bundled engine, no code) can be installed from any marketplace.

**Code activities from third-party marketplaces can't be installed in this release.** They are listed but blocked, with the message "Code activities from third-party marketplaces can't be installed on this system yet. Content packs can." This stays in place until the activity sandbox has passed its security checks on each operating system.

## Pinning

Every install is pinned to one commit. Lullshift stores its own copy of the files. It checks each file against what it installed every time it reads it, including when it starts the activity or shows its picture. If a file on disk no longer matches, the activity is turned off and **Manage** shows "Damaged, reinstall it." To use it again, uninstall it and install it again from its marketplace.

Lullshift never runs an activity straight from a branch, and never updates anything on its own.

## Updating

Updates happen only when you ask:

1. Choose **Refresh** on the marketplace. This fetches its latest list.
2. Choose **Review the update** on an installed activity. The review shows the old and new version and commit, any permissions added or removed, and a highlighted **Source changed** row if the update comes from a different repository.
3. Confirm to update. If you don't, nothing changes.

## Removing and uninstalling

- **Remove marketplace** deletes Lullshift's copy of the repository. Activities you installed from it keep working, but can't be updated until you add it again from the same address. If it moved and the old address still answers, add it back from the old address, then use **Change address**. If the old address is gone, uninstall its activities first, then add the new address.
- **Uninstall** an activity in **Manage**. Tick **Also delete saved progress** to remove its progress too.

## Where the files are

Marketplace caches are in `<app data>/marketplaces/` and installed activities in `<app data>/installed/`. See [Privacy](privacy.md#where-your-data-lives) for the app data folder on each system.

Publishing your own marketplace: see [Publishing a marketplace](../creators/publishing-a-marketplace.md).
