// Lullshift activity bridge client, protocol v1 (ADR-012), from @lullshift/activity-sdk 0.1.0.
//
// Copy this file next to your index.html (`lullshift create` does it for you) and load it as a
// classic script before your own code. Activities can't use module scripts:
//   <script src="lullshift-bridge-v1.js"></script>
// Every bundled activity carries an identical copy; a test checks them.
//
// The host owns interruption. When an agent needs the user, the host asks for your state and
// then unloads the page, whether or not you answer. Report state often with
// `LullshiftBridge.autosave(state)` so nothing is lost, and rebuild from `restore` in onInit.
//
//   LullshiftBridge.start({
//     onInit({ restore, reducedMotion, theme, content }) { ... }, // restore is null when fresh
//     saveState() { return { ... } },                     // any JSON value except null
//     onPause() { ... }, onResume() { ... },              // optional hints
//   });
//   LullshiftBridge.summary([{ label: "Moves", value: 12 }]); // shown while paused
//   const best = await LullshiftBridge.storage.get("best");   // needs "activity.storage"
//
// `content` is the pack data when this activity is an engine running a content pack (ADR-016),
// and null otherwise. Help and the preview image come from manifest.json, not from code.
(function () {
  "use strict";
  var PROTOCOL = 1;
  var match = /(?:^#|&)n=([0-9a-f]{32})(?:&|$)/.exec(location.hash);
  var nonce = match ? match[1] : "";
  var handlers = {};
  var pending = {};
  var counter = 0;
  // The last state the host has, or will have: what we restored from or last reported. A save
  // request is always answered with something, so the host never mistakes "nothing new" for a
  // stuck activity.
  var lastState;

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

  function usable(state) {
    return state !== undefined && state !== null;
  }

  // The activity's current state, or the last known one if it has nothing to report.
  function currentState() {
    var state;
    try {
      state = handlers.saveState ? handlers.saveState() : undefined;
    } catch (e) {
      state = undefined;
    }
    if (usable(state)) lastState = state;
    return lastState;
  }

  window.addEventListener("message", function (event) {
    if (event.source !== window.parent) return;
    var m = event.data;
    if (!m || m.lullshift !== PROTOCOL || m.nonce !== nonce) return;
    if (m.type === "init") {
      if (usable(m.restore)) lastState = m.restore;
      if (handlers.onInit) {
        handlers.onInit({
          restore: usable(m.restore) ? m.restore : null,
          reducedMotion: !!m.reducedMotion,
          theme: m.theme === "dark" ? "dark" : "light",
          content: m.content === undefined ? null : m.content,
        });
      }
    } else if (m.type === "pause" && handlers.onPause) {
      handlers.onPause();
    } else if (m.type === "resume" && handlers.onResume) {
      handlers.onResume();
    } else if (m.type === "save") {
      var state = currentState();
      if (usable(state)) send("state", { requestId: m.requestId, state: state });
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
      if (!usable(state)) return;
      lastState = state;
      send("state", { requestId: null, state: state });
    },
    // One to four { label, value } pairs: plain-text labels and values of at most 24
    // characters, or numbers. The host shows the latest while the activity is paused.
    summary: function (items) {
      send("summary", { items: items });
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
