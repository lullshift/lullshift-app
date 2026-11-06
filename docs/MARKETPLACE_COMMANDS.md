# Marketplace commands contract (Sprint 08)

This is the contract between the Tauri backend (`apps/desktop/src-tauri`) and the React UI (`apps/desktop/src`). All JSON uses camelCase. Every command returns `Result<T, CommandError>`, where `CommandError` is `{ kind: string, message: string }`. `message` is a short, friendly sentence that is safe to show: no git stderr, no URL userinfo, no raw serde text.

`kind` is one of:
- `offline`
- `authDenied`
- `notFound`
- `timeout`
- `gitMissing`
- `needsGit` (decision 28, ADR-023: Lullshift is using its built-in Git, and this marketplace needs installed Git: it asks for a sign-in, uses ssh, or is only reachable through a proxy)
- `invalidUrl`
- `invalidIndex`
- `invalidPack`
- `thirdPartyCodeBlocked`
- `alreadyExists`
- `builtinProtected`
- `busy`
- `tampered`
- `unreadable` (S12-T14: `activity_set_enabled(true)` found a file of the install that exists but can't be read right now, for example locked by another program; nothing changed, try again)
- `privateHost` (Sprint 12: a private address that the user hasn't confirmed, or a name that resolved to one)
- `moved` (Sprint 12: the repository answered with a redirect; `CommandError` also carries `moved: { url, sameHost }`, with `url` a display https address)
- `internal`

Long-running commands (add, refresh, preview) must not block the UI thread. Use async commands with spawn_blocking. Only one git operation runs per marketplace at a time; a second one returns `busy`.

## Types

```ts
type Marketplace = {
  id: string;                 // index `name`; "lullshift" for the built-in
  url: string;                // display form, never with userinfo
  builtin: boolean;
  description: string | null; // from the index, plain text
  owner: string | null;       // self-declared, unverified
  lastFetchedCommit: string | null;
  lastFetchedAt: string | null; // RFC 3339
  privateHost: boolean;       // Sprint 12: the user confirmed this marketplace is on their own computer or network
};

type MarketplaceEntry = {
  activityId: string;
  name: string | null;        // from the manifest when it can be read cheaply (same-repo source), else null
  description: string | null;
  kind: "app" | "content" | null;
  external: boolean;          // source is another git repo
  featured: boolean;
  installed: { commit: string; version: string; enabled: boolean } | null;
  blocked: "thirdPartyCode" | null; // known up front for a same-repo kind:app entry in a third-party marketplace
};

type InstallPreview = {
  token: string;              // opaque, single use, expires after 10 minutes
  marketplaceId: string;
  activityId: string;
  name: string;
  version: string;
  description: string;
  sourceUrl: string;          // display form
  sourcePath: string | null;
  commit: string;             // 40 hex
  kind: "app" | "content";
  engine: string | null;      // "id@major"
  permissions: string[];
  sizeBytes: number;
  fileCount: number;
  thirdParty: boolean;        // true for any non-built-in marketplace
  hasSavedState: boolean;     // saved progress from an earlier install of this id exists and will be used
  savedState: "none" | "own" | "unknown" | "foreign"; // Sprint 12: "foreign" means the review asks delete or keep
};

type AddressChanged = {       // Sprint 12
  marketplace: Marketplace;
  notFound: string[];         // activity ids whose pinned commit wasn't at the new address
};

type UpdatePreview = InstallPreview & {
  from: { commit: string; version: string; permissions: string[] };
  addedPermissions: string[];
  removedPermissions: string[];
  kindChanged: boolean;
  engineChanged: boolean;
  sourceChanged: { old: string; new: string } | null; // update comes from another repository than the install did (display URLs)
  unchanged: boolean;         // same commit: nothing to update
};
```

## Commands

- `marketplace_list() -> Marketplace[]`
- `marketplace_git_backend() -> { backend: "system" | "builtin" }` (ADR-023: which Git marketplaces use; chosen once per run, no network)
  - The built-in marketplace comes first.
- `marketplace_add({ url, confirmPrivate? }) -> Marketplace`
  - Sprint 12: a private address needs `confirmPrivate: true`, otherwise `privateHost`. A redirect returns `moved`.
  - Validates the URL, fetches, reads and validates the index, and uses the index `name` as the id.
  - Fails with `alreadyExists` if the id is taken, and with `invalidIndex` if the name is `lullshift`.
  - A name that was removed while installs from it stayed can be added again only from the same address it had. A different address fails with `alreadyExists` ("removed earlier ... different address"), so installs never silently take their updates from a new source. Uninstall them first, or add it from the original address.
  - Only the user's action triggers this.
- `marketplace_remove({ id }) -> void`
  - Returns `builtinProtected` for `lullshift`.
  - Deletes the cache. Installs are kept.
- `marketplace_refresh({ id, confirmPrivate? }) -> Marketplace`
  - Fetches again. For the built-in, it returns the record without any network call.
  - Sprint 12: `confirmPrivate: true` records the private-host confirmation after a successful fetch. A redirect returns `moved`.
- `marketplace_change_address({ id, url, confirmPrivate? }) -> AddressChanged` (Sprint 12)
  - `url` gets the same validation as `marketplace_add`.
  - Refuses the built-in (`builtinProtected`) and the current address (`invalidUrl`).
  - Fetches the index from the new address. Its `name` must equal `id`, otherwise `invalidIndex`.
  - Fetches every pinned commit of this marketplace's installs from the new address:
    - **found:** the install's source moves to the new address;
    - **missing:** the install keeps its old source and gets `sourceNote: "notFoundAtNewAddress"`;
    - **any other failure:** the change is aborted with nothing changed.
  - Installs and saved progress are kept.
- `marketplace_entries({ id }) -> MarketplaceEntry[]`
  - Reads the cached index, with no network.
  - For the built-in, lists the bundled activities, excluding engines.
- `marketplace_preview_install({ marketplaceId, activityId }) -> InstallPreview`
  - May fetch an external source.
  - Fails with `thirdPartyCodeBlocked` for a third-party `kind: app`.
  - Prepares the pack in memory or in a temp dir, but installs nothing.
- `marketplace_install({ token, keepSavedState? }) -> void`
  - Writes the pack, records it in the store, and makes it enabled.
  - Sprint 12: `keepSavedState: true` keeps progress the review reported as `savedState: "foreign"`. Missing or false deletes it, as before. It has no effect for other values.
- `marketplace_preview_update({ activityId }) -> UpdatePreview`
  - Uses the marketplace's last fetched index. The UI calls refresh first.
- `marketplace_update({ token }) -> void`
  - Swaps atomically, removes the old commit folder, and keeps the enabled flag.
- `activity_uninstall({ activityId, deleteSavedState }) -> void`
  - Works for installed activities only. Returns `builtinProtected` for bundled ones.
- `activity_set_enabled({ activityId, enabled }) -> void`
  - Works for bundled activities (through `activity_disabled`) and for installed ones.

## Changes to existing activity commands

- **`activities_list`:**
  - Returns enabled bundled activities plus installed ones whose digest verifies.
  - Each item gets `source: { marketplaceId: string, builtin: boolean, commit: string | null }`.
  - Disabled activities are left out.
  - An installed activity whose digest no longer matches is left out and disabled. `activities_manage_list` reports it as `tampered`.
- **`activities_manage_list() -> ManageItem[]`:** lists every bundled and installed activity, including disabled ones, so the user can re-enable them. Each item is `{ activityId, name, kind, builtin, marketplaceId, enabled, commit, version, problem: "tampered" | "missingEngine" | "unreadable" | null, sourceNote: "notFoundAtNewAddress" | null }` (`sourceNote` since Sprint 12).
- **Serving installed activities:**
  - Same sandbox, CSP, path rules and engine-pack rules as bundled ones.
  - Files are read from `<app data>/installed/<id>/<commit>/`.
  - An installed content pack runs on a bundled engine, using the existing pack runtime.
  - Never read anything from the marketplace cache.

## Implementation notes and deviations (backend, Sprint 08)

Code: `apps/desktop/src-tauri/src/market.rs` (commands), `activities.rs` and `activities/installed.rs` (listing, serving, verification). Tests: `src/market/tests.rs`.

**Additions and clarifications (none changes a shape above)**

- `activities_manage_list` is a new command (permission `allow-activities-manage-list`). Bundled engines are not items; in debug builds the sandbox probe appears like any bundled activity. `kind` is `"app"` or `"content"`; `name` is the manifest name (or the id if the files can't be read); `commit` is null for bundled ones.
- `problem` is `"tampered"` when the install folder no longer matches the digest recorded at install, **and** also for a record this release must not run (a third-party `kind: app` record, which cannot be created through the commands). `"missingEngine"` is used when the bundled engine is gone **or no longer accepts the pack's data**.
- Tokens: an unknown, used, expired (10 minutes) or wrong-kind token returns `notFound` ("This review has expired or was already used"). Every token works once, even if the command then fails. At most 8 reviews are held in memory; the oldest is dropped.
- `marketplace_update` returns `notFound` if the installed activity changed (commit or marketplace) since the review, and succeeds as a no-op when the previewed commit is the installed one (`unchanged: true`).
- `sourceChanged` (update review): the install's recorded source address against the address the update would come from. It is set when they differ (for example an index entry that now points at another repository), counts toward "needs your approval", and the UI shows it as a highlighted "Source changed" row with both addresses.
- `hasSavedState` (install review): true when saved progress exists for the id and the install will use it; the UI says "Saved progress from an earlier install of this id will be used." When an activity is uninstalled with its saved state kept, the marketplace and source it came from are recorded (migration 0007). Installing the id from a different marketplace or source deletes that state at install time (storage values, snapshot and play time), unless the user chose to keep it (`savedState: "foreign"` and `keepSavedState: true`, Sprint 12). `hasSavedState` is false for that case. State with no recorded owner (older than 0007) is kept and announced.
- Installs, updates and uninstalls also take a per-activity-id lock across marketplaces; a second one on the same id returns `busy`.
- Sprint 12 (decision 18.5): no file bytes are held in memory. Every read of an installed file is read again and hashed again against the snapshot's check before it is used. That covers the manifest, a pack's data, the preview image, a pack's listed pictures and an app's served files. A mismatch serves nothing and turns the activity off, so `problem` becomes `"tampered"`. `activity_content` returns the content error code `damaged`. A file that is there but can't be read right now (another program holds it, for example) is not a mismatch (S12-T14 security review, INFO-1): that one use is refused (`activity_content` returns `unreadable`, a served file is a 404, the preview is null), nothing is turned off, `problem` is `"unreadable"` while it lasts, and the next use checks again. A missing file or folder, a link or anything else that isn't a plain file still counts as a mismatch.
- `marketplace_preview_install` returns `alreadyExists` for the built-in marketplace (its activities are the bundled ones), for any id that exists in the bundled catalog (an installed activity may never shadow a bundled one, engines included), and for an id that is already installed (use update).
- `marketplace_entries` returns `notFound` for a marketplace that was never fetched ("Refresh it first"). For same-repo entries, name, description and kind come from `manifest.json` at the fetched commit (one batch of Git calls, no network); entries from another repository have `name`/`description`/`kind` null.
- `marketplace_refresh` fails with `invalidIndex` if the index's `name` no longer equals the marketplace id.
- `marketplace_remove` also deletes the external-source caches (`<cache>/external`). Installed activities keep working but can't be updated while their marketplace is removed (`marketplace_preview_update` returns `notFound`).
- `marketplace_add` accepts what `GitUrl::parse` accepts: `https://`, `ssh://`, `git@host:path` and `owner/repo`; anything else is `invalidUrl`.
  - Sprint 12: IP literals, single-label names and local suffixes parse, but they are private and need `confirmPrivate`.
  - Unspecified, link-local, multicast and reserved addresses are always `invalidUrl`.
  - Before each fetch the name is resolved and checked, and https fetches are pinned to the checked addresses (ADR-015 amendment).
- `alreadyExists` for a removed name re-added from a different address now says to use Change address.
- `activity_set_enabled(true)` on an installed activity re-verifies its files and returns `tampered` (staying off) if they differ. For a bundled activity it only toggles the library entry.
- `activity_uninstall` removes the record first (so it stops being served), then deletes the saved state if asked (storage values, saved snapshot and play time of that activity, in one transaction: `Store::clear_activity_state`), then the install folder.
- Disabled built-ins are refused by `activity_info`, `activity_content` and the `lullact` scheme, and hidden from `activities_list`. Their saved state, play time and snapshot commands keep working.
- `activity_preview` and `activity_content` now take the app state (injected by Tauri; no change to the JavaScript call).
- `Marketplace.url` is the display form (no userinfo). For `git@host:path` it is `host:path`; the address Git is given is kept in `<cache>/<id>/source.json` and re-validated on every use.

**Where things live**

- Cache: `<app data>/marketplaces/<id>/repo.git` (+ `external/`, `source.json`). Installs: `<app data>/installed/<id>/<commit>/`. Installed files are never read from the cache.
- Verification (Sprint 12): there is no per-session cache any more. Every snapshot build hashes every install and keeps only the per-file length and SHA-256. Install, update, uninstall, enable, disable and a damaged file all rebuild it.
- Serving an installed pack: the bundled engine's files, under the same CSP and path rules. The pack's data is read, re-hashed and validated against the engine schema each time `activity_content` is called, so later edits on disk can't reach the engine. Installed code activities (not installable in this release) are read and re-hashed file by file from the install folder.

**Test support**

`file://` repositories exist only for tests: `Git::for_tests` and `GitUrl::for_test_file` are compiled when the `marketplace` crate's `unsafe-test-file-urls` feature is on **and** `debug_assertions` is on. The desktop crate enables the feature only in `[dev-dependencies]` (resolver 3 does not unify dev-dependency features into normal builds), and a release build cannot contain the code either way. The production commands parse URLs with `GitUrl::parse`, which refuses `file://`.
