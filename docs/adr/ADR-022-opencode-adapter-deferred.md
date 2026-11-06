# ADR-022: OpenCode adapter deferred

- **Status:** Accepted (Sprint 12, S12-T01 research). Revisit when the conditions below hold.
- **Date:** 2026-11-06
- **Related:** ADR-010 (hook forwarder and installation), ADR-021 (Codex and Cursor), owner decisions 1 and 22

## Context

The owner asked for an observe-only OpenCode adapter like the Codex and Cursor ones (decisions 1 and 22). Sprint 12 first researches OpenCode's current official plugin API. The rule for every adapter is: only a documented, stable interface, installed at user level with consent, and no guessing.

## Findings (checked 2026-11-06)

- OpenCode has no hook file. Observing a session means installing a **plugin**: a JS/TS module in a plugin directory, or a package named in `opencode.json`.
- There are now **two incompatible plugin APIs**.
  - v1 (docs at <https://opencode.ai/docs/plugins/>): files in `~/.config/opencode/plugins/` (global) or `.opencode/plugins/`; `export const X = async ({ project, client, $, directory, worktree }) => ({ event: async ({ event }) => ... })`. The page lists event names (`session.created`, `session.idle`, `session.error`, `session.status`, `permission.asked`, `permission.replied`, `tool.execute.after`, `message.updated`...) but **documents no payload field for any event**. It does not say whether a plugin that throws or stalls can block OpenCode, or how load errors surface. The only blocking example is a `tool.execute.before` hook that throws.
  - v2 (OpenCode 2.x, current release 2.2.5 on 2026-10-30; guides at <https://opencode.ai/v2/docs/build/plugins> and <https://opencode.ai/v2/docs/build/plugins/migrate-v1>): plugins are `Plugin.define({ id, setup(ctx) })` from `@opencode/plugin`, discovered in `.opencode/plugins/` and `.opencode/plugin/`, config key `plugins` (was `plugin`). "V1 plugin implementations do not run in V2." Events are reached through `ctx.event.subscribe()` (events with a `type`); the guide names no session-start, error or permission event and no payload fields. The `session.hook("experimental.idle")` hook is marked in the source README as "name and shape may change". A package may export both implementations, but the two APIs are separate and the hooks are not translated.
- The source (<https://github.com/anomalyco/opencode>, branch `dev`, tag `v2.2.5`) is mid-rewrite (`packages/plugin/src/promise` and `effect` APIs). No stable, documented event payload contract was found there either.

## Decision

Lullshift does **not** ship an OpenCode adapter yet. Reasons:

1. The only way to observe a session is code that runs inside OpenCode. Which of two APIs a user's OpenCode loads depends on its major version, and the v1 file would silently not run on v2. Lullshift cannot tell which it installed into, so status (installed / needs repair) could not be honest.
2. No payload fields are documented, so the session id, and whether an event means "needs attention", would be guessed from source on a moving branch.
3. Whether a failing plugin affects OpenCode's startup or a running turn is undocumented, so the observe-only guarantee (a hook that cannot block or alter the agent) cannot be shown.

Nothing was installed, written or merged into any OpenCode directory during this research, and no OpenCode code, enum value or UI row was added.

## What would unblock it

Any one of these, followed by a new ADR and the usual consent, backup, idempotent install, Disconnect and security-review work:

- OpenCode documents the plugin event payloads (at least session id on `session.created`, `session.idle`, `session.error`, `permission.asked`) and states that a plugin error or slow handler cannot block the agent; **and** the v2 plugin API leaves beta or the docs name a supported way for one file to work on both; or
- OpenCode offers a declarative hook or notification setting (a command run on session events, like Codex hooks or Cursor `hooks.json`) with documented stdin.

If built, the design is already settled by ADR-021: user-level only, one small plugin file Lullshift fully owns, spawning `lullshift-desktop hook opencode --data-dir <dir>` in exec form (no shell) fire-and-forget, reading ids and enum hints only, with prompt capture under the same opt-in as ADR-013.

## Consequences

- The Integrations screen and the capability matrix list Claude Code, Codex and Cursor only. OpenCode users get no session tracking; the app doesn't pretend otherwise.
- The owner's decisions 1 and 22 are not met by this sprint; the report to the owner says why.
