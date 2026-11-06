# ADR-018: App update signing and update checks

- **Status:** Proposed. Becomes Accepted when the owner creates the signing key and the updater is enabled.
- **Date:** 2026-11-06
- **Related:** ADR-001 (no Lullshift-hosted services, no default telemetry), ADR-004 (manual release workflow), `docs/RELEASING.md` section 3, decision 26.3 in `sprints/DECISIONS_FOR_REVIEW.md`

## Context

Tauri's updater checks every update against a minisign public key. No key exists yet, so the updater is not registered and the app makes no update requests (S11-T01). The PRD allows app update checks only as network use the user starts. The owner decided on 2026-11-06 (decision 26.3) that, besides a manual button, users may opt in to a daily check.

## Decision

- **Key custody.** The owner generates the key pair on their own machine, outside the repository, and keeps the private key and its password in a password manager. In CI the key exists only as a secret of a GitHub `release` environment, limited to `main`, with the owner as required reviewer. The signing secret is passed only to the **Build installers** step. The private key never goes in the repository, `.env` files, logs or artifacts.
- **Update source.** `https://github.com/lullshift/lullshift-app/releases/latest/download/latest.json` and the installer files of the same release. These are GitHub's own servers, not a Lullshift service.
- **Manual check.** Settings has a **Check for updates** button. It shows the version and where the update comes from, and waits for the user to confirm before installing.
- **Opt-in daily check.** A setting, **Check for updates once a day**, is off by default and stored with the other local preferences. When the user turns it on, that choice is the user starting the network use, so the PRD rule holds. When it is on, the app checks at most once every 24 hours, only while running, and never at first launch before the user has seen the setting. A found update is only announced. Installing still needs the user's confirmation. Turning the setting off stops all scheduled checks immediately. No identifiers, usage data or system details are sent beyond what the GET request itself carries.
- **No configuration-driven background checks.** The Tauri configuration never contains a check-on-start or background setting. The daily schedule lives in app code behind the opt-in. `tests/release_config.rs` keeps enforcing this.
- Nothing in this ADR is implemented until the key exists. Then: add the plugin, the button, the opt-in setting with its tests, the endpoint in `docs/user/privacy.md`, and manual check M11-7.

## Consequences

- With the setting off, which is the default, Lullshift still makes no network request the user didn't start.
- A lost private key means installed copies can't be updated, and users must reinstall from the Releases page.
- The privacy page and the network-capture check (M11-4) must list the update endpoint once the updater ships.
