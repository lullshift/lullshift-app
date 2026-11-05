# ADR-012: Activity sandbox strategy and host bridge protocol

- **Status:** Accepted (Sprint 03)
- **Date:** 2026-11-05
- **Owner:** Implementation lead
- **Related:** ADR-003 (activity model), ADR-010 (hook installation), `reference/ACTIVITY_MANIFEST_V1.schema.json`, `tests/sandbox/`

## Context

Activities are untrusted code (CLAUDE.md, ADR-003). Most will come from Git marketplaces, and many will be AI-generated. They run next to the app that installs agent hooks and stores session history. Two things must hold:

- An activity can't reach the app, the network, the filesystem, other activities' data or the clipboard.
- The host, not the activity, decides when an activity stops. When an agent needs the user, the activity is paused and its progress kept, even if it is broken, busy or hostile (S03-A01).

Tauri 2.12 (wry 0.57, WebView2 on Windows) was checked against its source: custom URI schemes, how origins are classified for IPC, and the invoke key.

## Decision

### Where activities run

- **An origin apart from the app.** Activity files are served by a custom scheme, `lullact`. The URL is `http://lullact.localhost/<activity id>/<file>` on Windows and `lullact://localhost/<activity id>/<file>` elsewhere. The app is never served from that origin, and activities are never served from the app's origin.
  - All activities share this one origin and are told apart only by path. What keeps them apart is the opaque origin each one runs with, set twice: by the iframe and by the activity's own CSP header.
- **A sandboxed iframe in the main window:** `sandbox="allow-scripts"`, without `allow-same-origin`, so the document's security origin is opaque (`"null"`). It has no cookies, Web Storage or IndexedDB, and can't navigate the top window, open popups or submit forms. The `allow` attribute denies camera, microphone, geolocation, clipboard, screen capture, fullscreen, payment, USB, serial, HID and Bluetooth. The referrer policy is `no-referrer`.
- **The app CSP allows frames from the activity scheme only:** `frame-src lullact: http://lullact.localhost`, never `'self'` (S03-T06).
- **Every activity response carries a strict CSP:** `sandbox allow-scripts` (so the page has an opaque origin even if it is ever loaded without the iframe's attribute), `default-src 'none'`, scripts/styles/images/fonts/media only from `'self'`, `connect-src 'none'`, `frame-src`/`child-src`/`worker-src`/`object-src`/`manifest-src 'none'`, `base-uri 'none'`, `form-action 'none'`, and `frame-ancestors` limited to the app origin.
  - Further headers: `nosniff`, `no-referrer`, `no-store`, a deny-all `Permissions-Policy` and `Cross-Origin-Opener-Policy`.
  - No inline script and no `eval`. Classic scripts only: module scripts and web fonts would need CORS, which an opaque origin can't satisfy.
- **The scheme handler serves only listed activities.** An activity is listed only if its manifest:
  - parses and is of kind `app`, with runtime `isolated-web-v1` and entry point `index.html`;
  - declares all three interruptibility capabilities;
  - asks for no permission other than `activity.storage`.
  The handler also refuses path traversal and serves only an allowlist of file types. In v1 the files are the bundled ones compiled into the binary. Marketplace installs (Sprint 08) must go through the same checks.
- **Why not a separate webview window?** An iframe keeps one window and one IPC surface. Chromium also puts the opaque-origin frame in its own process (out-of-process iframe), so a busy activity doesn't freeze the app: the app answered in 1–10 ms during a 3-second busy loop. A separate webview stays the fallback if a later Tauri version lets frames reach IPC (see "Tauri globals" below).

### Tauri IPC is not reachable from activities

- Capabilities grant commands to the `main` window. No capability lists the activity scheme or any remote URL. A test fails if any capability exposes dialogs to scripts. Another test fails if a window other than `main` gets the agent-settings commands (S03-T07).
- **Tauri globals.** Tauri injects its initialization scripts into every frame, so `__TAURI_INTERNALS__`, `ipc` and `chrome.webview` exist inside the activity.
  - The denial tests call commands through them: `dashboard`, `notification_prefs`, `activity_clear_snapshot` for another activity, and `plugin:event|emit` of `agent-attention`.
  - None of these calls is ever answered. A canary save for another activity is untouched, and a forged attention event has no effect.
  - The direct `fetch` route to `ipc.localhost` is blocked by the activity's `connect-src 'none'`.
  - The main window itself isn't granted `emit`, so no script in it, or in a frame inside it, can fake an agent event.
  - **This depends on Tauri internals.** Re-run the denial tests on every Tauri or wry upgrade. If any of these checks fails, move activities to a separate webview with no capabilities before shipping.
- Installing agent hooks also needs a native confirmation dialog from Rust that names the settings file (S03-T07). Even a script that reached IPC couldn't install hooks without the user.
  - It could still do other harm: uninstall the hooks, turn off "needs you" notifications, or read the dashboard. Those calls have no native confirmation. That is why the Tauri-globals check is a release blocker, not a nice-to-have.

### Bridge protocol v1

All traffic is `postMessage` of plain objects `{ lullshift: 1, nonce, type, ... }`.

- **Activity to host:**
  - `ready`
  - `state` (`requestId` or null, any JSON value except null)
  - `storage` (`get`, `set`, `remove` with a key matching `[A-Za-z0-9._-]{1,64}`, and a value of at most 64 KiB)
- **Host to activity:**
  - `init` (`restore`, `reducedMotion`, `theme`)
  - `pause`
  - `resume`
  - `save` (`requestId`)
  - `storage-result`
- **Launch nonce.** Each launch gets a fresh 128-bit nonce, passed in the URL fragment (`#n=`). The host accepts a message only if:
  - its source is the current iframe's window;
  - its origin is `"null"`;
  - its nonce is current.
  Messages are capped at 320 KB of JSON. A token bucket allows 30 messages per second, with bursts up to 120. Anything else is dropped and counted, never queued.
- **Storage** is namespaced by activity id and needs the `activity.storage` permission. Limits: 64 KiB per value and 256 KiB per activity, checked in one SQLite transaction in Rust. The webview's own limits are a convenience, not the enforcement.
- **Snapshots** are JSON, at most 256 KiB, one per activity, stored in SQLite (migration 0003).
  - A null or missing state never replaces a snapshot, so a broken activity can't wipe good progress. Both the host and Rust enforce this.
  - The host ignores state sent before `init`, which would otherwise be a blank start state.
  - Saves are written one at a time, in the order they arrived. A new launch reads the snapshot only after pending saves finish.
- **An oversized message** from the current launch closes the activity as unresponsive. Its later messages are never decoded.
- The canonical client is `packages/activity-sdk/bridge/lullshift-bridge-v1.js` (it moved from `activities/lib/` in Sprint 05, ADR-017). Each bundled activity carries an identical copy, and a test keeps the copies identical.

### Additions in Sprint 05 (still protocol v1)

These are optional additions; an older host drops the new message as an unknown type, and an older activity simply never sends it.

- **`summary` (activity to host):** `{ type: "summary", items: [{ label, value }] }`.
  - One to four items. `label` is plain text of 1–24 characters, and `value` is plain text of 1–24 characters or a finite number.
  - The host shows the latest summary in the paused view, as text. It is held in memory for the current visit and never stored, so an activity can't use it as a hidden store.
  - The host keeps only the latest summary. Summaries count against the normal message rate limit.
- **`init.content`:** the validated pack data for engines (ADR-016), `null` for other activities. The UI fetches it with `activity_content(id)`, which re-validates the data in Rust against the engine's schema; a pack that fails never starts.
- **Help and preview come from the manifest, not the bridge.** The host renders `help` as plain text in its own panel; nothing in it is HTML. The host reads `preview` itself and shows it only as an `<img>` from a `data:` URL, so the app's CSP is unchanged and an SVG preview never runs script.
- **Time played** is measured by the host: the time an activity is running in a visible window. The host adds it to a per-activity total in SQLite (migration 0005) on every pause and close. The activity can't report or change it.
- **The bridge client always answers `save`** with the last known state if `saveState` returns nothing (ADR-017), which closes the "nothing to save looks like no answer" gap for activities using the SDK.

### Host-owned interruption

- **Pausing means save, then unload.** The host asks for state and waits at most 800 ms. Then it removes the iframe whether or not the activity answered. Nothing the activity does can delay or block the pause.
- **Autosave.** The host asks for state every 15 seconds, and activities may push state at any time. After three unanswered requests in a row, the activity is marked unresponsive and paused.
- **What triggers a pause:**
  - an `agent-attention` event, emitted by Rust on every change into `needs_attention`, whatever the notification settings are;
  - the user pressing Pause;
  - the window being hidden;
  - the user leaving the Activities view.
- **Each pause request pauses once.** Resuming after an attention pause ("Keep playing") doesn't replay it.
- **After a pause,** keyboard focus moves to the paused card if it would otherwise fall to the page body.
- **Resuming** loads a fresh launch with a new nonce and the last snapshot. Crash recovery is the same path. Because only snapshots survive, a crash, a reload and a pause all restore to the last save.

### Denial tests

`tests/sandbox/run-denial-tests.mjs` runs against a debug build over the WebView2 debug port. The sandbox probe (`apps/desktop/src-tauri/sandbox-probe/`) is compiled into debug builds only. It attempts each escape from inside the sandbox. The runner then checks what the probe can't see from inside:

- the app stays responsive while the activity is busy;
- pause and restore work;
- a stubborn activity is still paused and its last good save is kept;
- the activity can't navigate remotely;
- other activities' saves are untouched.

**Windows 11 (26200), WebView2, 2026-11-05: 42 of 42 checks pass with `--attention`, with one known gap.**

| Area | Result |
|---|---|
| Parent document, parent globals, top navigation, popups | Denied |
| `eval`, `Function`, inline script, workers | Denied by CSP |
| Cookies, `localStorage`, `sessionStorage`, IndexedDB | Denied (opaque origin) |
| `fetch`, XHR, WebSocket, EventSource, `sendBeacon`, remote images, nested remote frames | Denied by CSP |
| Self-navigation to a remote page | Denied: the app's `frame-src` turns it into an error page |
| Tauri commands and event emit via the injected globals | Never answered; no effect |
| Clipboard, notifications | Denied |
| Forged messages with a wrong nonce; storage over quota | Dropped / refused |
| Busy loop, stubborn pause, restore | App responsive (1–10 ms); paused in under 1 s; last good save kept |
| **WebRTC** | **Gap:** `RTCPeerConnection` exists and gathers host ICE candidates |

**macOS (WKWebView) and Linux (WebKitGTK):** not run. No machine was available. Those runs are manual tests M03-4 and M03-5 in `docs/MANUAL_TESTS.md`. The runner uses the Chromium debug protocol, so on those OSes the probe's own results page is the check. WebKit has no out-of-process iframes, so a busy activity may slow the app there. Pause still works because the host removes the frame.

### Known gaps

- **WebRTC.** CSP does not govern WebRTC. An activity can create a peer connection, learn local ICE candidates and, if UDP or a TURN server is reachable, send data out. Chromium's `--force-webrtc-ip-handling-policy=disable_non_proxied_udp` was tried through WebView2 browser arguments and made no measurable difference, so it was not kept.
  - **Consequence:** until this is closed, only first-party, reviewed activities ship. Bundled activities are reviewed for any use of `RTCPeerConnection`.
  - Installing third-party code activities from marketplaces (Sprint 08) is **blocked** on closing this gap, or on reviewing each such activity.
  - **Options:** a separate webview with WebRTC disabled where the platform allows it; a stronger WebView2 policy or flag; or a pre-install static check backed by a runtime check.
- **Tauri globals in frames** (above): mitigated today, but by behaviour we don't control.
- **Before third-party code activities (Sprint 08)**, these must also be settled. They were found in the Sprint 03 security review and are harmless while every activity is bundled and reviewed:
  - **Loading another activity.** An activity can navigate its own frame to another activity's page with the same nonce. That page would then run under the first activity's id and storage. The host should pin the frame to the launched activity's path, or serve each activity from its own host name.
  - **Storage quota.** The quota counts key and value bytes only. Many tiny keys still cost database space, and Rust has no write budget per activity. A cost per row or a cap on keys is needed, plus a write rate limit in Rust.
  - **"Nothing to save" looks like no answer.** An activity that has nothing to report is closed as unresponsive after three autosaves. The SDK's bridge client now answers with the last known state (Sprint 05). Activities that use their own client must still always return a state.
  - **Uninstalling hooks and changing notification settings** have no native confirmation (see above).

## Consequences

- Activities are static web pages that talk only through the bridge. The SDK, kits and engines (Sprints 05–07) build on protocol v1. A change that breaks the protocol needs protocol v2 and this ADR updated.
- Because a pause unloads the page, activities must restore from a snapshot. Timers, audio and animation stop on every pause. That is the point.
- The debug-only probe and the denial tests are part of the quality gate for any change to the host, the scheme handler, CSPs, capabilities or Tauri versions.
