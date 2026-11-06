// WebRTC attempts for the sandbox probe (S12-T13, ADR-012). Debug builds only.
//
// Loaded by the probe page and by a nested frame inside it (nested.js), and evaluated by
// tests/sandbox/run-denial-tests.mjs in the app page, so the same attempt runs in every realm.
// The host turns WebRTC off in the webview itself, because CSP does not govern it.
"use strict";

function rtcNoop() {}

function rtcConstructor(w) {
  var PC = w && (w.RTCPeerConnection || w.webkitRTCPeerConnection);
  return typeof PC === "function" ? PC : null;
}

/**
 * Two peer connections in window `w`, one with a STUN server, connected to each other with a
 * data channel. Calls `finish(blocked, detail)` once. Escaped if the data channel opens, or if
 * ICE gathers a candidate that needs a socket (UDP, server-reflexive, relay or passive TCP). An
 * active TCP host candidate opens no socket by itself; whether it can reach anything is checked
 * by `rtcEgress` against the runner's listeners.
 */
function rtcAttempt(w, finish) {
  var PC = rtcConstructor(w);
  if (!PC) return finish(true, "API absent");
  var a, b;
  var seen = [];
  var socketed = [];
  var over = false;
  function done(blocked, detail) {
    if (over) return;
    over = true;
    try {
      a.close();
      b.close();
    } catch (e) {
      // already closed
    }
    finish(blocked, detail);
  }
  // The deadline comes first, so a slow constructor can't outlast it.
  setTimeout(function () {
    if (socketed.length) return done(false, "ICE candidates gathered: " + socketed.join(", "));
    var state = a ? a.iceGatheringState + "/" + a.iceConnectionState : "not created";
    done(
      true,
      "no channel; ICE " +
        state +
        (seen.length ? "; socketless: " + seen.join(", ") : "; no candidates"),
    );
  }, 4000);
  try {
    a = new PC({ iceServers: [{ urls: "stun:stun.example.com:3478" }] });
    b = new PC();
  } catch (e) {
    return done(true, "constructor threw: " + (e && e.name));
  }
  a.onicecandidate = function (e) {
    var c = e.candidate;
    if (!c || !c.candidate) return;
    var fields = c.candidate.split(" ");
    var protocol = (c.protocol || fields[2] || "").toLowerCase();
    var type = c.type || fields[7] || "?";
    var tcpType = c.tcpType || (c.candidate.match(/tcptype (\w+)/) || [])[1] || "";
    var label = protocol + "/" + type + (tcpType ? "/" + tcpType : "");
    seen.push(label);
    if (!(protocol === "tcp" && type === "host" && tcpType === "active")) socketed.push(label);
    b.addIceCandidate(c).catch(rtcNoop);
  };
  b.onicecandidate = function (e) {
    if (e.candidate) a.addIceCandidate(e.candidate).catch(rtcNoop);
  };
  var channel;
  try {
    channel = a.createDataChannel("probe");
  } catch (e) {
    return done(true, "createDataChannel threw: " + (e && e.name));
  }
  channel.onopen = function () {
    done(false, "data channel opened (" + seen.join(", ") + ")");
  };
  a.createOffer()
    .then(function (o) {
      return a.setLocalDescription(o);
    })
    .then(function () {
      return b.setRemoteDescription(a.localDescription);
    })
    .then(function () {
      return b.createAnswer();
    })
    .then(function (o) {
      return b.setLocalDescription(o);
    })
    .then(function () {
      return a.setRemoteDescription(b.localDescription);
    })
    .catch(rtcNoop);
}

/**
 * Tries to reach the runner's listeners from window `w`: STUN to `127.0.0.1:cfg.udp` and
 * `cfg.far`, TURN over TCP to `127.0.0.1:cfg.tcp` and `cfg.far`, and remote host candidates
 * (UDP and passive TCP) for both. Resolves to a description; the runner decides by what its
 * listeners and `netstat` saw, not by what the page says.
 */
function rtcEgress(w, cfg) {
  var PC = rtcConstructor(w);
  if (!PC) return Promise.resolve("API absent");
  var a, b;
  try {
    a = new PC({
      iceServers: [
        { urls: ["stun:127.0.0.1:" + cfg.udp, "stun:" + cfg.far + ":3478"] },
        {
          urls: [
            "turn:127.0.0.1:" + cfg.tcp + "?transport=tcp",
            "turn:" + cfg.far + ":3478?transport=tcp",
          ],
          username: "u",
          credential: "p",
        },
      ],
    });
    b = new PC();
    a.createDataChannel("x");
  } catch (e) {
    return Promise.resolve("refused: " + (e && e.name));
  }
  var remote = [
    "candidate:1 1 udp 2122260223 127.0.0.1 " + cfg.udp + " typ host",
    "candidate:2 1 tcp 1518280447 127.0.0.1 " + cfg.tcp + " typ host tcptype passive",
    "candidate:3 1 udp 2122260222 " + cfg.far + " 3478 typ host",
    "candidate:4 1 tcp 1518280446 " + cfg.far + " 3479 typ host tcptype passive",
  ];
  function close() {
    try {
      a.close();
      b.close();
    } catch (e) {
      // already closed
    }
  }
  return a
    .createOffer()
    .then(function (o) {
      return a.setLocalDescription(o).then(function () {
        return b.setRemoteDescription(o);
      });
    })
    .then(function () {
      return b.createAnswer();
    })
    .then(function (o) {
      return b.setLocalDescription(o).then(function () {
        return a.setRemoteDescription(o);
      });
    })
    .then(function () {
      return remote.reduce(function (p, candidate) {
        return p.then(function () {
          return a.addIceCandidate({ candidate: candidate, sdpMid: "0" }).catch(rtcNoop);
        });
      }, Promise.resolve());
    })
    .then(function () {
      return new Promise(function (r) {
        setTimeout(r, 5000);
      });
    })
    .then(
      function () {
        var state = a.iceGatheringState + "/" + a.iceConnectionState;
        close();
        return "ran; ICE " + state;
      },
      function (e) {
        close();
        return "failed: " + (e && e.name);
      },
    );
}
