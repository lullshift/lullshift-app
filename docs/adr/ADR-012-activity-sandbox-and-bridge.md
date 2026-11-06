# ADR-012: Activity sandbox strategy and host bridge protocol

- **Status:** Accepted (Sprint 03); amended in Sprint 12 (WebRTC turned off, S12-T13)
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
| **WebRTC** | **Gap in Sprint 03:** `RTCPeerConnection` exists and gathers host ICE candidates. Closed in Sprint 12, see below. |

**Windows 11 (26200), WebView2 runtime 156.0.4314.58, 2026-11-06 (Sprint 12): 48 of 48 checks pass with `--attention`, no known gaps.** The six new WebRTC checks are described in the next section.

**macOS (WKWebView) and Linux (WebKitGTK):** not run. No machine was available. Those runs are manual tests M03-4 and M03-5 in `docs/MANUAL_TESTS.md`. The runner uses the Chromium debug protocol, so on those OSes the probe's own results page is the check. WebKit has no out-of-process iframes, so a busy activity may slow the app there. Pause still works because the host removes the frame.

### WebRTC is off in the webview (Sprint 12, S12-T13)

CSP has no directive that governs WebRTC. The CSP3 `webrtc` directive is not shipped by Chromium or WebKit, and Permissions-Policy has no WebRTC feature in either engine. So the engine itself must stop WebRTC, for the whole webview. No page in the app needs WebRTC: the Companion and Git run in Rust, and the app page makes no network requests. The code is in `apps/desktop/src-tauri/src/webrtc.rs`.

**Why not remove `RTCPeerConnection` with script.** Any frame the activity creates is a fresh script realm with its own constructors. On Windows a nested blank frame gets its own opaque origin, so the parent can't script it. But a `srcdoc` frame can load the activity's own scripts (`script-src 'self'` is inherited, and `frame-src` doesn't apply to `srcdoc`) and run WebRTC there. The probe does exactly that, and in the negative control below it opened a data channel. A script-level removal can't reach that realm. Removing the constructors would only hide the API; it can't block the connection.

**Windows (WebView2).** WebView2 has no setting to turn WebRTC off, and Blink has no runtime flag for `RTCPeerConnection`. So the app takes away every route out, with browser arguments in `tauri.conf.json` (`additionalBrowserArgs`; a test keeps them equal to `webrtc::WINDOWS_ARGS`):

- `--webrtc-ip-handling-policy=disable_non_proxied_udp`: WebRTC may use proxied connections only. No UDP, STUN or UDP relay.
  - This is Edge's name: it maps to the `webrtc.ip_handling_policy` preference. Chromium's own `--force-webrtc-ip-handling-policy` is not in the Edge runtime (156.0.4314.58) at all, which is why the Sprint 03 attempt made no difference. It is still passed, in case a later runtime reads it.
- `--proxy-server=http://127.0.0.1:9`: the webview's only proxy is a closed local port. WebRTC's remaining TCP connections (ICE-TCP and TURN over TCP) all go through Chromium's proxy-resolving socket, so they fail. A fixed proxy list without `DIRECT` has no fallback.
- `--proxy-bypass-list=<-loopback>;http://tauri.localhost;http://ipc.localhost;http://lullact.localhost;http://localhost:1420`:
  - `<-loopback>` removes Chromium's implicit loopback bypass, so WebRTC can't reach this machine either.
  - The `http://` exceptions keep the app's own hosts (and the dev server) working. Without them the app page did not load.
  - WebRTC asks the proxy about `https://<host>:<port>` addresses only, so scheme-limited `http://` rules never match it. A test fails if any exception is a bare host, a wildcard or `https://`.
- wry's default `--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection` is repeated, because setting arguments replaces wry's defaults. Chromium keeps only the last `--disable-features`, so there must be just one.

`RTCPeerConnection` still exists on Windows. It gathers no candidates, and nothing reaches the network or this machine.

**macOS (WKWebView).** In `setup`, before anything can open an activity, the app turns off WebKit's `PeerConnectionEnabled` preference: `-[WKPreferences _setPeerConnectionEnabled:NO]` on the main webview's configuration, then reads it back.

- `PeerConnectionEnabled` gates `RTCPeerConnection`, `RTCDataChannel` and the related dictionaries (`EnabledBySetting=PeerConnectionEnabled` in WebKit's IDL).
- It is a page-level setting, so it covers every frame of the page, nested ones included. Frames created after the change get no constructor.
- `-[WKWebView configuration]` returns a copy that shares the live `WebPreferences` object (`PageConfiguration::copyDataFrom` copies a `RefPtr`), so the change reaches the running page.
- It is SPI (`WKPreferencesPrivate.h`, macOS 10.13.4+). The selectors are checked with `respondsToSelector:` first, and failure is recorded and printed with a fixed message.
- It needs a few Objective-C messages (read the configuration and preferences, check both selectors, set, read back), which are unsafe Rust. They live in their own tiny crate, `crates/macos-webrtc`, the only crate in the workspace that allows unsafe code, so `lullshift-desktop` keeps `forbid(unsafe_code)` through the workspace lints. A test fails if any other crate stops inheriting those lints, lowers `unsafe_code`, or contains an `unsafe` block or an `allow(unsafe_code)` attribute. The function body is the one checked against `objc2` 0.6.5 in Sprint 12 (it type-checks for `aarch64-apple-darwin`); it takes the view pointer that Tauri's `PlatformWebview::inner()` returns. It has not been run on macOS.

**Linux (WebKitGTK).** `WebKitSettings:enable-webrtc` is set to false on the main webview and read back. False is already its default since 2.38, and wry doesn't change it; the app's WebKitGTK binding requires 2.38 or later. It is a page setting and covers every frame.

**Evidence (Windows, this machine, 2026-11-06).** The probe now tries WebRTC from four realms:

- the activity frame;
- a nested blank frame, scripted by the parent;
- a nested `srcdoc` frame running the activity's own `rtc.js` and `nested.js`;
- a worker.

Each attempt connects two peer connections with a data channel and a STUN server. The runner also listens on a local UDP and a local TCP port. It asks the activity frame, the nested realm and the app page to reach them through STUN, TURN over TCP and remote ICE candidates. It also watches `netstat` for TCP connection attempts to the TEST-NET address 192.0.2.1, which stands in for any non-local host without needing an inbound firewall rule.

| Run | Data channel (frame / nested realm) | Local UDP packets | Local TCP connections | TCP to 192.0.2.1 |
|---|---|---|---|---|
| Before (no arguments) | opened over UDP | 44 | 2 | SYN sent |
| Only `--webrtc-ip-handling-policy` | none | 0 | 2 | SYN sent |
| Plus the closed proxy | none | 0 | 2 (loopback bypass) | none |
| Plus `<-loopback>` without `http://` exceptions | app page did not load | | | |
| **Shipped arguments** | **none; no candidates** | **0** | **0** | **none** |
| Negative control: shipped build with `--webrtc-ip-handling-policy=default --no-proxy-server` appended | opened over UDP | 44–48 | 2 | SYN sent |

The full proof with the shipped arguments passes 48 of 48 with `--attention`.

**Sources** (checked 2026-11-06):

- WebView2 environment variables and registry: arguments are appended to the app's own. <https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/webview2-idl>
- WebView2 browser flags (`--proxy-server`, `--proxy-bypass-list`, `--no-proxy-server`): <https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/webview-features-flags>
- Chromium port allocator: non-proxied UDP disabled means no UDP, STUN or UDP relay. <https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/platform/p2p/port_allocator.cc>
- Chromium P2P TCP sockets go through the proxy-resolving socket factory, with `https://` addresses. <https://chromium.googlesource.com/chromium/src/+/main/services/network/p2p/socket_tcp.cc>
- Chromium `RTCPeerConnection` IDL (no runtime flag) and the Connection-Allowlist WebRTC block: <https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/modules/peerconnection/>
- The switch names in the installed runtime were read from `msedge.dll` (156.0.4314.58).
- WebKit `PeerConnectionEnabled`: <https://github.com/WebKit/WebKit/blob/main/Source/WTF/Scripts/Preferences/UnifiedWebPreferences.yaml>, `Source/WebCore/Modules/mediastream/RTCPeerConnection.idl`, `Source/WebKit/UIProcess/API/Cocoa/WKPreferencesPrivate.h` and `WKPreferences.mm`.
- WebKitGTK `enable-webrtc` (since 2.38, default false): <https://webkitgtk.org/reference/webkitgtk/stable/property.Settings.enable-webrtc.html>

**Options considered and not taken:**

- **Chromium's Connection-Allowlist header with WebRTC blocked.** It is an experimental draft behind a feature flag. Blink only drops `addIceCandidate` and ICE servers and reports a failed state; it doesn't filter candidates inside a remote description.
- **Edge group policies** (`WebRtcIPHandling`, `WebRtcLocalhostIpHandling`). They are machine policies, not the app's to set.
- **A separate webview.** It is not needed now that WebRTC is off app-wide.

**Remaining gaps:**

- **Local overrides.** Arguments in `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` or in the WebView2 registry policy are appended after the app's. The negative control shows they can undo this (`--no-proxy-server`, or another IP policy). Setting either needs the user's own account, which already controls the app.
- **Windows depends on Edge keeping `--webrtc-ip-handling-policy`,** and on WebRTC TCP still going through the proxy. Re-run the proof on WebView2 runtime updates, as for Tauri upgrades. The proof fails if either changes.
- **macOS relies on WebKit SPI.** If it goes away, `webrtc::state()` records the failure. Third-party code stays blocked by its own constant either way.
- **Windows is asserted, not confirmed.** The arguments are applied when the webview is created and WebView2 gives no way to read them back, so `webrtc::state()` is `Asserted` there, not `Off`. The third-party code gate (ADR-015) needs both its per-OS constant and `state() == Off` (told to `marketplace::set_webrtc_confirmed_off`), so on Windows it stays closed even if the constant is flipped, until a run-time read-back exists (for example the activity probe run from the app). That is a deliberate extra lock; adding the read-back is the work needed before Windows can be unblocked.
- **macOS and Linux have not been run.** The runner needs the Chromium debug protocol. On those OSes, open the probe in a debug build and read its results page: every WebRTC line must say "blocked", with "API absent". Those are manual checks.

### Known gaps

- **WebRTC.** Closed in Sprint 12 on Windows (proof passes in full), and turned off on macOS and Linux with their proof still to run (see "WebRTC is off in the webview" above). Third-party code activities stay blocked on every OS by one constant per OS (ADR-015) until the product owner unblocks an OS whose proof has passed.
- **Tauri globals in frames** (above): mitigated today, but by behaviour we don't control.
- **Before third-party code activities (Sprint 08)**, these must also be settled. They were found in the Sprint 03 security review and are harmless while every activity is bundled and reviewed:
  - **Loading another activity.** An activity can navigate its own frame to another activity's page with the same nonce. That page would then run under the first activity's id and storage. The host should pin the frame to the launched activity's path, or serve each activity from its own host name.
  - **Storage quota.** The quota counts key and value bytes only. Many tiny keys still cost database space, and Rust has no write budget per activity. A cost per row or a cap on keys is needed, plus a write rate limit in Rust.
  - **"Nothing to save" looks like no answer.** An activity that has nothing to report is closed as unresponsive after three autosaves. The SDK's bridge client now answers with the last known state (Sprint 05). Activities that use their own client must still always return a state.
  - **Uninstalling hooks and changing notification settings** have no native confirmation (see above).

## Consequences

- Activities are static web pages that talk only through the bridge. The SDK, kits and engines (Sprints 05–07) build on protocol v1. A change that breaks the protocol needs protocol v2 and this ADR updated.
- Because a pause unloads the page, activities must restore from a snapshot. Timers, audio and animation stop on every pause. That is the point.
- The debug-only probe and the denial tests are part of the quality gate for any change to the host, the scheme handler, CSPs, capabilities, the webview's browser arguments, or Tauri or WebView2 runtime versions.
