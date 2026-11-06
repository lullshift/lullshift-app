# ADR-022: OpenCode adapter (experimental, OpenCode 2.x only)

- **Status:** Accepted-experimental (Sprint 12, S12-T01). Amended 2026-11-06 by the owner's decision to build it for OpenCode v2 only, marked experimental. The original status was "Accepted: adapter deferred"; the first version of this ADR recorded why (below, "History").
- **Date:** 2026-11-06
- **Related:** ADR-010 (hook forwarder and installation), ADR-013 (opt-in task text), ADR-021 (Codex and Cursor), owner decisions 1, 22 and 23

## Context

The owner asked for an observe-only OpenCode adapter like the Codex and Cursor ones. The first research round (kept under "History") found no documented event payloads and two incompatible plugin APIs, and deferred the adapter. The owner then decided: build it for OpenCode v2 only, mark it experimental, use the plugin fields as they appear in OpenCode's source, and show the status as unknown until an event has actually arrived. This ADR records that research and the design.

The rule that still holds: observe-only, user-level, explicit consent, exact removal. What is relaxed is "only a documented interface": the plugin event types below are source-confirmed, not documented, which is why the adapter is labelled experimental.

## Research (checked 2026-11-06)

Sources, read in full for the parts below:

- Docs: <https://opencode.ai/v2/docs/build/plugins> and <https://opencode.ai/v2/docs/build/plugins/migrate-v1>.
- Source: <https://github.com/anomalyco/opencode>, tag `v2.2.5` (latest v2 tag at the time), commit `9bbb8ce18c80f099e0e448775f59d965ead40a21`. Files: `packages/plugin/src/README.md`, `src/promise/{plugin,event,adapter}.ts`, `src/host.ts`, `src/runtime.ts`, `src/source*.ts`; `packages/client/src/promise/generated/types.ts` (event types); `packages/core/src/{config/discovery.ts,config/plugin/source.ts,config.ts,plugin.ts,plugin/{module,source-directory,service}.ts}`; `packages/util/src/{global,global-roots}.ts`.
- v1 loader, for the cross-version risk: tag `v1.19.5`, `packages/opencode/src/plugin/index.ts`.

### Where a global plugin can live

- The v2 docs describe only project folders (`.opencode/plugin/`, `.opencode/plugins/`) and the config key `plugins` (absolute path, directory entries only: "a configured file path is skipped with a warning").
- The source shows a **global** location. `ConfigDiscovery` puts the global config directory in the config entries as a `Directory` (`config.ts` `loadDirectory(sources.global)`), and `PluginSourceDirectory.discover` scans `plugin/` and `plugins/` under every directory entry for `*.js`, `*.ts` files (and directories, and symlinks to either). The global directory is `$OPENCODE_CONFIG_DIR`, else `$XDG_CONFIG_HOME/opencode`, else `<homedir>/.config/opencode`, on every platform including Windows (`util/src/global.ts`, `global-roots.ts`).
- A standalone file is accepted as a server entry (`PluginModule`: "legacy auto-discovery still admits standalone server sources"). So **no config entry is needed**: one file in `<global config dir>/plugins/` is enough, and no OpenCode config file is touched.
- This is source-confirmed only, not documented. It may change.

### Plugin shape

- Module default export `{ id: string, setup(ctx) }` (`PluginModule` schema). `Plugin.define` just returns its argument, so a file can export a plain object and import nothing from `@opencode/plugin`. Plugins run in-process on Bun or Node (`source.bun.ts`, `source.node.ts`); `node:child_process` is available on both.
- `ctx.event.subscribe({ types?, signal? })` returns an `AsyncIterable` of events `{ id, created, type, data, ... }` (not a callback). The migrate guide: "Abort it from the cleanup function returned by `setup`", and the plugin must clean up its own timers and subprocesses.
- **Setup is awaited and sessions wait for it.** `Plugin.activate` runs each plugin's `setup`; session entry points `awaitActivation` before reading plugin state. So `setup` must return at once and must not await the event loop.
- **Errors do not reach OpenCode.** A setup that throws or rejects is caught, logged as "failed to load plugin", and that plugin is simply not activated. The same applies to a module that fails to import. There is no documented or source-visible timeout for `setup`, which is why ours does no work before returning. An error thrown while iterating the stream is the plugin's own (inside its async loop).
- Hooks (`ctx.session.hook`, `ctx.permission.hook`, `ctx.tool.hook`...) can change behaviour (for example the `experimental.idle` hook can admit new input, and "name and shape may still change"). The plugin registers none.

### Event types used (all `data.sessionID: string`)

| OpenCode 2.x event | Extra fields used | Lullshift event |
|---|---|---|
| `session.created` | `parentID?` (set for a helper session: ignored) | session_started |
| `session.execution.started`, `session.tool.called`, `session.text.started` | none | working (at most one per second per session) |
| `session.status` | `status.type == "busy"` | working |
| `session.inbox.enqueued` | `item.type == "user"`; `item.payload.text` only with task-text consent | working (and the intent, below) |
| `permission.asked` | none (resources, action, message are never read) | needs_attention |
| `permission.replied` | `reply: once/always/reject` | working |
| `session.idle`, `session.execution.succeeded` | none | turn_ended (once per turn) |
| `session.execution.interrupted` | `reason: user/shutdown/superseded/inactivity` | turn_ended |
| `session.execution.failed` | none (the error is never read) | failed (the idle that follows does not replace it) |
| `session.deleted` | none | session_ended |

There is no `session.error` event in v2 (it was v1); failure is `session.execution.failed`. `permission.asked` is a real attention signal. Nothing reports "waiting for a question or idle prompt", so attention is partial, like Codex.

### Task text

The user's prompt is `data.item.payload.text` of `session.inbox.enqueued` where `item.type == "user"` (`SessionInboxItem`, `SessionInboxUserPayload`; confirmed in the generated types, not in the prose docs). It is therefore supported under the ADR-013 opt-in: the plugin reads `context-capture/opencode` in the app data directory (the same marker the app writes when the user turns Task context on for OpenCode), includes the text (cut to 4,000 characters) only if the marker exists, and the forwarder checks the marker again before reading it. Tool inputs, paths, replies and file attachments are never read.

### Cross-version risk (v1)

OpenCode 1.x scans the same `~/.config/opencode/plugins/` folder with a different API. v1 `applyPlugin` accepts a function export or an object with a `server` function; a default export `{ id, setup }` fails with "Plugin export is not a function", which v1 logs and also publishes as a session error event (a user-visible plugin error), then continues. So on 1.x our file does nothing but may show an error. Lullshift cannot tell which version the user runs, so the UI and docs say so. The file is deliberately not made dual-API (an extra `server` export would couple it to v1's loader, which we can't test).

## Decision

Build an experimental OpenCode adapter for OpenCode 2.x:

1. **One owned file.** `lullshift-observer.js` in `<global config dir>/plugins/`, written by Lullshift, recognised by its first line (`// Lullshift-owned file: lullshift-observer, plugin version N`). Install is refused if a file with that name exists without the header, or is a symlink, read-only or not a regular file. Writes are atomic and abort if the file changed meanwhile; installing twice changes nothing; repair rewrites an older version; uninstall deletes the file only if it carries the header. No OpenCode config file is read or written. The record kept in SQLite is the same `InstallRecord` as for the hook files.
2. **Observe-only and defensive plugin.** Exports only `{ id, setup }`; `setup` is synchronous, subscribes, starts the loop un-awaited and returns a cleanup that aborts the stream; no hooks; everything in try/catch; the child is spawned with an argument array (`spawn(exe, ["hook", "opencode", "--data-dir", dir])`, no shell), stdio `["pipe", "ignore", "ignore"]`, `detached`, `windowsHide`, `unref`ed, never awaited, at most 8 at once; "working" coalesced to one per second per session; helper sessions ignored; maps and sets bounded. The message is built field by field: `{ v, event, sessionId, hint?, prompt? }`.
3. **Safe embedding.** The forwarder path and data directory are embedded as ASCII-only JavaScript string literals (`\uXXXX` for everything outside printable ASCII, `"` and `\` escaped), refused if they contain control characters. A test evaluates the literals in node and compares them with the original paths, including quotes, backslashes, `$`, backticks and non-BMP characters. AppImage is handled by `hook_exe.rs` as for the other adapters.
4. **Forwarder.** `hook opencode` reads that small message (not OpenCode's own event), maps it in Rust (`opencode/map.rs`), builds and validates the event like the others, and reads `prompt` only with the consent marker. No change to the protocol or its generated types: the provider is a free-form string.
5. **Status is honest.** File missing: not connected. File equals what Lullshift would write now: **installed**, shown as **Unknown** until the app's event log has an OpenCode event received after the file's modification time (then **Connected**). File ours but different (moved app, older plugin, edited): needs repair. A file with the name that isn't ours: cannot connect. The check polls only while the card is open and waiting. A later silence is not detected.
6. **Not part of "all connected".** The welcome's "everything is connected" check stays on the three supported agents (`SUPPORTED_AGENTS`); OpenCode is in a separate `EXPERIMENTAL_AGENTS` list. Reasons: an experimental, version-dependent integration must not keep the Connect button on forever for someone who doesn't use OpenCode, nor count as "done" when its status is only a guess.
7. **Uninstall.** `--uninstall-hooks` (Windows NSIS and Linux `prerm`) also deletes the plugin file when it carries the header, with or without a database record; the Linux `prerm` also unsets `XDG_CONFIG_HOME` and `OPENCODE_CONFIG_DIR` so each user's own location is used.

## Consequences

- Integrations lists OpenCode with an Experimental label and a note that it works only with OpenCode 2.x. The capability matrix has an OpenCode column with the gaps above (attention partial, no session end for sessions closed without deleting, no title, helper sessions not shown).
- A future OpenCode 2.x release can change event names or the discovery rule without notice. The plugin then silently does nothing, and the card stays "Connected" from the earlier event. Re-check this ADR when OpenCode 2.x changes its plugin API; replace the source-confirmed items with documented ones when they exist.
- OpenCode 1.x users who connect see no data and possibly a plugin error in OpenCode until they disconnect.
- Manual verification against a real OpenCode 2.x is still needed (see the sprint report): everything here was verified against source and a fake context, not a running OpenCode.

## History

First version of this ADR (2026-11-06, research only, adapter deferred): OpenCode had two incompatible plugin APIs; the v1 docs (<https://opencode.ai/docs/plugins/>) listed event names without payload fields; the v2 guide named no session-start, error or permission event and no payload fields; whether a failing or slow plugin could block OpenCode was undocumented; so nothing was built. It listed two unblocking conditions (documented payloads plus a stated no-block guarantee, or a declarative hook setting). The owner overrode that on 2026-11-06 for v2 only, accepting source-confirmed fields and the experimental label.
