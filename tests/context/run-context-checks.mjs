// Context Keeper checks (Sprint 04, ADR-013), run against the real app with a synthetic
// multi-session fixture. It sends Claude Code hook events through the hook forwarder for four
// test sessions named fixture-*-<time>, checks capture with and without consent, the return
// card, keyboard opening of the detail panel, then deletes the sessions and restores your
// context and panel settings. Back up your app data folder first if you want to be sure.
//
// Usage (Windows, from the repo root, with a debug build running on the WebView2 debug port,
// as in tests/sandbox/run-denial-tests.mjs):
//   node tests/context/run-context-checks.mjs [screenshot-dir]
import { mkdirSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { join } from "node:path";

const PORT = 9333;

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

const OUT = process.argv[2];
if (OUT) mkdirSync(OUT, { recursive: true });
const results = [];
const ok = (name, pass, detail = "") => {
  results.push(pass);
  console.log(
    `${pass ? "PASS" : "FAIL"}  ${name}${detail ? "  (" + String(detail).slice(0, 160) + ")" : ""}`,
  );
};
const errors = [];
listeners.push((m) => {
  if (m.method === "Runtime.exceptionThrown")
    errors.push(m.params.exceptionDetails.exception?.description?.slice(0, 200));
  if (m.method === "Runtime.consoleAPICalled" && m.params.type === "error")
    errors.push(JSON.stringify(m.params.args.map((a) => a.value ?? a.description)).slice(0, 200));
});
const shot = async (name) => {
  if (!OUT) return;
  const r = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(`${OUT}/${name}.png`, Buffer.from(r.result.data, "base64"));
};
const key = async (k, code, vk) => {
  for (const type of ["keyDown", "keyUp"])
    await send("Input.dispatchKeyEvent", {
      type,
      key: k,
      code,
      windowsVirtualKeyCode: vk,
      ...(type === "keyDown" && k === "Enter" ? { text: "\r" } : {}),
    });
  await sleep(150);
};
const inv = (cmd, args = {}) =>
  main(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);
const exe = join(process.cwd(), "target", "debug", "lullshift-desktop.exe");
const dataDir = join(process.env.APPDATA ?? "", "app.lullshift.desktop");
const hook = (session, event, extra = {}) =>
  spawnSync(exe, ["hook", "claude-code", "--data-dir", dataDir], {
    input: JSON.stringify({
      session_id: session,
      cwd: "C:/secret/repo",
      transcript_path: "C:/secret/t.jsonl",
      hook_event_name: event,
      ...extra,
    }),
  });
const T = Date.now();
const A = `fixture-a-${T}`,
  B = `fixture-b-${T}`,
  C = `fixture-c-${T}`,
  D = `fixture-d-${T}`;
const FAKE = "sk-test" + "1234567890abcdefghijklmnopqrstuv";
const PROMPT_A = `Add retention to the event log; my key is ${FAKE}`;
const panelText = () =>
  main(
    "(document.querySelector('[role=complementary]') ?? document.querySelector('[class*=panel]'))?.innerText ?? ''",
  );
const sel = (id) => `document.querySelector('[data-session-key*="${id}"]')`;

await send("Runtime.enable");
await send("Page.enable");
await send("Page.reload");
await waitFor("app UI", () => main("!!document.querySelector('.nav-item')"));
const prevPrefs = await inv("context_prefs");
const prevUi = await inv("ui_prefs");
console.log("saved prefs", JSON.stringify(prevPrefs), JSON.stringify(prevUi));
try {
  await inv("set_context_prefs", {
    prefs: { retentionDays: prevPrefs.retentionDays, capture: { "claude-code": true } },
  });
  await inv("set_ui_prefs", { prefs: { detailPanelPinned: true } });
  hook(A, "SessionStart", { source: "startup" });
  hook(A, "UserPromptSubmit", { prompt: PROMPT_A });
  hook(A, "Notification", {
    notification_type: "permission_prompt",
    message: "Claude needs your permission to use Bash",
  });
  hook(B, "SessionStart", { source: "startup" });
  hook(B, "UserPromptSubmit", { prompt: "Refactor the notification gate" });
  hook(B, "PostToolUse", { tool_name: "Edit", tool_input: { file_path: "C:/secret/x.rs" } });
  hook(C, "SessionStart", { source: "startup" });
  hook(C, "UserPromptSubmit", { prompt: "Wire up export for sessions" });
  hook(C, "Stop");
  hook(C, "SessionEnd", { reason: "prompt_input_exit" });
  await sleep(800);
  const dA = await inv("session_detail", { provider: "claude-code", sessionId: A });
  ok(
    "captured intent with secret hidden",
    dA.context.intent?.source === "agent" &&
      dA.context.intent.text.includes("[hidden]") &&
      !dA.context.intent.text.includes(FAKE),
    dA.context.intent?.text,
  );
  ok(
    "captured ask",
    dA.context.ask?.text === "Claude needs your permission to use Bash",
    dA.context.ask?.text,
  );
  ok("timeline has events", dA.timeline.length >= 3, dA.timeline.map((t) => t.eventType).join(","));
  ok("timeline holds no task text", !/retention|Bash|secret/.test(JSON.stringify(dA.timeline)));
  const dash = await inv("dashboard");
  const sc = dash.sessions.find((s) => s.providerSessionId === C);
  ok(
    "ended session with captured intent needs review",
    sc?.needsReview === true,
    JSON.stringify(sc),
  );
  const sb = dash.sessions.find((s) => s.providerSessionId === B);
  ok(
    "working session doesn't need review",
    sb?.needsReview === false && sb?.state === "working",
    JSON.stringify(sb),
  );

  await inv("set_context_prefs", {
    prefs: { retentionDays: prevPrefs.retentionDays, capture: { "claude-code": false } },
  });
  hook(D, "SessionStart", { source: "startup" });
  hook(D, "UserPromptSubmit", { prompt: "SHOULD NOT BE KEPT" });
  await sleep(500);
  const dD = await inv("session_detail", { provider: "claude-code", sessionId: D });
  ok(
    "capture off keeps nothing",
    dD.context.intent === null && dD.context.ask === null && !dD.context.hasCaptured,
    JSON.stringify(dD.context),
  );

  // UI at the default 900 px window: open by keyboard, Esc returns focus.
  await click(button("Sessions"));
  await sleep(700);
  ok(
    "card is focusable",
    await main(
      `(() => { const el = ${sel(A)}; if (!el) return false; el.focus(); return document.activeElement === el; })()`,
    ),
  );
  await key("Enter", "Enter", 13);
  await sleep(700);
  let text = await panelText();
  ok(
    "Enter opens the needs-you panel",
    /needs you/i.test(text) && text.includes("permission to use Bash"),
    text.slice(0, 160),
  );
  ok("panel shows no raw secret", !text.includes(FAKE));
  await shot("1-needs-you-900-pinned");
  await key("Escape", "Escape", 27);
  await sleep(500);
  ok(
    "Esc returns focus to the card",
    await main(`document.activeElement === ${sel(A)}`),
    await main("document.activeElement?.outerHTML?.slice(0,120)"),
  );

  await main(`${sel(B)}.click(); true`);
  await sleep(700);
  await shot("2-working-900-pinned");
  console.log("working panel:", (await panelText()).slice(0, 400).replace(/\n+/g, " | "));

  await click(
    `[...document.querySelectorAll('[role=tab],button')].find(b => /^History/.test(b.textContent.trim()))`,
  );
  await sleep(600);
  await main(`(${sel(C)})?.click(); true`);
  await sleep(700);
  text = await panelText();
  ok(
    "return card shown for ended session",
    /Welcome back/.test(text) && /know whether the task is done/.test(text),
    text.slice(0, 200),
  );
  await shot("3-return-card-900");
  await click(button("Done reviewing"));
  await sleep(800);
  const dash2 = await inv("dashboard");
  ok(
    "Done reviewing clears needsReview",
    dash2.sessions.find((s) => s.providerSessionId === C)?.needsReview === false,
  );

  await send("Emulation.setDeviceMetricsOverride", {
    width: 1300,
    height: 760,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await click(
    `[...document.querySelectorAll('[role=tab],button')].find(b => /^Live/.test(b.textContent.trim()))`,
  );
  await main(`(${sel(A)})?.click(); true`);
  await sleep(700);
  await shot("4-pinned-1300");
  await send("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-color-scheme", value: "dark" }],
  });
  await sleep(300);
  await shot("5-pinned-1300-dark");
  await send("Emulation.setEmulatedMedia", { features: [] });
  await inv("set_ui_prefs", { prefs: { detailPanelPinned: false } });
  await send("Page.reload");
  await waitFor("app UI", () => main("!!document.querySelector('.nav-item')"));
  await click(button("Sessions"));
  await sleep(600);
  await main(`(${sel(A)})?.click(); true`);
  await sleep(700);
  await shot("6-floating-1300");
  await send("Emulation.clearDeviceMetricsOverride");
} finally {
  for (const s of [A, B, C, D])
    await inv("delete_session", { provider: "claude-code", sessionId: s }).catch((e) =>
      console.log("delete failed", s, e.message),
    );
  const left = (await inv("dashboard")).sessions.filter((s) =>
    s.providerSessionId.startsWith("fixture-"),
  );
  ok("fixture sessions deleted", left.length === 0, left.length);
  await inv("set_context_prefs", { prefs: prevPrefs });
  await inv("set_ui_prefs", { prefs: prevUi });
  console.log(
    "restored prefs",
    JSON.stringify(await inv("context_prefs")),
    JSON.stringify(await inv("ui_prefs")),
  );
}
console.log("errors:", errors.length ? errors : "none");
console.log(`${results.filter(Boolean).length}/${results.length} passed`);
process.exit(results.every(Boolean) ? 0 : 1);
