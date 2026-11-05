// Sandbox denial tests for activities (S03-T05, ADR-012), run against the real app.
//
// The probe activity (debug builds only) tries each escape from inside the sandbox. This script
// launches it in a running debug build over the WebView debug port, collects its results, and
// checks what the probe can't check from inside: a stuck activity doesn't freeze the app,
// the host pauses and restores without the activity's help, and the frame can't navigate away.
//
// Usage (Windows, from the repo root):
//   pnpm --dir apps/desktop build
//   pnpm --dir apps/desktop tauri build --debug --no-bundle
//   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9333"
//   target/debug/lullshift-desktop.exe
//   node tests/sandbox/run-denial-tests.mjs [--attention]
//
// --attention also sends a real "needs you" hook event through the hook forwarder, for a test
// session named sandbox-test-<time> that is then ended. It appears in the app's History.
//
// Exit code 0 when every check passes. Known gaps (listed in ADR-012) are reported, not failed.

import { spawnSync } from "node:child_process";
import { join } from "node:path";

const PORT = 9333;
const PROBE_ID = "app.lullshift.test.sandbox-probe";
const KNOWN_GAPS = new Set(["WebRTC peer connection"]);
const withAttention = process.argv.includes("--attention");

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function pageTarget() {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();
      const page = list.find((t) => t.type === "page" && t.url.includes("localhost"));
      if (page) return page;
    } catch {
      // not up yet
    }
    await sleep(500);
  }
  throw new Error(`no app page on debug port ${PORT}; is the debug build running?`);
}

const target = await pageTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r, { once: true }));
let nextId = 0;
const pending = new Map();
const listeners = [];
ws.addEventListener("message", (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) {
    pending.get(m.id)(m);
    pending.delete(m.id);
  } else if (m.method) {
    for (const l of listeners) l(m);
  }
});
const send = (method, params = {}, sessionId) =>
  new Promise((resolve) => {
    nextId += 1;
    pending.set(nextId, resolve);
    ws.send(JSON.stringify({ id: nextId, method, params, ...(sessionId ? { sessionId } : {}) }));
  });

// Execution contexts per frame, and attached out-of-process iframe sessions.
const contexts = new Map(); // frameId -> contextId (page session)
let frameSession = null;
listeners.push((m) => {
  if (m.method === "Runtime.executionContextCreated" && !m.sessionId) {
    const ctx = m.params.context;
    if (ctx.auxData?.isDefault) contexts.set(ctx.auxData.frameId, ctx.id);
  }
  if (m.method === "Target.attachedToTarget" && m.params.targetInfo.type === "iframe") {
    frameSession = m.params.sessionId;
    void send("Runtime.runIfWaitingForDebugger", {}, frameSession);
  }
  if (m.method === "Target.detachedFromTarget" && m.params.sessionId === frameSession) {
    frameSession = null;
  }
});

async function evalIn(expression, { sessionId, contextId } = {}) {
  const r = await send(
    "Runtime.evaluate",
    { expression, returnByValue: true, awaitPromise: true, ...(contextId ? { contextId } : {}) },
    sessionId,
  );
  if (r.error) throw new Error(r.error.message);
  if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.text);
  return r.result?.result?.value;
}
const main = (expression) => evalIn(expression);

async function mainFrameId() {
  const tree = await send("Page.getFrameTree");
  return tree.result.frameTree.frame.id;
}

/** Evaluates inside the activity frame, wherever Chromium put it. */
async function inFrame(expression) {
  if (frameSession) return evalIn(expression, { sessionId: frameSession });
  const top = await mainFrameId();
  const tree = (await send("Page.getFrameTree")).result.frameTree;
  const child = tree.childFrames?.[0]?.frame;
  if (!child || child.id === top) throw new Error("activity frame not found");
  const contextId = contexts.get(child.id);
  if (!contextId) throw new Error("activity frame has no script context");
  return evalIn(expression, { contextId });
}

async function waitFor(what, fn, ms = 15_000) {
  const deadline = Date.now() + ms;
  let last;
  while (Date.now() < deadline) {
    try {
      const v = await fn();
      if (v) return v;
    } catch (e) {
      last = e;
    }
    await sleep(200);
  }
  throw new Error(`timed out waiting for ${what}${last ? `: ${last.message}` : ""}`);
}

const click = (selectorJs) =>
  main(`(() => { const el = ${selectorJs}; if (!el) return false; el.click(); return true; })()`);
const button = (text) =>
  `[...document.querySelectorAll('button')].find(b => b.textContent.includes(${JSON.stringify(text)}))`;
const snapshotTicks = () =>
  main(
    `window.__TAURI_INTERNALS__.invoke('activity_snapshot', { id: '${PROBE_ID}' }).then(s => s ? JSON.parse(s.snapshot).ticks : null)`,
  );

const report = [];
const check = (name, pass, detail = "") => {
  report.push({ name, pass, detail });
  console.log(`${pass ? "PASS" : "FAIL"}  ${name}${detail ? `  (${detail})` : ""}`);
};

await send("Runtime.enable");
await send("Page.enable");
await send("Target.setAutoAttach", {
  autoAttach: true,
  waitForDebuggerOnStart: false,
  flatten: true,
});
await send("Page.reload");
await waitFor("app UI", () => main("!!document.querySelector('.nav-item')"));

// Start fresh: no saved probe state. Plant a canary save for another activity, which the probe
// tries to delete through Tauri; keep any real save to put back afterwards.
const CANARY_ID = "app.lullshift.games.lantern-pond";
const invoke = (cmd, args) =>
  main(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);
await invoke("activity_clear_snapshot", { id: PROBE_ID });
const realSave = await invoke("activity_snapshot", { id: CANARY_ID });
await invoke("activity_save_snapshot", { id: CANARY_ID, snapshot: '{"canary":true}' });
await click(button("Activities"));
await waitFor("probe in the list", () => click(button("Sandbox probe")));
await waitFor("probe frame", () => main("!!document.querySelector('.host-frame')"));

// 1. Escape attempts from inside the sandbox.
const probe = await waitFor(
  "probe results",
  () => inFrame("window.probeDone && window.probeResults"),
  30_000,
);
const globals = await inFrame("window.probeGlobals");
console.log(
  `\nActivity security origin: ${probe.origin}; frame ${frameSession ? "out of process" : "in process"}; ` +
    `Tauri globals present: ${globals.length ? globals.join(", ") : "none"}`,
);
for (const r of probe.results) {
  if (KNOWN_GAPS.has(r.name) && !r.blocked) {
    console.log(`GAP   ${r.name}  (${r.detail}) - known, see ADR-012`);
    report.push({ name: r.name, pass: true, detail: `known gap: ${r.detail}`, gap: true });
  } else {
    check(`denied: ${r.name}`, r.name.startsWith("control") ? !r.blocked : r.blocked, r.detail);
  }
}
check("activity origin is opaque", probe.origin === "null", probe.origin);

// 2. A busy activity must not freeze the app.
void inFrame(
  "(() => { const t = Date.now(); while (Date.now() - t < 3000) {} return 1; })()",
).catch(() => undefined);
await sleep(300);
const t0 = Date.now();
await main("1 + 1");
const latency = Date.now() - t0;
check("app stays responsive while the activity is busy", latency < 1000, `${latency} ms`);
await sleep(3000);

// 3. Pause without cooperation, then restore (S03-A01).
await waitFor("ticks", async () => (await inFrame("window.probe.ticks()")) > 20);
await click(button("Pause"));
await waitFor("paused card", () =>
  main("!document.querySelector('.host-frame') && !!document.querySelector('.host-card')"),
);
const savedAtPause = await snapshotTicks();
check(
  "pause removes the activity frame and saves",
  typeof savedAtPause === "number" && savedAtPause > 20,
  `saved ticks ${savedAtPause}`,
);
await click(button("Resume"));
await waitFor("frame again", () => main("!!document.querySelector('.host-frame')"));
const restored = await waitFor("restore", () => inFrame("window.restoredTicks"));
check("resume restores the saved state", restored === savedAtPause, `restored ${restored}`);

await sleep(1500);
await inFrame("window.probe.beStubborn()");
const lastGood = await snapshotTicks();
await sleep(2000);
const ticksBefore = await inFrame("window.probe.ticks()");
const pauseStart = Date.now();
await click(button("Pause"));
await waitFor(
  "paused card (stubborn)",
  () => main("!document.querySelector('.host-frame') && !!document.querySelector('.host-card')"),
  5000,
);
const pauseMs = Date.now() - pauseStart;
const keptTicks = await snapshotTicks();
check("an activity that ignores save is still paused promptly", pauseMs < 2500, `${pauseMs} ms`);
check(
  "its last good save is kept",
  keptTicks === lastGood && keptTicks < ticksBefore,
  `kept ${keptTicks}, live was ${ticksBefore}`,
);

// 4. The frame can't navigate itself to a remote page.
await click(button("Resume"));
await waitFor("frame again", () => main("!!document.querySelector('.host-frame')"));
await waitFor("probe ready", () => inFrame("typeof window.probe === 'object'"));
await inFrame("location.href = 'https://example.com/'; true").catch(() => undefined);
await sleep(2500);
// The target list can keep the requested URL even when Chromium committed an error page in its
// place, so read what the frame actually holds.
const frameState = await (async () => {
  if (frameSession) {
    return evalIn(
      "JSON.stringify({ href: location.href, remote: document.body ? document.body.innerText.includes('Example Domain') : false })",
      { sessionId: frameSession },
    ).catch((e) => JSON.stringify({ error: e.message }));
  }
  const targets = (await send("Target.getTargets")).result.targetInfos;
  return JSON.stringify({ targets: targets.filter((t) => t.type === "iframe").map((t) => t.url) });
})();
const parsedFrame = JSON.parse(frameState);
const navigated =
  parsedFrame.remote === true ||
  (parsedFrame.href ?? "").startsWith("https://example.com") ||
  (parsedFrame.targets ?? []).some((u) => u.startsWith("https://example.com"));
check("activity can't navigate to a remote page", !navigated, frameState);

// 4b. Nothing the probe tried changed the app: the attention event it tried to emit never
// paused anything, and the canary save for another activity is untouched.
check(
  "probe's forged attention event had no effect",
  !(await main(
    "document.querySelector('.host-card')?.textContent?.includes('needs you') ?? false",
  )),
);
const canary = await invoke("activity_snapshot", { id: CANARY_ID });
check(
  "another activity's save is untouched",
  canary?.snapshot === '{"canary":true}',
  canary?.snapshot ?? "deleted",
);
if (realSave)
  await invoke("activity_save_snapshot", { id: CANARY_ID, snapshot: realSave.snapshot });
else await invoke("activity_clear_snapshot", { id: CANARY_ID });

// 5. Optional: a real "needs you" event pauses the activity (end to end).
if (withAttention) {
  await click(button("Close"));
  await waitFor("list", () => click(button("Sandbox probe")));
  await waitFor("probe running", () =>
    inFrame("typeof window.probe === 'object' && window.probe.ticks() > 5"),
  );
  const exe = join(process.cwd(), "target", "debug", "lullshift-desktop.exe");
  const dataDir = join(process.env.APPDATA ?? "", "app.lullshift.desktop");
  const session = `sandbox-test-${Date.now()}`;
  const hook = (event) =>
    spawnSync(exe, ["hook", "claude-code", "--data-dir", dataDir], {
      input: JSON.stringify({ session_id: session, cwd: "C:/", hook_event_name: event }),
    });
  hook("PermissionRequest");
  const paused = await waitFor(
    "attention pause",
    () => main("document.querySelector('.host-card')?.textContent ?? ''"),
    5000,
  ).catch(() => "");
  check(
    "a real needs-you event pauses the activity",
    /needs you/.test(paused),
    paused.slice(0, 80),
  );
  // "Keep playing" resumes and stays running: the same request must not pause the new launch.
  await click(button("Keep playing"));
  await waitFor("probe after keep playing", () =>
    inFrame("typeof window.probe === 'object'"),
  ).catch(() => undefined);
  await sleep(2000);
  check(
    "keep playing after an attention pause stays running",
    await main("!!document.querySelector('.host-frame') && !document.querySelector('.host-card')"),
  );
  hook("SessionEnd");
}

await click(button("Close"));
await main(`window.__TAURI_INTERNALS__.invoke('activity_clear_snapshot', { id: '${PROBE_ID}' })`);
ws.close();

const failed = report.filter((r) => !r.pass);
const gaps = report.filter((r) => r.gap);
console.log(
  `\n${report.length - failed.length}/${report.length} passed, ${gaps.length} known gap(s).`,
);
process.exit(failed.length ? 1 : 0);
