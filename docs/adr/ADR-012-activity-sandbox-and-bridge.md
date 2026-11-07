# ADR-012: Activity sandbox strategy and host bridge protocol

- **Status:** Accepted (Sprint 03); amended in Sprint 12 (WebRTC turned off, S12-T13) and after the security review of owner decision 27.5 (launch keys, serving budgets, storage rows, native IPC research)
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

- **An origin apart from the app.** Activity files are served by a custom scheme, `lullact`. Since the security review of decision 27.5 the URL carries a per-launch key: `http://lullact.localhost/<launch key>/<activity id>/<file>` on Windows and `lullact://localhost/<launch key>/<activity id>/<file>` elsewhere (see "Launch keys" below). The app is never served from that origin, and activities are never served from the app's origin.
  - All activities share this one origin. What keeps their data apart is the opaque origin each one runs with, set twice: by the iframe and by the activity's own CSP header. What keeps their *files* apart is the launch key: a request is served only if its key is live and was issued for the id in its path.
- **A sandboxed iframe in the main window:** `sandbox="allow-scripts"`, without `allow-same-origin`, so the document's security origin is opaque (`"null"`). It has no cookies, Web Storage or IndexedDB, and can't navigate the top window, open popups or submit forms. The `allow` attribute denies camera, microphone, geolocation, clipboard, screen capture, fullscreen, payment, USB, serial, HID and Bluetooth. The referrer policy is `no-referrer`.
- **The app CSP allows frames from the activity scheme only:** `frame-src lullact: http://lullact.localhost`, never `'self'` (S03-T06).
- **Every activity response carries a strict CSP:** `sandbox allow-scripts` (so the page has an opaque origin even if it is ever loaded without the iframe's attribute), `default-src 'none'`, scripts/styles/images/fonts/media only from `'self'`, `connect-src 'none'`, `frame-src`/`child-src`/`worker-src`/`object-src`/`manifest-src 'none'`, `base-uri 'none'`, `form-action 'none'`, and `frame-ancestors` limited to the app origin.
  - Further headers: `nosniff`, `no-referrer`, `no-store`, a deny-all `Permissions-Policy` and `Cross-Origin-Opener-Policy`.
  - No inline script and no `eval`. Classic scripts only: module scripts and web fonts would need CORS, which an opaque origin can't satisfy.
- **Launch keys (security review of decision 27.5, MEDIUM-2).** The trusted UI calls `activity_launch(id)` for every launch. Core returns the entry address under a fresh random 128-bit key (32 hex digits, `getrandom`) and remembers `key -> id`.
  - The scheme handler refuses (HTTP 403, with the activity CSP) any request whose key is missing, unknown or revoked, or whose path names an id other than the key's. So an activity cannot navigate its frame to another activity's page (which would run under the first activity's nonce, id and storage), and cannot load another activity's script, stylesheet or image. The request is refused before any file is looked up.
  - A key is revoked when its activity is launched again (the new key replaces the old one), when the UI calls `activity_launch_end(id)` (the frame was removed: paused, failed, closed, replaced), and when the app exits. Keys are never persisted.
  - The frame still resolves relative URLs against `/<key>/<id>/index.html`, and `'self'` in the CSP is the whole `lullact` origin, so bundled and installed activities needed no change. Absolute paths such as `/main.js` never worked and still don't.
  - The key is not a secret from its own activity (it can read `location`); it only has to be unguessable and useless for any other id.
  - **UI guard.** The host counts `load` events of the frame per launch. A second load means the frame navigated; the host freezes it at once with reason `navigated` ("<name> was stopped") and does not wait for a save, since the new document has no bridge. The last save is kept. This is defence in depth: the 403 page is itself a load.
  - New commands `activity_launch` and `activity_launch_end` are in the app's ACL manifest (`build.rs`) and granted to the `main` window only, like the other activity commands (`the_new_commands_are_granted_only_to_the_main_window` style test, plus `no_capability_exposes_dialogs_to_scripts`).
- **Serving is off the webview's thread, and bounded (MEDIUM-1).** The handler is `register_asynchronous_uri_scheme_protocol` (Tauri 2.12.5 `app.rs`: `H: Fn(UriSchemeContext, Request<Vec<u8>>, UriSchemeResponder)`, answered with `UriSchemeResponder::respond`), and each request runs on a blocking thread, because serving an installed file reads and hashes it (up to 5 MiB).
  - **Budget per activity.** A fixed 10-second window allows 400 requests and 48 MiB sent. A request that arrives when either is used up gets HTTP 429 (with the activity CSP) before any file is read; a request that crosses the byte limit is the last one served. A loop of `new Image().src = '/<key>/<id>/big.png?' + Math.random()` therefore costs at most about ten 5 MiB reads per window, not an unbounded number. The budget is per activity id, so one flooding activity does not starve another.
  - **Cache of verified bytes (small files only).** An installed file of at most 256 KiB that was just read and hashed is kept in memory for five seconds (4 MiB per activity at most), and served again only while its length and modification time are unchanged. A changed file is caught on the next request (length or time differs, so it is read and hashed again and fails the check). A change that restores the length and time is caught when the entry expires, and until then only the bytes that were verified are served, never the changed ones. Larger files are never cached; the budget limits how often they are hashed. This is the one place installed content is held in memory (decision 18.5 otherwise keeps none), and it lives only inside one verified snapshot.
- **The scheme handler serves only listed activities.** An activity is listed only if its manifest:
  - parses and is of kind `app`, with runtime `isolated-web-v1` and entry point `index.html`;
  - declares all three interruptibility capabilities;
  - asks for no permission other than `activity.storage`.
  The handler also refuses path traversal and serves only an allowlist of file types. In v1 the files are the bundled ones compiled into the binary. Marketplace installs (Sprint 08) must go through the same checks.
- **Why not a separate webview window?** An iframe keeps one window and one IPC surface. Chromium also puts the opaque-origin frame in its own process (out-of-process iframe), so a busy activity doesn't freeze the app: the app answered in 1–10 ms during a 3-second busy loop. A separate webview stays the fallback if a later Tauri version lets frames reach IPC (see "Tauri globals" below).

### Tauri IPC is not reachable from activities

- Capabilities grant commands to the `main` window. No capability lists the activity scheme or any remote URL. A test fails if any capability exposes dialogs to scripts. Another test fails if a window other than `main` gets the agent-settings commands (S03-T07).
- **Tauri globals.** On Windows Tauri's initialization scripts run in every frame, so `__TAURI_INTERNALS__`, `ipc` and `chrome.webview` exist inside the activity. (macOS and Linux inject them into the main frame only; see "Native IPC channels from a frame" below, which also corrects how origins are classified.)
  - The denial tests call commands through them: `dashboard`, `notification_prefs`, `activity_clear_snapshot` for another activity, and `plugin:event|emit` of `agent-attention`.
  - None of these calls is ever answered. A canary save for another activity is untouched, and a forged attention event has no effect.
  - The direct `fetch` route to `ipc.localhost` is blocked by the activity's `connect-src 'none'`.
  - The main window itself isn't granted `emit`, so no script in it, or in a frame inside it, can fake an agent event.
  - **This depends on Tauri internals.** Re-run the denial tests on every Tauri or wry upgrade. If any of these checks fails, move activities to a separate webview with no capabilities before shipping.
- Installing agent hooks also needs a native confirmation dialog from Rust that names the settings file (S03-T07). Even a script that reached IPC couldn't install hooks without the user.
  - It could still do other harm: uninstall the hooks, turn off "needs you" notifications, or read the dashboard. Those calls have no native confirmation. That is why the Tauri-globals check is a release blocker, not a nice-to-have.

### Native IPC channels from a frame (security review of decision 27.5, MEDIUM-4)

The question: can a frame inside the main window post to the host's IPC handler, and does anything but a secret stop the handler from treating that message as the main window's? Checked against the pinned sources (wry 0.57.1, tauri 2.12.5, tauri-runtime-wry 2.12.3), 2026-11-06.

| Platform | Channel in a frame | What wry reports as the message's URL | Reaches the handler from a subframe? |
|---|---|---|---|
| Windows (WebView2) | `window.chrome.webview` is present in the frame; Tauri's `window.ipc` is built on it | `ICoreWebView2WebMessageReceivedEventArgs::Source` (`webview2/mod.rs`) | Not through the `CoreWebView2` handler wry uses: frame messages need a handler on `CoreWebView2Frame`, which wry does not register. The denial tests show the canary untouched, which is the evidence. Not read from Microsoft's docs in this change. |
| macOS (WKWebView) | `window.webkit.messageHandlers.ipc` is available in every frame | `WKScriptMessage.frameInfo.request.URL` (`wkwebview/class/wry_web_view_delegate.rs`): the **sending frame's** URL | Yes. |
| Linux (WebKitGTK) | `window.webkit.messageHandlers.ipc`, same | `webview.uri()` (`webkitgtk/mod.rs`): the **top-level** URI, whichever frame sent it | Yes, and it is attributed to the main page. |

What Tauri does with a message (`ipc/protocol.rs`, `webview/mod.rs`):

- **There is no main-frame flag.** `wry::Request` carries only a URL and a body, so Rust cannot ask whether a message came from the main frame, and cannot reject non-main-frame IPC by itself. No hook runs before Tauri's own check. A Rust-side rejection is not possible with these versions; nothing was added.
- **Origin is not a barrier.** `Webview::is_local_url` treats a URL as local if its scheme (or, on Windows, `<scheme>.localhost`) is a custom protocol registered by the app. `lullact` is one, so a message from an activity frame would be classified as the `main` window's local origin, and the `main` capability (which lists the activity and marketplace commands) would apply. The earlier statement in this ADR that Tauri classifies activity origins as remote was wrong for registered schemes.
- **The invoke key is what stops it.** `Webview::on_message` drops a message unless it carries the webview's random `__TAURI_INVOKE_KEY__` (`ipc-protocol.js`: a `const` inside a closure, never put on `window`). Tauri's initialization scripts are added with `for_main_frame_only: true` (`manager/webview.rs`), and wry applies that on macOS and Linux (`wkwebview/mod.rs`, `webkitgtk/mod.rs`), so **no subframe there ever holds the key**. On Windows wry always adds scripts to subframes (wry docs on `for_main_frame_only`), so a frame **does** hold the key and `__TAURI_INTERNALS__`; there the barrier is that its message never reaches the handler (row 1).

So the control on macOS and Linux is the secrecy of the invoke key plus the main-frame-only injection; on Windows it is the missing frame handler. All three are behaviour of dependencies, not of this app. They are why "Tauri globals in frames" stays in Known gaps and why a Tauri or wry upgrade must re-run the denial tests.

Probe additions (`sandbox-probe/ipc.js`, `probe.js`, `nested.js`):

- "native IPC channels reachable from the activity frame" and "…from a nested frame" report which of `webkit.messageHandlers` (and its `ipc` handler), `chrome.webview` and `window.ipc` exist, and store them in `window.probeIpc`; the runner prints them.
- "forged native IPC call from the activity frame" and the nested-frame equivalent post one forged invoke (a guessed key, `activity_clear_snapshot` for the runner's canary save) through every channel they have. They are counted as blocked because the frame cannot see the host's answer; refusal is shown by the runner's "another activity's save is untouched" and "forged attention event had no effect" checks. On macOS and Linux, where the runner does not run, the manual check is that the probe page lists `webkit.messageHandlers (ipc)` as reachable, and that lantern-pond's save survives the run (M03-4, M03-5).

If an upgrade ever gives subframes the key, or makes wry deliver subframe messages to the main handler, the fix is the one this ADR already names: move activities to a separate webview with no capabilities.

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
- **Storage** is namespaced by activity id and needs the `activity.storage` permission. Limits (security review of decision 27.5, MEDIUM-3), all checked in Rust, in one SQLite transaction; the webview's own limits are a convenience, not the enforcement:
  - 64 KiB per value;
  - 256 KiB per activity, where **every key costs its key bytes + its value bytes + 64 bytes of row overhead**, so many tiny keys are not free;
  - at most **1024 keys** per activity (an empty-valued 1025th key is refused);
  - a **write budget** of 240 writes (`set` and `remove`) per activity per minute.
  Refusals reach the bridge as `storage-result` errors: `quota_exceeded` (bytes or key count), `value_too_large`, and the new `rate_limited` (budget used up; retry later). The SDK passes the code through as the rejection's message, so no SDK change is needed; the codes and limits are documented for creators in `docs/creators/sdk-and-bridge.md`.
  - The check sums the namespace's rows with the store lock held. That scan is bounded by the key cap (at most 1024 rows on the `(activity_id, key)` primary key), so a running per-activity counter and a schema change were not needed (no migration for this item). Rows written before this change are counted the same way; an activity already over the new limits can still read and remove keys; it can replace a value only if the result fits, and add a key only once it is back under.
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

**Windows 11 (26200), WebView2, 2026-11-06 (security review of decision 27.5): 59 of 59 checks pass with `--attention`, no known gaps.** The 11 new checks:

| Area | Result |
|---|---|
| Script and stylesheet loaded from another activity, with the frame's own key, a guessed key, or no key | Refused (error event) |
| Control: a script of the frame's own, under its own key | Loads |
| Frame navigates to another activity's page (`/<own key>/<other id>/index.html`) | Refused by the server; the host stops the frame ("was stopped", reason `navigated`) |
| Launch address carries a 128-bit key; a resumed launch gets a different one | Yes |
| Native IPC reachable from the activity frame and from a nested frame | `chrome.webview` and `window.ipc` present in both; no `webkit.messageHandlers` on Windows |
| Forged invoke (guessed key, `activity_clear_snapshot` for the canary) posted through `window.ipc` and `chrome.webview` from both frames | Refused: the canary save is untouched and the forged attention event has no effect |

That last row is the evidence for the first row of the table in "Native IPC channels from a frame": on Windows `window.ipc` and `chrome.webview` do exist in the frame (observed), Tauri's script, and so the invoke key, is injected into subframes there (wry's documentation of `for_main_frame_only`), and what keeps a frame's message from acting is that it never reaches the handler (inferred from the untouched canary, not from Microsoft's documentation).

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
- It needs a few Objective-C messages (read the configuration and preferences, check both selectors, set, read back), which are unsafe Rust. They live in their own tiny crate, `crates/macos-webrtc`, one of the two crates in the workspace that allow unsafe code (the other is `windows-package`, added for decision 33; see the ADR-010 amendment), so `lullshift-desktop` keeps `forbid(unsafe_code)` through the workspace lints. A test fails if any other crate stops inheriting those lints, lowers `unsafe_code`, or contains an `unsafe` block or an `allow(unsafe_code)` attribute. The function body is the one checked against `objc2` 0.6.5 in Sprint 12 (it type-checks for `aarch64-apple-darwin`); it takes the view pointer that Tauri's `PlatformWebview::inner()` returns. It has not been run on macOS.

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
- **macOS relies on WebKit SPI.** If it goes away, `webrtc::state()` records the failure, and the third-party code warning says plainly that the block couldn't be applied on this computer (amendment below).
- **Windows is asserted, not confirmed.** The arguments are applied when the webview is created and WebView2 gives no way to read them back, so `webrtc::state()` is `Asserted` there, not `Off`. Since owner decision 27.5 this no longer keeps third-party code out; the install warning says that the block can't be read back on this computer (amendment below). A run-time read-back (for example the activity probe run from the app) would still let the warning say more.
- **macOS and Linux have not been run.** The runner needs the Chromium debug protocol. On those OSes, open the probe in a debug build and read its results page: every WebRTC line must say "blocked", with "API absent". Those are manual checks.

### Known gaps

- **WebRTC.** Closed in Sprint 12 on Windows (proof passes in full), and turned off on macOS and Linux with their proof still to run (see "WebRTC is off in the webview" above). Third-party code activities can be installed on every OS after a risk warning and consent (owner decision 27.5, amendment below).
- **Tauri globals in frames** (above): mitigated today, but by behaviour we don't control.
- **Before third-party code activities (Sprint 08)**, these were to be settled. They were found in the Sprint 03 security review. Owner decision 27.5 opened third-party code with a consent warning before they were all closed; the security review of that change (commit 72d3576) then closed (a) and (b):
  - **(a) Loading another activity: closed.** Per-launch keys in the address (see "Launch keys"); the server refuses another id's files and unknown or revoked keys, and the UI stops a frame that loads a second document. Denial checks: scripts and stylesheets from another activity are refused, and a navigation to another activity's page stops the frame.
  - **(b) Storage quota: closed.** A cost per row (64 bytes), at most 1024 keys, and a write budget of 240 per activity per minute, all in Rust. See "Storage".
  - **"Nothing to save" looks like no answer.** An activity that has nothing to report is closed as unresponsive after three autosaves. The SDK's bridge client now answers with the last known state (Sprint 05). Activities that use their own client must still always return a state.
  - **Uninstalling hooks and changing notification settings** have no native confirmation (see above).

## Consequences

- Activities are static web pages that talk only through the bridge. The SDK, kits and engines (Sprints 05–07) build on protocol v1. A change that breaks the protocol needs protocol v2 and this ADR updated.
- Because a pause unloads the page, activities must restore from a snapshot. Timers, audio and animation stop on every pause. That is the point.
- The debug-only probe and the denial tests are part of the quality gate for any change to the host, the scheme handler, CSPs, capabilities, the webview's browser arguments, or Tauri or WebView2 runtime versions.

## Amendment: third-party code with consent (owner decision 27.5, 2026-11-06)

The product owner opened third-party code activities on Windows, macOS and Linux, with a risk warning and explicit consent at each install and at each update that changes the code's pinned commit (`sprints/DECISIONS_FOR_REVIEW.md` item 27.5; details in the ADR-015 amendment). This replaces the rule that third-party code is enabled **only where the per-OS sandbox proof passed**, and the Sprint 12 extra lock that needed `webrtc::state() == Off`.

**Nothing in this ADR's sandbox changes.** Third-party code is served on `lullact:` with the same activity CSP, the same response headers, the same path rules and the same sandboxed iframe as bundled code. WebRTC stays off app-wide on every OS. The bridge, the permissions model (`activity.storage` only) and host-owned interruption are the same. The denial tests remain the quality gate for any change to them.

**The WebRTC state is now information.** `webrtc::set_state` tells `marketplace::set_webrtc_block`, which only picks the warning's honest note: Windows `Asserted` says the proof passed on a test computer but the block can't be read back here; macOS and Linux `Off` say the block is in place and read back but the proof hasn't run on that platform; `Failed` says plainly that the block couldn't be applied on this computer; anything else says the block isn't confirmed yet. No note claims a guarantee, and none blocks the install.

**Residual risks** (also in the ADR-015 amendment): macOS and Linux unverified; Windows not read back; `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` or the WebView2 registry policy can undo the Windows block; DNS prefetch untested; the remaining Sprint 03 items under "Known gaps" (the "nothing to save" case and native confirmation of hook uninstalls) are still open; frame navigation to another activity's page and the storage quota were closed in the security review of this decision.

**Project rules.** The "only where the sandbox proof passed" rule came from Sprint 08 (S08-T05) and `docs/TECHNICAL_ARCHITECTURE.md` §6.4, which now point here. `CLAUDE.md` is changed only by the owner; the suggested wording for its activity rules is in this change's report.
