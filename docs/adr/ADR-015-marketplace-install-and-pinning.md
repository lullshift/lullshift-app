# ADR-015: Marketplace index hardening, Git invocation and install semantics

- **Status:** Accepted (Sprint 08), pending product-owner review (see `sprints/DECISIONS_FOR_REVIEW.md`)
- **Date:** 2026-11-05
- **Owner:** Implementation lead
- **Related:** ADR-002 (Git marketplaces), ADR-003 (activity model), ADR-012 (sandbox), ADR-016 (engines and packs), TECHNICAL_ARCHITECTURE §7

## Context

ADR-002 makes any Git repository a marketplace and pins installs to commits. Sprint 08 builds the manager. The repository, its index and every file in it are attacker-controlled, and `git` itself has a long history of option-injection, hook and transport issues. The Sprint 00 security review listed gaps in the v1 index schema (credentials in URLs, option injection, unsafe refs and paths).

## Decision

### Where the code lives

A new crate, `crates/marketplace`, owns: URL, ref and path validation; the hardened `git` runner; fetching into the cache; index parsing; resolving an entry to a pinned commit; copying and strictly validating a pack folder into the install directory; and digests. It has no Tauri dependency and is tested against local repositories built by the tests. `local-store` gains the `marketplaces` and `installed_activities` tables (migration 6). The desktop crate exposes commands and serves installed activities from disk beside the bundled ones.

### Index schema v1, tightened (still `schemaVersion: 1`)

The changes only refuse values that were never safe; no valid index in this repo changes.

- `git` URL: `https://host/path` or `ssh://[user@]host[:port]/path` or `git@host:path`, or GitHub shorthand `owner/repo`. Refused: userinfo with a password or token in https (`user:pass@`, any `@` before the host in https), a host or path segment starting with `-`, whitespace and control characters, `..` or `.git` as a whole path segment except a final `.git` suffix, loopback, link-local and private-range IP literals and `localhost`. Shorthand owner and repo may not start with `-` or `.`.
- `ref`: `^(?!-)(?!.*\.\.)(?!.*[:^~*?\[\\\s])(?!.*@\{)[A-Za-z0-9._/-]{1,200}$` and no `.lock` suffix or `//`; at run time it must also pass `git check-ref-format --allow-onelevel`, or be a 40-hex commit SHA.
- `relativePath`: `/`-separated segments; each segment non-empty, not `.` or `..` or `.git`, no leading `-`, not a Windows reserved name (`CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`, with or without extension), no trailing dot or space.
- `name`: `lullshift` is reserved; only the built-in index may use it (checked at run time, since the schema can't know who is loading it).

The same rules are enforced in Rust at run time; the schema and the Rust code share one fixture set.

### Git invocation

Every call is an argument vector (never a shell) with:

- environment: `GIT_ALLOW_PROTOCOL=https:ssh`, `GIT_TERMINAL_PROMPT=0`, `GIT_CONFIG_NOSYSTEM=1`, `GIT_ASKPASS` and `SSH_ASKPASS` left as the user set them (so credential managers work), `GCM_INTERACTIVE` untouched, `GIT_LFS_SKIP_SMUDGE=1`, `GIT_SSH_COMMAND` left to the user (we never build ssh commands);
- `-c core.hooksPath=<empty app-owned dir> -c core.fsmonitor=false -c protocol.allow=never -c protocol.https.allow=always -c protocol.ssh.allow=always -c submodule.recurse=false -c core.symlinks=false -c core.longpaths=true -c filter.lfs.required=false -c filter.lfs.process= -c filter.lfs.smudge= -c filter.lfs.clean=`;
- `--end-of-options` (or `--`) before every URL, ref and path;
- `clone --no-checkout --filter=blob:none --no-tags --single-branch` (or a plain fetch of one ref), then file content is read from the pinned tree with `git ls-tree`/`cat-file`, never by checking out a working tree. So no repository file is ever written to disk as a file by Git, `.gitattributes` filters never run and symlinks are never created;
- a timeout per call (60 s fetch, 10 s local commands), output caps, and the process tree killed on timeout or cancel.

`file://` is allowed only in tests, through a constructor that is not reachable from the app's commands.

### Cache, install and pinning

- Cache: `<app data>/marketplaces/<id>/repo.git` (bare). Installs: `<app data>/installed/<activity id>/<commit>/`, written by Lullshift from blobs it read and checked. Never executed from the cache.
- Resolve: fetch the index at the marketplace's default branch (or at a commit the user pins), validate it, pick the entry; for an external source fetch that repository and resolve `ref` (or `HEAD`) to a commit SHA.
- Validate the pack tree before writing anything: only regular blobs (`100644`; `100755` and `120000` symlinks and `160000` submodules are refused), canonical paths under the rules above, no case-insensitive or Unicode-normalisation collisions, allowlisted extensions per kind (content packs: manifest, data, preview, `.md`/`.txt`), at most 512 files, 5 MiB per file, 20 MiB per pack; manifest and engine data checked with the same validators as bundled activities; permissions must be known; content packs must name an installed engine major.
- Write to a temporary sibling directory, compute the SHA-256 digest of the canonical file list, then rename into place. Record `InstalledActivity` (marketplace, source URL, path, commit, manifest version, kind, engine, permissions, digest, enabled).
- Serve installed activities only after the digest still matches; a mismatch disables the activity and says so.

### Review, updates, removal

- Install needs an explicit confirmation of a review that shows marketplace, source URL, commit, kind, engine, permissions, size and an "unverified third-party" notice.
- Updates are checked only when the user asks (Refresh). An update shows old and new version and commit and every added or removed permission; it installs only on explicit confirmation, into a new commit folder, and the old one is removed after the swap. Nothing updates silently.
- Uninstall deletes the install folder; the saved state is deleted only if the user ticks it.
- Removing a marketplace leaves its installed activities in place (they are pinned copies) but stops updates; the cache is deleted.

### Third-party code

Third-party code activities (`kind: app` from a non-built-in marketplace) can be listed and reviewed but **not installed** on any OS in this release, because the Sprint 03 sandbox proof has an open WebRTC gap on Windows and has not run on macOS or Linux. The gate is one host constant per OS, defaulting to blocked; flipping it is a product-owner decision after the gap is closed. Content packs from third-party marketplaces can be installed.

### Built-in marketplace

`lullshift` is always present, cannot be removed or refreshed, and its activities are the bundled ones. Each can be disabled and re-enabled; a disabled activity disappears from the library but keeps its saved state.

## Consequences

- Private repositories work through whatever the user's `git` already uses; Lullshift never reads, stores or logs credentials, and scrubs URLs of userinfo before showing or logging them.
- Reading blobs instead of checking out avoids a whole class of Git checkout attacks, at the cost of more `git` calls per install (fine for packs of at most 512 files).
- Real private-repo installs can only be tested by a person with access (manual test).
- The "no local hosts" URL rule is **text only**. It refuses `localhost`, IP literals and local-looking suffixes in the address, but a DNS name can still resolve to a loopback, link-local or private address (DNS rebinding, or a public name pointing inward). Lullshift does not resolve names itself, so it cannot stop that; redirects are off (see below), the fetch is size-capped, and nothing fetched is executed.

## Implementation notes and deviations (`crates/marketplace`)

Recorded where the shipped crate differs from, or is stricter than, the text above.

- **Fetch is a shallow fetch, not `clone --filter=blob:none`.** `git fetch --depth=1 --no-tags <url> +<ref|HEAD>:refs/lullshift/fetched` into a bare repository created with `git init --bare --template=`. A partial clone would leave a promisor remote and make `cat-file` fetch blobs lazily, outside the call that was timed and capped; the shallow fetch brings the one tree we read and nothing is fetched later (`GIT_NO_LAZY_FETCH=1`). The `-c` set and the environment in this ADR are used as written, plus `gc.auto=0` and `maintenance.auto=false` (so automatic maintenance cannot delete commits that installs are pinned to), `LC_ALL=C` (so stderr can be classified), and the removal of inherited `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, `GIT_OBJECT_DIRECTORY`, `GIT_ALTERNATE_OBJECT_DIRECTORIES`, `GIT_NAMESPACE`, `GIT_COMMON_DIR`, `GIT_CONFIG_PARAMETERS`, `GIT_CONFIG_COUNT`, `GIT_EXTERNAL_DIFF` and `GIT_TEMPLATE_DIR`. `GIT_ASKPASS`, `SSH_ASKPASS` and `GIT_SSH_COMMAND` are inherited untouched.
- **URLs are stricter than listed.** Only letters, digits and `. _ - / : @` are accepted (no `%`, `=`, brackets or non-ASCII). The host must be a DNS name with at least two labels whose last label starts with a letter, so **every IP literal is refused** (not only loopback, link-local and private ranges), as are `localhost` and the suffixes `.local .internal .lan .localdomain .intranet .corp .home .private .arpa`. `ssh://` may carry a user name but never a password; `git@host:path` is normalised to `ssh://git@host/path` before it reaches `git`; the display form never has userinfo. Path parts must start with a letter, digit or underscore (so `.`, `..`, `.git`, hidden names and a leading `-` are impossible); a final `repo.git` is fine.
- **Index paths** use the same part rule plus: at most 12 parts of at most 64 characters, no trailing `.`, no Windows reserved names. `description` and `owner` may not contain control, bidirectional-override or invisible characters (the class used for manifest text).
- **Pack paths**: non-ASCII file names are refused outright, so Unicode-normalisation collisions cannot occur; case collisions are found with an ASCII lowercase key. At most 8 path parts. A file and a folder with the same name are refused.
- **Content packs** may contain `manifest.json`, the manifest's `data` file, the manifest's `preview` image (at most 256 KiB) and `.md`/`.txt` files. Any other file, including any other `.json`, is refused. Manifests are checked with the host's own `parse_manifest` rules (moved into the crate and shared with the desktop crate) plus: no unknown top-level field, `offline: true`, unique known permissions, valid tags. The data file is at most 256 KiB and is checked with the host's engine data validator (also moved into the crate).
- **`file://`** exists only behind `#[cfg(test)]` (`Git::for_tests`, `GitUrl::for_test_file`) and, for the desktop crate's command tests, behind the `unsafe-test-file-urls` feature **and** `debug_assertions`; the feature is requested only from a `[dev-dependencies]` entry (resolver 3 does not unify those into normal builds), so no release build contains it.
- **Digest** is SHA-256 over `lullshift-pack-digest-v1\n` followed, for each file sorted by path, by `path NUL byte-length NUL sha256-hex(bytes) LF`. It covers file names and contents only, not modes or timestamps; `verify_install` also refuses links and anything that is not a plain file.
- **Third-party code** (`kind: app`, non-built-in marketplace) is refused with `PackError::ThirdPartyCodeBlocked` right after the manifest is read, before any other file is read; no review preview is produced for it (the index entry is enough to list it).
- **Error messages** never carry `git` stderr, URLs from the index or `serde` messages (which can quote values); `IndexError::Parse` keeps only the kind of problem and its line and column.
- **Review fixes (Sprint 08 security review).** The runner also sets `http.followRedirects=false` (an address must not hand the fetch to another host; a repository that moved must be re-added at its new address), `http.sslVerify=true` and `transfer.fsckObjects`/`fetch.fsckObjects=true` (Git rejects malformed or hostile trees before we read them). Inherited `GIT_SSL_NO_VERIFY`, `GIT_CURL_VERBOSE`, `GIT_CONFIG_GLOBAL`, `GIT_EXEC_PATH` and every `GIT_TRACE*` variable are removed. A fetch may add at most 200 MiB to a cache's `objects/` folder: the folder is polled while `git fetch` runs and checked once more when it ends, and past the cap the process tree is killed (`OutputTooLarge`). A cache that did not exist is fetched into a sibling staging folder and moved into place only on success, so a failed or oversized first fetch leaves nothing; an existing cache is fetched into in place (older commits that installs are pinned to must stay), and only temporary pack files are cleaned up after a failure. Re-adding a removed marketplace name from a different address is refused. Installed files are served from the bytes that were hashed. Index text uses the same invisible-character set as manifests (including U+2028/2029, the Hangul and Halfwidth filler characters and U+FFF9-FFFB) and refuses more than four combining marks in a row.
