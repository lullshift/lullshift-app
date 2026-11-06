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

Addresses with a password or token in them, IP addresses, `localhost` and local network names are refused.

Lullshift fetches the repository's list and shows its activities. Anyone can publish a marketplace, so add only ones you trust. The owner name shown comes from the repository itself and is not checked.

### Private repositories

Private repositories work through your own Git setup: your credential manager, SSH keys or the `gh` CLI. Lullshift never asks for, stores or sends Git passwords or tokens. If access is denied, Lullshift says so. Set up Git access for that repository on this computer, then try again.

## Installing

Choose **Browse activities**, pick one, then **Review before installing**. The review shows:

- the marketplace and the source repository;
- the exact commit it will install;
- whether it is a content pack or a code activity, and which engine a pack uses;
- the permissions it asks for, and its size.

Nothing is installed until you confirm. Third-party marketplaces are not reviewed by the Lullshift project.

**Content packs** (data for a bundled engine, no code) can be installed from any marketplace.

**Code activities from third-party marketplaces can't be installed in this release.** They are listed but blocked, with the message "Code activities from third-party marketplaces can't be installed on this system yet. Content packs can." This stays in place until the activity sandbox has passed its security checks on each operating system.

## Pinning

Every install is pinned to one commit. Lullshift stores its own copy of the files and checks them each time it starts the activity. If the files on disk no longer match, the activity is turned off and **Manage** shows "Changed after install".

Lullshift never runs an activity straight from a branch, and never updates anything on its own.

## Updating

Updates happen only when you ask:

1. Choose **Refresh** on the marketplace. This fetches its latest list.
2. Choose **Review the update** on an installed activity. The review shows the old and new version and commit, any permissions added or removed, and a highlighted **Source changed** row if the update comes from a different repository.
3. Confirm to update. If you don't, nothing changes.

## Removing and uninstalling

- **Remove marketplace** deletes Lullshift's copy of the repository. Activities you installed from it keep working, but can't be updated until you add it again from the same address.
- **Uninstall** an activity in **Manage**. Tick **Also delete saved progress** to remove its progress too.

## Where the files are

Marketplace caches are in `<app data>/marketplaces/` and installed activities in `<app data>/installed/`. See [Privacy](privacy.md#where-your-data-lives) for the app data folder on each system.

Publishing your own marketplace: see [Publishing a marketplace](../creators/publishing-a-marketplace.md).
