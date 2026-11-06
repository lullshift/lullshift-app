// Sandbox probe (S03-T05, ADR-012). Debug builds only. Attempts each escape an untrusted
// activity might try and records whether it was blocked. Results go to `window.probeResults`
// and the page, where `tests/sandbox/run-denial-tests.mjs` reads them over the debug port.
//
// It is also a well-behaved activity: a tick counter that saves its state, so the same page
// checks host-enforced pause and restore (S03-A01).
"use strict";

var results = [];
var violations = [];
var ticks = 0;
var stubborn = false;

document.addEventListener("securitypolicyviolation", function (e) {
  violations.push(e.effectiveDirective + " " + e.blockedURI);
});

function record(name, blocked, detail) {
  results.push({ name: name, blocked: blocked, detail: String(detail || "").slice(0, 160) });
}

function attempt(name, fn) {
  try {
    var outcome = fn();
    record(name, !outcome, outcome ? "succeeded: " + outcome : "no effect");
  } catch (e) {
    record(name, true, e && e.name ? e.name + ": " + e.message : e);
  }
}

function attemptAsync(name, fn, ms) {
  return new Promise(function (resolve) {
    var done = false;
    function finish(blocked, detail) {
      if (done) return;
      done = true;
      record(name, blocked, detail);
      resolve();
    }
    setTimeout(function () {
      finish(true, "no response within " + ms + " ms");
    }, ms);
    try {
      fn(finish);
    } catch (e) {
      finish(true, e && e.name ? e.name + ": " + e.message : e);
    }
  });
}

var REMOTE = "https://example.com/";

/** A srcdoc frame nested in the probe that runs rtc.js and nested.js in its own realm. */
function nestedFrame(onMessage) {
  var f = document.createElement("iframe");
  window.addEventListener("message", function (e) {
    if (e.source === f.contentWindow && e.data && e.data.probeNested) onMessage(e.data, f);
  });
  f.srcdoc = '<script src="rtc.js"></script><script src="nested.js"></script>';
  document.body.appendChild(f);
  return f;
}

// For the runner: WebRTC egress from a nested frame's own realm (S12-T13).
window.probeNestedEgress = function (cfg) {
  return new Promise(function (resolve) {
    var ran = false;
    nestedFrame(function (m, f) {
      if (m.probeNested === "ready") {
        ran = true;
        f.contentWindow.postMessage({ probeEgress: cfg }, "*");
      } else if (m.probeNested === "egress") {
        resolve("nested realm: " + m.detail);
      }
    });
    setTimeout(function () {
      resolve(ran ? "nested realm: no answer" : "nested script did not run");
    }, 12000);
  });
};

function rtcCheck(name, getWindow) {
  return attemptAsync(
    name,
    function (finish) {
      rtcAttempt(getWindow(), finish);
    },
    6000,
  );
}

function runChecks() {
  // Informational: whether Tauri's scripts are injected into frames. What matters is whether
  // they work, which the "Tauri command" checks below test.
  window.probeGlobals = ["__TAURI_INTERNALS__", "__TAURI__", "ipc"]
    .filter(function (k) {
      return typeof window[k] !== "undefined";
    })
    .concat(window.chrome && window.chrome.webview ? ["chrome.webview"] : []);
  attempt("read parent document", function () {
    return window.parent.document ? "parent document readable" : false;
  });
  attempt("read parent globals", function () {
    return window.parent.__TAURI_INTERNALS__ ? "parent Tauri internals readable" : false;
  });
  attempt("navigate top window", function () {
    window.top.location.href = REMOTE;
    return "navigation call did not throw";
  });
  attempt("open popup", function () {
    var w = window.open(REMOTE);
    return w ? "window opened" : false;
  });
  attempt("eval", function () {
    return eval("1 + 1") === 2 ? "eval ran" : false;
  });
  attempt("Function constructor", function () {
    return new Function("return 2")() === 2 ? "Function ran" : false;
  });
  attempt("inline script", function () {
    var s = document.createElement("script");
    s.textContent = "window.__inlineRan = true;";
    document.body.appendChild(s);
    return window.__inlineRan ? "inline script ran" : false;
  });
  attempt("localStorage", function () {
    window.localStorage.setItem("k", "v");
    return "localStorage writable";
  });
  attempt("sessionStorage", function () {
    window.sessionStorage.setItem("k", "v");
    return "sessionStorage writable";
  });
  attempt("cookies", function () {
    document.cookie = "k=v";
    return document.cookie.indexOf("k=v") >= 0 ? "cookie set" : false;
  });
  var beaconUrl = REMOTE + "beacon";
  var beaconQueued = false;
  try {
    beaconQueued = navigator.sendBeacon(beaconUrl, "x");
  } catch (e) {
    beaconQueued = false;
  }

  function tauriInvoke(name, cmd, args) {
    return attemptAsync(
      name,
      function (finish) {
        var internals = window.__TAURI_INTERNALS__;
        if (!internals || typeof internals.invoke !== "function") return finish(true, "absent");
        internals.invoke(cmd, args || {}).then(
          function (v) {
            finish(false, "command answered: " + JSON.stringify(v).slice(0, 80));
          },
          function (e) {
            finish(true, "rejected: " + e);
          },
        );
      },
      4000,
    );
  }

  var asyncChecks = [
    tauriInvoke("Tauri command: dashboard", "dashboard"),
    tauriInvoke("Tauri command: notification settings", "notification_prefs"),
    tauriInvoke("Tauri command: clear an activity's save", "activity_clear_snapshot", {
      id: "app.lullshift.games.lantern-pond",
    }),
    tauriInvoke("Tauri plugin: emit event", "plugin:event|emit", {
      event: "agent-attention",
      payload: "probe",
    }),
    attemptAsync(
      "webview native message",
      function (finish) {
        var wv = window.chrome && window.chrome.webview;
        if (!wv) return finish(true, "absent");
        wv.postMessage("probe");
        // The host can't confirm receipt to the probe; the runner checks that nothing changed.
        finish(true, "posted; no channel back");
      },
      1000,
    ),
    attemptAsync(
      "sendBeacon",
      function (finish) {
        setTimeout(function () {
          var blocked = violations.some(function (v) {
            return v.indexOf("connect-src") === 0 && v.indexOf("beacon") >= 0;
          });
          finish(
            blocked || !beaconQueued,
            blocked ? "CSP connect-src violation" : "queued, no violation seen",
          );
        }, 1000);
      },
      3000,
    ),
    attemptAsync(
      "fetch remote",
      function (finish) {
        fetch(REMOTE, { mode: "no-cors" }).then(
          function () {
            finish(false, "request completed");
          },
          function (e) {
            finish(true, e.name + ": " + e.message);
          },
        );
      },
      4000,
    ),
    attemptAsync(
      "fetch Tauri IPC",
      function (finish) {
        fetch("http://ipc.localhost/dashboard", { method: "POST", body: "{}" }).then(
          function (r) {
            finish(false, "IPC answered " + r.status);
          },
          function (e) {
            finish(true, e.name + ": " + e.message);
          },
        );
      },
      4000,
    ),
    attemptAsync(
      "XMLHttpRequest remote",
      function (finish) {
        var x = new XMLHttpRequest();
        x.onload = function () {
          finish(false, "loaded " + x.status);
        };
        x.onerror = function () {
          finish(true, "error event");
        };
        x.open("GET", REMOTE);
        x.send();
      },
      4000,
    ),
    attemptAsync(
      "WebSocket",
      function (finish) {
        var ws = new WebSocket("wss://example.com/");
        ws.onopen = function () {
          finish(false, "socket opened");
        };
        ws.onerror = function () {
          finish(true, "error event");
        };
      },
      4000,
    ),
    attemptAsync(
      "EventSource",
      function (finish) {
        var es = new EventSource(REMOTE);
        es.onopen = function () {
          finish(false, "stream opened");
        };
        es.onerror = function () {
          finish(true, "error event");
          es.close();
        };
      },
      4000,
    ),
    attemptAsync(
      "remote image",
      function (finish) {
        var img = new Image();
        img.onload = function () {
          finish(false, "image loaded");
        };
        img.onerror = function () {
          finish(true, "error event");
        };
        img.src = REMOTE + "favicon.ico";
      },
      4000,
    ),
    attemptAsync(
      "nested remote frame",
      function (finish) {
        var f = document.createElement("iframe");
        f.onload = function () {
          var blocked = violations.some(function (v) {
            return v.indexOf("frame-src") === 0;
          });
          finish(blocked, blocked ? "CSP frame-src violation" : "frame loaded");
        };
        f.src = REMOTE;
        document.body.appendChild(f);
      },
      4000,
    ),
    attemptAsync(
      "worker",
      function (finish) {
        var w = new Worker(URL.createObjectURL(new Blob(["postMessage(1)"])));
        w.onmessage = function () {
          finish(false, "worker ran");
        };
        w.onerror = function () {
          finish(true, "error event");
        };
      },
      3000,
    ),
    attemptAsync(
      "IndexedDB",
      function (finish) {
        var req = indexedDB.open("probe");
        req.onsuccess = function () {
          finish(false, "database opened");
        };
        req.onerror = function () {
          finish(true, "error event");
        };
      },
      3000,
    ),
    attemptAsync(
      "clipboard write",
      function (finish) {
        navigator.clipboard.writeText("probe").then(
          function () {
            finish(false, "clipboard written");
          },
          function (e) {
            finish(true, e.name + ": " + e.message);
          },
        );
      },
      3000,
    ),
    attemptAsync(
      "notifications",
      function (finish) {
        if (typeof Notification === "undefined") return finish(true, "API absent");
        Notification.requestPermission().then(function (p) {
          finish(p !== "granted", "permission " + p);
        });
      },
      3000,
    ),
    // WebRTC (S12-T13, ADR-012). CSP does not govern WebRTC, so the host turns it off in the
    // webview itself. Each check tries the same thing from a different script realm: the
    // activity frame, a nested blank frame, a nested frame that runs its own script (each a fresh
    // set of constructors), and a worker.
    rtcCheck("WebRTC peer connection", function () {
      return window;
    }),
    // The parent scripting a nested blank frame's fresh realm directly.
    rtcCheck("WebRTC in a nested blank frame", function () {
      var f = document.createElement("iframe");
      document.body.appendChild(f);
      return f.contentWindow;
    }),
    attemptAsync(
      "WebRTC in a nested frame running its own script",
      function (finish) {
        var api = "?";
        nestedFrame(function (m) {
          if (m.probeNested === "ready") api = m.api;
          if (m.probeNested === "attempt") {
            finish(m.blocked, "RTCPeerConnection " + api + "; " + m.detail);
          }
        });
      },
      8000,
    ),
    attemptAsync(
      "WebRTC in a worker",
      function (finish) {
        var w = new Worker(
          URL.createObjectURL(
            new Blob(["postMessage(typeof RTCPeerConnection + ' ' + typeof RTCDataChannel)"]),
          ),
        );
        w.onmessage = function (e) {
          finish(String(e.data).indexOf("function") !== 0, "worker sees: " + e.data);
        };
        w.onerror = function () {
          finish(true, "worker did not run");
        };
      },
      3000,
    ),
    attemptAsync(
      "forged host message (wrong nonce)",
      function (finish) {
        // A message pretending to be from another launch must be dropped by the host.
        window.parent.postMessage(
          {
            lullshift: 1,
            nonce: "0".repeat(32),
            type: "storage",
            requestId: "x",
            op: "get",
            key: "k",
          },
          "*",
        );
        window.addEventListener("message", function (e) {
          if (e.data && e.data.requestId === "x") finish(false, "host answered a forged message");
        });
      },
      1500,
    ),
    attemptAsync(
      "storage outside quota",
      function (finish) {
        LullshiftBridge.storage.set("big", "x".repeat(70 * 1024)).then(
          function () {
            finish(false, "70 KB value stored");
          },
          function (e) {
            finish(true, e.message);
          },
        );
      },
      3000,
    ),
  ];

  // Positive control: the bridge works for allowed calls.
  var control = LullshiftBridge.storage.set("probe", "ok").then(function () {
    return LullshiftBridge.storage.get("probe");
  });

  Promise.all(asyncChecks)
    .then(function () {
      return control.then(
        function (v) {
          record("control: bridge storage works", v !== "ok", "read back " + v);
        },
        function (e) {
          record("control: bridge storage works", true, e.message);
        },
      );
    })
    .then(function () {
      // self.origin is the security origin ("null" when sandboxed); location.origin is the URL's.
      window.probeResults = { results: results, violations: violations, origin: self.origin };
      document.getElementById("results").textContent = results
        .map(function (r) {
          return (r.blocked ? "blocked  " : "ESCAPED  ") + r.name + " (" + r.detail + ")";
        })
        .join("\n");
      window.probeDone = true;
    });
}

LullshiftBridge.start({
  onInit: function (ctx) {
    ticks = ctx.restore && typeof ctx.restore.ticks === "number" ? ctx.restore.ticks : 0;
    window.restoredTicks = ticks;
    setInterval(function () {
      ticks += 1;
      document.getElementById("ticks").textContent = "Ticks: " + ticks;
      if (ticks % 10 === 0 && !stubborn) LullshiftBridge.autosave({ ticks: ticks });
    }, 100);
    runChecks();
  },
  // In stubborn mode the probe ignores save requests, like a broken or hostile activity.
  saveState: function () {
    if (stubborn) throw new Error("stubborn");
    return { ticks: ticks };
  },
});

window.probe = {
  ticks: function () {
    return ticks;
  },
  beStubborn: function () {
    // Stops answering save requests and stops reporting on its own, so only the host's last
    // copy remains.
    stubborn = true;
  },
};
