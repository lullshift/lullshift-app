// Which native IPC channels a frame can reach, and a forged call through each (ADR-012,
// security review of decision 27.5, MEDIUM-4). Shared by probe.js and nested.js. Debug builds only.
//
// WebView2 has `window.chrome.webview`; WKWebView and WebKitGTK have
// `window.webkit.messageHandlers.ipc`. Tauri's script also defines `window.ipc` in the main
// frame. A frame that can post to the host's IPC handler is only safe if the host refuses the
// message. The forged call here uses a guessed invoke key and names a command with a visible
// side effect (clearing the runner's canary save), so a message that was accepted shows up as a
// failed "another activity's save is untouched" check in the runner, or by hand on macOS and
// Linux (see docs/MANUAL_TESTS.md).
"use strict";

var CANARY_ID = "app.lullshift.games.lantern-pond";

function ipcSurface(win) {
  var out = { webkit: false, handlers: [], ipc: false, chromeWebview: false, windowIpc: false };
  try {
    var mh = win.webkit && win.webkit.messageHandlers;
    if (mh) {
      out.webkit = true;
      out.ipc = !!(mh.ipc && typeof mh.ipc.postMessage === "function");
      for (var k in mh) out.handlers.push(k);
    }
  } catch (e) {
    out.error = e && e.name ? e.name : "error";
  }
  try {
    out.chromeWebview = !!(win.chrome && win.chrome.webview);
  } catch (e) {
    // Not reachable from this frame.
  }
  try {
    out.windowIpc = !!(win.ipc && typeof win.ipc.postMessage === "function");
  } catch (e) {
    // Not reachable from this frame.
  }
  return out;
}

function describeSurface(s) {
  var parts = [];
  if (s.webkit) parts.push("webkit.messageHandlers" + (s.ipc ? " (ipc)" : ""));
  if (s.chromeWebview) parts.push("chrome.webview");
  if (s.windowIpc) parts.push("window.ipc");
  return parts.length ? "reachable: " + parts.join(", ") : "none reachable";
}

/** Posts one forged invoke through every channel `win` has. Returns the channels used. */
function forgeIpc(win) {
  var body = JSON.stringify({
    cmd: "activity_clear_snapshot",
    callback: 1,
    error: 2,
    payload: { id: CANARY_ID },
    options: {},
    __TAURI_INVOKE_KEY__: "probe-guessed-key",
  });
  var via = [];
  try {
    if (win.webkit && win.webkit.messageHandlers && win.webkit.messageHandlers.ipc) {
      win.webkit.messageHandlers.ipc.postMessage(body);
      via.push("webkit.messageHandlers.ipc");
    }
  } catch (e) {
    // Refused by the engine.
  }
  try {
    if (win.ipc && typeof win.ipc.postMessage === "function") {
      win.ipc.postMessage(body);
      via.push("window.ipc");
    }
  } catch (e) {
    // Refused by the engine.
  }
  try {
    if (win.chrome && win.chrome.webview) {
      win.chrome.webview.postMessage(body);
      via.push("chrome.webview");
    }
  } catch (e) {
    // Refused by the engine.
  }
  return via;
}
