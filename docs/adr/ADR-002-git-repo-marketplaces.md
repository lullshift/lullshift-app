# ADR-002: Git repositories as activity marketplaces

- **Status:** Accepted
- **Date:** 2026-11-05
- **Owner:** Product owner
- **Supersedes:** Handoff v1.1 hosted catalog, upload/quarantine/moderation pipeline, CDN distribution and paid listings

## Context

Handoff v1.1 planned a hosted marketplace with accounts, server-side review and signed CDN distribution. With no hosted services in v1 (ADR-001), distribution must work without a Lullshift server. Claude Code's plugin model (add a Git repo as a marketplace, install items from it) is a proven, familiar pattern for developers.

## Decision

1. **A marketplace is any Git repository** with an index file at `.lullshift/marketplace.json` (schema: `reference/MARKETPLACE_V1.schema.json`). The index lists activities; each entry points to a folder in the same repo or to another Git repo.
2. **Public and private repos are both supported.** Any Git URL works (GitHub, GitLab, self-hosted); `owner/repo` is shorthand for GitHub.
3. **Lullshift uses the user's existing Git access** (system `git` with its credential manager, SSH keys, or `gh` auth). Lullshift never asks for, stores or transmits Git tokens. If the user can `git clone` it, Lullshift can install from it.
4. **Transport is plain `git clone`/`fetch`**, not a hosting provider's REST API (no rate limits, no API tokens, provider-neutral, works offline after clone).
5. **Installs are pinned to a commit SHA.** Updates are explicit user actions that show the version change, the source, and any new permission requests. Branch heads are never executed directly.
6. **The `lullshift-app` repository is the built-in marketplace** (`lullshift`). Its activities are bundled in the installer at the app's release tag, are always available offline, cannot be removed (only disabled) and are updated with the app (ADR-003).
7. **No central review.** Anyone can publish a marketplace. The install UI states clearly that third-party marketplaces are unverified and shows source repo, commit, kind (content vs code) and permissions before install. Only the built-in marketplace is reviewed (by pull request).
8. **Package signing (e.g. Sigstore) is deferred** past v1; commit pinning + the activity sandbox is the v1 trust baseline.

## Security consequences

- The activity sandbox (architecture doc, "Activity sandbox and host bridge") is the primary safety boundary for third-party code and must be proven on Windows, macOS and Linux before third-party *code* activities can be installed. Content-only packs (ADR-003) can be enabled earlier. *Amended by owner decision 27.5 (ADR-015 amendment, 2026-11-06): third-party code can be installed on every platform after a risk warning and explicit consent at each install and code-changing update; the warning says what is and isn't verified on the user's platform. The sandbox is unchanged.*
- Repository content is untrusted input: validate the index and manifests strictly; reject path traversal, symlinks escaping the pack folder, oversized files, Git submodules/LFS (v1), and unknown permissions.
- Git is invoked with a hardened configuration (no hooks, no `core.fsmonitor`, no custom protocols beyond https/ssh/file, no submodule recursion) into an app-owned cache directory.

## Rollback

A hosted catalog could later be added as just another marketplace source; the Git model does not need to change.
