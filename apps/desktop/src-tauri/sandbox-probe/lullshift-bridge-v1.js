// Lullshift activity bridge client, protocol v1 (ADR-012).
//
// Each bundled activity carries an identical copy of this file (a test checks the copies). The
// Activity SDK (Sprint 05) replaces it. Load it as a classic script before your own code:
//   <script src="lullshift-bridge-v1.js"></script>
//
// The host owns interruption. When an agent needs the user, the host asks for your state and
// then unloads the page, whether or not you answer. Report state often with
// `LullshiftBridge.autosave(state)` so nothing is lost, and rebuild from `restore` in onInit.
//
//   LullshiftBridge.start({
//     onInit({ restore, reducedMotion, theme }) { ... },  // restore is null on a fresh start
//     saveState() { return { ... } },                     // any JSON-serializable value
//     onPause() { ... }, onResume() { ... },              // optional hints
//   });
//   const best = await LullshiftBridge.storage.get("best"); // needs "activity.storage"
(function () {
  "use strict";
  var PROTOCOL = 1;
  var match = /(?:^#|&)n=([0-9a-f]{32})(?:&|$)/.exec(location.hash);
  var nonce = match ? match[1] : "";
  var handlers = {};
  var pending = {};
  var counter = 0;

  function send(type, fields) {
    var message = { lullshift: PROTOCOL, nonce: nonce, type: type };
    for (var k in fields) message[k] = fields[k];
    // The parent is the Lullshift host; this page's own origin is opaque, so "*" is required.
    window.parent.postMessage(message, "*");
  }

  function request(fields) {
    counter += 1;
    var id = "r" + counter;
    return new Promise(function (resolve, reject) {
      pending[id] = { resolve: resolve, reject: reject };
      fields.requestId = id;
      send("storage", fields);
    });
  }

  // `undefined` means "nothing to report": the host then keeps the state it already has.
  function currentState() {
    try {
      return handlers.saveState ? handlers.saveState() : undefined;
    } catch (e) {
      return undefined;
    }
  }

  window.addEventListener("message", function (event) {
    if (event.source !== window.parent) return;
    var m = event.data;
    if (!m || m.lullshift !== PROTOCOL || m.nonce !== nonce) return;
    if (m.type === "init" && handlers.onInit) {
      handlers.onInit({ restore: m.restore, reducedMotion: !!m.reducedMotion, theme: m.theme });
    } else if (m.type === "pause" && handlers.onPause) {
      handlers.onPause();
    } else if (m.type === "resume" && handlers.onResume) {
      handlers.onResume();
    } else if (m.type === "save") {
      var state = currentState();
      if (state !== undefined && state !== null) {
        send("state", { requestId: m.requestId, state: state });
      }
    } else if (m.type === "storage-result" && pending[m.requestId]) {
      var p = pending[m.requestId];
      delete pending[m.requestId];
      if (m.ok) p.resolve(m.value === undefined ? null : m.value);
      else p.reject(new Error(m.error || "storage_failed"));
    }
  });

  window.LullshiftBridge = Object.freeze({
    start: function (h) {
      handlers = h || {};
      send("ready", {});
    },
    autosave: function (state) {
      send("state", { requestId: null, state: state });
    },
    storage: Object.freeze({
      get: function (key) {
        return request({ op: "get", key: key });
      },
      set: function (key, value) {
        return request({ op: "set", key: key, value: String(value) });
      },
      remove: function (key) {
        return request({ op: "remove", key: key });
      },
    }),
  });
})();
