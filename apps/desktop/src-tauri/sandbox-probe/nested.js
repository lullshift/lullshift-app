// Runs inside a srcdoc frame that the probe nests in itself (S12-T13). The frame is a fresh
// script realm with its own constructors, and the parent can't reach into it (it gets its own
// opaque origin), so it reports to the parent by message. Debug builds only.
"use strict";

rtcAttempt(window, function (blocked, detail) {
  parent.postMessage(
    { probeNested: "attempt", blocked: blocked, detail: String(detail).slice(0, 160) },
    "*",
  );
});

window.addEventListener("message", function (e) {
  if (e.source !== parent || !e.data || !e.data.probeEgress) return;
  rtcEgress(window, e.data.probeEgress).then(function (detail) {
    parent.postMessage({ probeNested: "egress", detail: detail }, "*");
  });
});

parent.postMessage({ probeNested: "ready", api: typeof RTCPeerConnection }, "*");
