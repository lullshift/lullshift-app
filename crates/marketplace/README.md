# marketplace

Git marketplaces for Lullshift (Sprint 08, ADR-002, ADR-015). No Tauri, no UI, no database: the desktop crate calls this crate and stores the results.

Everything that comes from a marketplace repository, its index or an activity folder is **untrusted input**. The crate never checks a repository out, never runs anything from it, and writes nothing into the install root until the whole pack has passed validation.

## Flow

```text
Git::new(app_dir)                         find git on PATH, create the empty hooks dir
Git::fetch(cache, url, ref)               shallow fetch into a bare cache repo -> pinned Commit
Git::read_index(cache, commit, builtin)   .lullshift/marketplace.json -> MarketplaceIndex (strict)
Git::resolve_entry(market, entry)         entry -> ResolvedSource (repo, commit, path); fetches external repos
Git::prepare_pack(source, request)        list, validate, read -> PreparedPack { preview, files }
install(root, pack)                       temp dir + digest check + atomic rename -> <root>/<id>/<commit>/
verify_install(dir, digest)               re-check before serving; links and stray files fail
verified_hashes(dir, digest)              the same check, keeping per-file hashes; VerifiedHashes::read
                                          re-reads and re-hashes one file on every use (decision 18.5)
diff_update(old, new_preview)             version, commit, permissions, kind and engine changes
remove_install(root, id, commit)          uninstall, or drop the old version after an update
```

## What is checked

- **URLs, refs, paths, names** (`validate`): fail-closed. The same rules are in `reference/MARKETPLACE_V1.schema.json` and both are driven by the shared fixtures in `tests/fixtures/marketplace` (`valid/` and `invalid/` for the schema, `runtime-invalid/` for rules the schema cannot know, such as the reserved name `lullshift`).
- **Git calls** (`git`): argument vector, no shell, `--end-of-options` before URLs and refs, the `-c` options and environment from ADR-015, `GIT_ALLOW_PROTOCOL=https:ssh`, a timeout that kills the process tree, output caps. Failures become a `GitFailureKind` (offline, access denied, not found, timeout, git missing, output too large, other) from the exit status and stderr. Stderr is never kept or shown.
- **Pack tree** (`pack`): only regular `100644` blobs (symlinks, submodules and the executable bit are refused), canonical ASCII paths, no case collisions, at most 512 files, 5 MiB per file and 20 MiB per pack, allowlisted file types per kind, manifest id equal to the index id, known permissions only, an engine the host provides, data valid against the engine schema.
- **Where a fetch goes** (`net`, `redirect`, Sprint 12):
  - Private hosts (IP literals, single-label names, local suffixes) need the user's confirmation, carried by `GitUrl::confirm_private`. Index entries may never be private.
  - Before each fetch the name is resolved and every address is classed. https fetches are pinned to the checked addresses with `http.curloptResolve`.
  - After a redirect-like failure, one credential-free `info/refs` probe reports a move (`Error::Moved`). Git itself never follows redirects.
  - The ADR-015 amendment lists the residual risks.
- **Plain text** (`manifest::text_ok`): the host's hidden-character set and combining-mark limit, shared with the schemas and the SDK through `tests/fixtures/plain-text/cases.json`.
- **Third-party code** (`gate`): `kind: app` from a non-built-in marketplace is reviewed like any pack and needs the user's consent to install or update (`needs_code_consent`; the desktop host enforces it, owner decision 27.5). `code_risk_status` turns the host's WebRTC state (`set_webrtc_block`) into the warning's honest verification note; it never blocks anything. The pack checks are the same for every kind.

## `file://` and tests

Production code cannot use `file://`: `Git::for_tests` and `GitUrl::for_test_file` exist only under `#[cfg(test)]`. The tests build real repositories in temp folders with `git init`, `hash-object`, `update-index` and `mktree` (`src/testutil.rs`) and cover the malicious-repository list (`src/tests/malicious.rs`); hooks, filters and option injection, each with a control that proves the trap would fire without the hardening (`src/tests/execution.rs`); and installs, refs, updates and tampering (`src/tests/happy.rs`). They need `git` on `PATH`.

```text
cargo test -p marketplace
```

## Shared code

`manifest` (activity manifest rules) and `data_schema` (engine data-schema validator) were moved here from the desktop crate, so a downloaded pack is judged by exactly the rules used for bundled ones. The desktop crate imports them.
