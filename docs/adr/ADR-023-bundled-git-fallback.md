# ADR-023: Built-in Git fallback (gitoxide) for public https marketplaces

- **Status:** Accepted (owner decision 28, 2026-11-06)
- **Date:** 2026-11-06
- **Related:** ADR-002 (Git marketplaces), ADR-015 (Git invocation, cache, amendments 1-3), `sprints/DECISIONS_FOR_REVIEW.md` items 16, 17, 18, 27.3, 27.4 and 28

## Context

Marketplaces are Git repositories (ADR-002). Until now Lullshift needed `git` on `PATH`; without it every marketplace action failed with `gitMissing`. Many people who would use Lullshift, especially on Windows, don't have Git installed. The owner decided (28):

- when no `git` is found, fall back to a Git library bundled in the app, so marketplaces still work;
- the library is gitoxide (`gix`), pure Rust; libgit2 was considered and rejected;
- scope: **public https marketplaces only**. Private (sign-in) and ssh marketplaces still need installed Git, and the app says so;
- the same hardening as with installed Git: address pin, no redirects, object checks, size cap;
- installed Git is always preferred when present.

## Decision

### Backend choice

`marketplace::Git` carries a `Backend`:

- `System`: the hardened runner from ADR-015, unchanged.
- `Builtin`: `crates/marketplace/src/builtin.rs` (gitoxide), chosen by `Git::new` only when `find_git()` finds no `git` in the absolute `PATH` entries.

The choice is made once per app run (the desktop caches the `Git`). Installing Git takes effect after restarting Lullshift. Every operation the marketplace code uses has both implementations behind the same methods (`ensure_repo`, `check_ref_format`, `fetch` of a ref or a commit, reading `.lullshift/marketplace.json` and manifests by path, recursive listing, reading blobs). The built-in Git never starts a process: its `run` returns `GitMissing` if anything ever reaches the runner by mistake (tested).

### Library and features

- `gix = "=0.90.0"` (released 2026-10-22, MIT OR Apache-2.0, <https://docs.rs/gix/0.90.0>), `default-features = false`, features `blocking-network-client` and `sha1` only.
- `gix-transport = "=0.62.0"` (the version gix 0.90.0 uses), feature `http-client` only. This switches on gitoxide's backend-neutral smart-HTTP layer (the `Http` trait and `connect_http`) **without** curl or reqwest.
- The HTTP backend is ours (`builtin/http.rs`): the `ureq 3.5.1` client the redirect probe already uses (rustls, platform verifier), with the probe's `PinnedResolver`. reqwest was not needed: with our own `Http` implementation we control DNS, redirects, proxies, TLS, byte counting and timeouts directly, and no new HTTP or TLS stack enters the build. No OpenSSL.
- `blocking-network-client` compiles gix's credential and prompt crates and its filter/attributes crates (`gix-credentials`, `gix-prompt`, `gix-filter`, ...). They are linked but never reached: credentials are replaced by a function that always refuses, and nothing is checked out.

### Hardening parity

| Runner (installed Git, ADR-015) | Built-in Git | How |
|---|---|---|
| Argument vector, scrubbed environment, `GIT_CONFIG_NOSYSTEM`, no `GIT_*` overrides | No process at all. Repositories open with `gix::open::Options::isolated()`: no system, global, user or installation config, no includes, no environment variables (`GIT_*`, `HOME`, `XDG_*`, proxies) | `builtin::open` |
| `protocol.allow=never`, https and ssh only; `file://` only in tests | https only. ssh and `git@` addresses return `NeedsSystemGit` before any network or disk work. No `file://` at all (tests use a loopback server instead) | `repo::fetch_checked`, `PinnedHttp::target` |
| Resolve check (`net::check_destination`) | Same function, same rules (private hosts need the confirmation; never-allowed addresses refused) | shared code path |
| Pin: `http.curloptResolve` | The HTTP client's resolver (`redirect::PinnedResolver`) answers only for the checked host and port, with only the checked addresses; any other name fails | `builtin/http.rs` |
| Unresolved name: `Offline` without a proxy; behind a proxy, fetch unpinned | Unresolved: `Offline` without a proxy; **behind a proxy, `NeedsSystemGit`**. The built-in Git never uses a proxy (`Config::proxy(None)`, environment ignored), so it never fetches unpinned. A user behind a mandatory proxy installs Git | `repo::fetch_checked` |
| `http.followRedirects=false`; a failed fetch triggers one redirect probe | Redirects never followed (`max_redirects(0)`). A 3xx is recorded with its `Location` and turned into the same `Error::Moved` with `redirect::moved_to`, so the same-host flow (decision 18.1) is identical. No second request is needed | `builtin::classify` |
| `http.sslVerify=true`, curl's CA store | TLS always verified with the platform verifier (rustls). gix's `configure` call, which could carry `http.sslVerify=false`, proxies, extra headers or cookies from configuration, is ignored | `PinnedHttp::configure` |
| Credentials: the user's own helpers, `GIT_TERMINAL_PROMPT=0` | Never. A 401, 403 or 407 ends the fetch with `NeedsSystemGit`; gix's credential callback always refuses, so no helper, askpass or prompt runs; any `Authorization` header is dropped | `builtin.rs`, `http.rs` |
| `transfer.fsckObjects` / `fetch.fsckObjects` (strict) | gix verifies the pack checksum and every object's hash while indexing (`Mode::Verify`) and refuses objects that don't decode. Then `builtin::fsck` checks the fetched commit and every tree under it before the fetched ref moves: canonical modes only (no zero-padding), no empty, `.`, `..` or `/`-containing names, no `.git` in any spelling Git refuses (case, trailing dots/spaces, `git~1`, NTFS streams, HFS+ ignorables), no `.gitmodules` symlink, Git sort order, no duplicates, no null ids, every blob and subtree present. A failure deletes the new pack (like Git's quarantine) | `builtin/fsck.rs` |
| 200 MiB cap on objects added per fetch (folder size watched) | 200 MiB cap on bytes received per fetch (all responses counted; the pack is at most that). Over the cap the read fails and gix is interrupted; `OutputTooLarge`. gix's per-object allocation limit is set to the same 200 MiB against delta bombs | `http::Counted`, `gitoxide.objects.allocLimit` |
| 60 s network timeout, 10 s local | Same 60 s budget per fetch: every request gets the remaining time as ureq's global timeout, every read checks the deadline, and a watchdog interrupts gix's pack processing at the deadline. Local reads are in-process and have no separate timeout | `http::Budget`, watchdog in `builtin::fetch` |
| First fetch into a staging folder, renamed on success; refresh in place, partial files removed, ref moves only on success | Same code in `repo.rs`. The built-in fetch writes `refs/lullshift/incoming` first and moves `refs/lullshift/fetched` only after the object check; gix's temporary pack files are deleted on drop (and `.tmp*` files are removed after a failure) | `repo::fetch_checked`, `builtin::settle` |
| No checkout, no hooks (`core.hooksPath` empty), no LFS/filters, no submodule recursion | No checkout, no working tree, no index file; the cache is created without templates or hook samples (`builtin::ensure_repo`). gix runs no hooks, filters or LFS on fetch; submodules are never touched | `builtin.rs` |
| `git check-ref-format --allow-onelevel` after our own ref rules | `gix::validate::reference::name_partial` after the same rules | `builtin::check_ref_format` |
| `ls-tree -z -l`, `cat-file --batch` | Same output shape (`TreeEntry`, canonical modes, sizes from object headers, `ls-tree -r` order); blobs must be blobs of the listed size; the same output cap turns a huge tree into "too many files" | `builtin::ls_paths`, `ls_tree_recursive`, `read_blobs` |

### Cache format

The cache stays a normal bare repository (`HEAD`, `config`, `objects/`, `refs/`, `shallow`), the same layout `git init --bare --template=` makes. Either backend reads and refreshes a cache the other wrote (tested both ways, including `git fsck --full` on a cache the built-in Git wrote), so installing or removing Git later keeps working.

### UI

- The marketplace panel shows one plain-text status line: "Using Git installed on this computer." or "Using Lullshift's built-in Git (public https only). To add private or ssh marketplaces, install Git, then restart Lullshift." It comes from the new `marketplace_git_backend` command.
- New error kind `needsGit` (not `gitMissing`): "Install Git to use this marketplace. ..." No link is shown.

### Tests

- The whole marketplace fixture suite (156 tests) runs on both backends: `tests::builtin::whole_suite_on_the_builtin_git` re-runs the test binary with `LULLSHIFT_TEST_GIT_BACKEND=builtin`. In that mode every repository is served by `testserver` (`git http-backend` behind plain http on 127.0.0.1, test-only switch) at a host under `.invalid` that only the pinned resolver can reach, so every built-in fetch in the suite also proves the pin.
- Built-in-only tests: no `git` on `PATH` selects it; ssh, 401 and 403 give `needsGit` with one request and no `Authorization`; redirects become `moved` (same host and other host) and are not followed; a closed pinned address means no request reaches the real server; unresolved with and without a proxy; the production built-in Git can't talk plain http; the deadline; fetch by commit id; cache interop both ways; refs.
- A handful of runner-specific tests (the curl pin, `GIT_TRACE` scrubbing, process kill) only apply to installed Git and are skipped in built-in mode, with the built-in equivalent named in each.

## Alternatives considered

- **libgit2 (`git2`)**: rejected by the owner. C code with its own HTTP and TLS stacks (OpenSSL on Linux), a long CVE history, harder to pin addresses and cap sizes.
- **gix with its reqwest backend**: works, but adds reqwest, hyper and a second TLS setup, and gix's reqwest backend builds its own client, so pinning DNS would need patching. Our own `Http` implementation over the ureq we already ship was simpler and tighter.
- **Bundling a `git` executable**: large, per-platform, and its own update and signing burden.
- **Keep requiring Git**: rejected by the owner.

## Dependency audit (2026-11-06)

- `cargo tree -p marketplace -e normal`: 104 distinct crates before, 193 after (+89, mostly the `gix-*` family). The workspace lockfile gains 73 packages (the rest were already used elsewhere).
- Licences of the new crates: all permissive and compatible with Apache-2.0: 67 MIT OR Apache-2.0 (in its various spellings), 6 Unlicense OR MIT, 5 MIT, 2 Unlicense/MIT, `dunce` CC0-1.0 OR MIT-0 OR Apache-2.0, `tinyvec` Zlib OR Apache-2.0 OR MIT, `zlib-rs` Zlib, `unicode-bom` Apache-2.0, `encoding_rs` (Apache-2.0 OR MIT) AND BSD-3-Clause, `unicode-normalization` (MIT OR Apache-2.0) AND Unicode-3.0.
- Advisories: `cargo deny` and `cargo audit` are not installed here; the 73 new lockfile packages were checked against the OSV database (`api.osv.dev/v1/querybatch`, crates.io ecosystem): 0 advisories. Older gix releases had advisories (path handling in checkout, SHA-1 collision detection); both are in code paths this crate either doesn't use (checkout) or has fixed in the pinned versions (`sha1dc` collision detection is in the tree).
- Binary size: not measured for this change (two full LTO release builds of the desktop app were not run). To be measured before release by comparing `cargo build --release -p lullshift-desktop` with and without this change. gix's default features are off (no status, blame, merge, worktree mutation or HTTP backends).

## Consequences

- Marketplaces work on a computer without Git, for public https repositories.
- Private and ssh marketplaces, and networks that require a proxy, still need installed Git; the app says so with `needsGit`.
- More third-party Rust code in the app (gix family). It is pure Rust and pinned exactly; every bump reruns the suite on both backends.

## Residual risks and gaps

- **Object checks.** Not covered compared with `git fsck --strict`: commit and tag header details (dates, e-mail syntax, extra headers), `.gitmodules` contents, symlink target checks. Lullshift's own pack validation still refuses symlinks, submodules, executables and `.git` paths in anything it installs, and commit headers are never used.
- **401 for missing repositories.** Some hosts (GitHub) answer 401 for a repository that doesn't exist, so a mistyped public address shows `needsGit` rather than `notFound` on the built-in Git.
- **Proxy-only networks.** The built-in Git refuses rather than fetching unpinned; those users need Git.
- **Size cap measure.** Bytes received rather than folder growth; the received stream (pack plus framing) is never smaller than the pack written, so the cap is at least as strict.
- **Restart after installing Git.** The backend is chosen once per run.
