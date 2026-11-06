// `lullshift simulate` (ADR-017, PRD 4.6): runs an activity in the harness, inside a headless
// browser, through the interruption scenarios the host puts every activity through.

import { parseSummary } from "@lullshift/activity-sdk/host";
import { launchBrowser, type Browser } from "./browser.ts";
import { listFiles, printable } from "./files.ts";
import { startServers, type Servers } from "./serve.ts";
import type { Report } from "./validate.ts";

export type Level = "pass" | "warning" | "error";

export interface Outcome {
  scenario: string;
  level: Level;
  detail: string;
}

interface Exception {
  text: string;
  url: string;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** The files the harness serves for this activity: its own, or, for a content pack, its engine's. */
export function servedFiles(report: Report) {
  if (report.manifest?.kind === "content" && report.engine) {
    // The engine's listing is validated like any activity's before it is served. Like the app,
    // `images/` comes from the pack (only its listed, checked pictures), never from the engine.
    const listed = new Set(report.manifest.images ?? []);
    const files = [
      ...listFiles(report.engine.dir).files.filter((f) => !f.rel.startsWith("images/")),
      ...report.files.filter((f) => listed.has(f.rel)),
    ];
    return { files, content: report.data ?? null };
  }
  return { files: report.files, content: null };
}

export async function simulate(
  report: Report,
  browserPath: string,
  log: (line: string) => void = () => {},
): Promise<Outcome[]> {
  const m = report.manifest!;
  const { files, content } = servedFiles(report);
  const servers = await startServers(files, {
    mode: "simulate",
    name: m.name,
    storage: (m.permissions ?? []).includes("activity.storage"),
    content,
  });
  let browser: Browser | null = null;
  try {
    browser = await launchBrowser(browserPath);
    return await run(browser, servers, log);
  } finally {
    await browser?.close();
    await servers.close();
  }
}

async function run(
  browser: Browser,
  servers: Servers,
  log: (line: string) => void,
): Promise<Outcome[]> {
  const { cdp, page } = browser;
  const outcomes: Outcome[] = [];
  const exceptions: Exception[] = [];
  const outbound = new Set<string>();
  const frameSessions = new Set<string>();
  const local = (url: string) =>
    url.startsWith(servers.activityOrigin) ||
    url.startsWith(servers.harnessOrigin) ||
    url.startsWith("data:") ||
    url.startsWith("about:");

  cdp.on(({ method, params }) => {
    if (method === "Runtime.exceptionThrown") {
      const d = params.exceptionDetails as {
        text?: string;
        url?: string;
        exception?: { description?: string };
        stackTrace?: { callFrames?: { url: string }[] };
      };
      const url = d.url ?? d.stackTrace?.callFrames?.[0]?.url ?? "";
      exceptions.push({ text: d.exception?.description?.split("\n")[0] ?? d.text ?? "error", url });
    } else if (method === "Network.requestWillBeSent") {
      const url = (params.request as { url: string }).url;
      if (!local(url)) outbound.add(url);
    } else if (method === "Target.attachedToTarget") {
      // Out-of-process frames (the activity, when the browser isolates it) get their own session.
      const sid = params.sessionId as string;
      frameSessions.add(sid);
      void enable(sid);
    } else if (method === "Target.detachedFromTarget") {
      frameSessions.delete(params.sessionId as string);
    }
  });

  async function enable(sid: string) {
    await Promise.all([
      cdp.send("Runtime.enable", {}, sid),
      cdp.send("Network.enable", {}, sid),
      cdp.send(
        "Target.setAutoAttach",
        { autoAttach: true, waitForDebuggerOnStart: false, flatten: true },
        sid,
      ),
    ]).catch(() => {});
    await cdp.send("Runtime.runIfWaitingForDebugger", {}, sid).catch(() => {});
  }

  await enable(page);
  await cdp.send("Page.enable", {}, page);
  await cdp.send("Page.navigate", { url: `${servers.harnessOrigin}/` }, page);

  const evaluate = async <T>(expression: string): Promise<T> => {
    const r = await cdp.send<{
      result: { value?: T };
      exceptionDetails?: { text: string; exception?: { description?: string } };
    }>("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true }, page);
    if (r.exceptionDetails) {
      throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
    }
    return r.result.value as T;
  };
  for (let i = 0; i < 50; i++) {
    if (await evaluate<boolean>("typeof window.sim === 'object'").catch(() => false)) break;
    await sleep(100);
  }

  /** Counts focusable elements inside the activity frame, wherever the browser runs it. */
  const focusableInActivity = async (): Promise<number | null> => {
    const expr =
      "document.querySelectorAll('button:not([disabled]),a[href],input:not([disabled]),select,textarea,[tabindex]:not([tabindex=\"-1\"])').length";
    const { frameTree } = await cdp.send<{
      frameTree: { childFrames?: { frame: { id: string; url: string } }[] };
    }>("Page.getFrameTree", {}, page);
    const child = frameTree.childFrames?.find((c) =>
      c.frame.url.startsWith(servers.activityOrigin),
    );
    if (child) {
      try {
        const { executionContextId } = await cdp.send<{ executionContextId: number }>(
          "Page.createIsolatedWorld",
          { frameId: child.frame.id, worldName: "lullshift-sim" },
          page,
        );
        const r = await cdp.send<{ result: { value?: number } }>(
          "Runtime.evaluate",
          { expression: expr, contextId: executionContextId, returnByValue: true },
          page,
        );
        if (typeof r.result.value === "number") return r.result.value;
      } catch {
        // The frame is in another process; ask its own session below.
      }
    }
    for (const sid of frameSessions) {
      try {
        const r = await cdp.send<{ result: { value?: number } }>(
          "Runtime.evaluate",
          { expression: expr, returnByValue: true },
          sid,
        );
        if (typeof r.result.value === "number") return r.result.value;
      } catch {
        // not this one
      }
    }
    return null;
  };

  const out = (scenario: string, level: Level, detail: string) => {
    // Exception text and URLs come from the activity: keep escape sequences out of the terminal.
    detail = printable(detail);
    outcomes.push({ scenario, level, detail });
    log(`  ${level === "pass" ? "✓" : level === "warning" ? "!" : "✗"} ${scenario}: ${detail}`);
  };
  const exceptionCount = () =>
    exceptions.filter((e) => !e.url.startsWith(servers.harnessOrigin)).length;
  type Launch = { ok: boolean; readyMs: number };
  type Saved = { ok: boolean; state: unknown };
  const launch = (opts = {}) => evaluate<Launch>(`sim.launch(${JSON.stringify(opts)})`);
  const save = () => evaluate<Saved>("sim.save()");
  const settle = () => sleep(400);

  // 1. Fresh start.
  {
    const r = await launch();
    if (r.ok) out("fresh start", "pass", `ready in ${r.readyMs} ms`);
    else {
      out("fresh start", "error", "never said ready within 8 s (call LullshiftBridge.start)");
      return finish();
    }
    await settle();
  }

  // 2. Keyboard: something to focus.
  {
    const n = await focusableInActivity();
    if (n === null) out("keyboard", "warning", "couldn't look inside the activity frame");
    else if (n === 0) out("keyboard", "error", "nothing in the activity can take keyboard focus");
    else out("keyboard", "pass", `${n} focusable element(s)`);
  }

  // 3. Save and restore round trip.
  {
    const s1 = await save();
    if (!s1.ok) {
      out(
        "save",
        "error",
        "didn't answer a save request within 800 ms with a JSON state (not null, at most 256 KiB)",
      );
    } else {
      out("save", "pass", `state of ${JSON.stringify(s1.state).length} bytes`);
      await evaluate("sim.freeze('user')");
      const again = await launch();
      await settle();
      const s2 = await save();
      if (!again.ok || !s2.ok) {
        out("restore", "error", "didn't start again from its saved state");
      } else {
        const diff = diffPaths(s1.state, s2.state);
        if (diff.structural.length) {
          out(
            "restore",
            "error",
            `restoring changed the state's shape at ${diff.structural.slice(0, 5).join(", ")}`,
          );
        } else if (diff.values.length) {
          out(
            "restore",
            "warning",
            `restored state differs at ${diff.values.slice(0, 5).join(", ")}; fine for timers, a bug otherwise`,
          );
        } else {
          out("restore", "pass", "save → restore → save gives the same state");
        }
      }
    }
  }

  // 4. Agent events that must not pause.
  {
    await evaluate("sim.resetAgents()");
    const before = await evaluate<number>("sim.pauses()");
    const t = Date.now();
    for (const [i, type] of ["working", "completed", "failed", "cancelled", "working"].entries()) {
      await evaluate(
        `sim.agent(${JSON.stringify({ id: `quiet-${i}`, session: "a", type, ts: t + i })})`,
      );
    }
    const s = await save();
    const paused = (await evaluate<number>("sim.pauses()")) - before;
    if (paused === 0 && s.ok) out("working, completed, failed, cancelled", "pass", "kept running");
    else
      out(
        "working, completed, failed, cancelled",
        "error",
        "stopped answering while agents only worked",
      );
  }

  // 5. needs_attention, a duplicate, and an out-of-order event: one pause, nothing lost.
  {
    const before = await evaluate<number>("sim.pauses()");
    const t = Date.now() + 1000;
    const attention = { id: "need-1", session: "a", type: "needs_attention", ts: t };
    const first = await evaluate<{ paused: boolean }>(`sim.agent(${JSON.stringify(attention)})`);
    await evaluate(`sim.agent(${JSON.stringify(attention)})`);
    await evaluate(
      `sim.agent(${JSON.stringify({ id: "old-1", session: "a", type: "working", ts: t - 500 })})`,
    );
    await evaluate(
      `sim.agent(${JSON.stringify({ id: "need-2", session: "a", type: "needs_attention", ts: t - 400 })})`,
    );
    const pauses = (await evaluate<number>("sim.pauses()")) - before;
    const phase = await evaluate<string>("sim.phase()");
    if (first.paused && pauses === 1 && phase === "paused") {
      out("needs_attention, duplicate, out of order", "pass", "paused once");
    } else {
      out(
        "needs_attention, duplicate, out of order",
        "error",
        `paused ${pauses} time(s), host phase ${phase}`,
      );
    }
    const resumed = await launch();
    await settle();
    const s = await save();
    out(
      "resume after attention",
      resumed.ok && s.ok ? "pass" : "error",
      resumed.ok && s.ok ? "resumed from its save" : "didn't resume",
    );
  }

  // 6. needs_attention before the activity is ready.
  {
    // Record the saved state after the closing save, so only the early launch could change it.
    await evaluate("sim.freeze('user')");
    const kept = JSON.stringify(await evaluate("sim.snapshot()"));
    await evaluate("sim.resetAgents()");
    void evaluate("sim.launch({ attentionBeforeReady: true })").catch(() => {});
    await sleep(600);
    const phase = await evaluate<string>("sim.phase()");
    const after = JSON.stringify(await evaluate("sim.snapshot()"));
    if (phase === "paused" && after === kept) {
      out("needs_attention before ready", "pass", "unloaded at once; saved state untouched");
    } else {
      out(
        "needs_attention before ready",
        "error",
        `host phase ${phase}; saved state ${after === kept ? "kept" : "changed"}`,
      );
    }
    const r = await launch();
    await settle();
    out("start after an early pause", r.ok ? "pass" : "error", r.ok ? "started" : "didn't start");
  }

  // 7. Close and reopen, then crash and recover.
  {
    const closed = await evaluate<{ saved: boolean; ms: number }>("sim.freeze('user')");
    out(
      "close",
      closed.saved ? "pass" : "warning",
      closed.saved
        ? `saved and unloaded in ${closed.ms} ms`
        : "unloaded without answering the final save; the last save is kept",
    );
    const reopened = await launch();
    await settle();
    await evaluate("sim.crash()");
    const recovered = await launch();
    await settle();
    const s = await save();
    if (reopened.ok && recovered.ok && s.ok)
      out("crash and recover", "pass", "started again from the last save");
    else out("crash and recover", "error", "didn't start again after a crash");
  }

  // 8. Reduced motion and dark theme from the start.
  {
    await evaluate("sim.freeze('user')");
    const before = exceptionCount();
    const r = await launch({ reducedMotion: true, theme: "dark" });
    await settle();
    out(
      "reduced motion, dark theme",
      r.ok && exceptionCount() === before ? "pass" : "error",
      r.ok ? (exceptionCount() === before ? "started cleanly" : "threw errors") : "didn't start",
    );
    await evaluate("sim.freeze('user')");
  }

  return finish();

  async function finish(): Promise<Outcome[]> {
    // Summary messages: what the paused view would show.
    const summary = await evaluate<unknown>("sim.summary()").catch(() => null);
    const dropped = await evaluate<Record<string, number>>("sim.dropped()").catch(
      (): Record<string, number> => ({}),
    );
    if (summary) out("paused view stats", "pass", JSON.stringify(summary));
    else if ((dropped.field ?? 0) > 0) {
      out(
        "paused view stats",
        "warning",
        "some messages were refused; check LullshiftBridge.summary items (1–4, ≤ 24 characters)",
      );
    } else {
      out(
        "paused view stats",
        "warning",
        "no summary sent; the paused view will show time played and the preview only",
      );
    }
    if (summary && !parseSummary(summary))
      out("paused view stats", "error", "summary failed validation");
    const droppedTotal = Object.values(dropped).reduce((a, b) => a + b, 0);
    if (droppedTotal)
      out(
        "bridge messages",
        "warning",
        `${droppedTotal} message(s) refused: ${JSON.stringify(dropped)}`,
      );
    const errs = exceptions.filter((e) => !e.url.startsWith(servers.harnessOrigin));
    if (errs.length) {
      const sample = [...new Set(errs.map((e) => e.text))].slice(0, 3).join(" | ");
      out("errors", "error", `${errs.length} uncaught error(s): ${sample}`);
    } else {
      out("errors", "pass", "no uncaught errors");
    }
    if (outbound.size) {
      out(
        "offline",
        "error",
        `tried to reach the network (blocked): ${[...outbound].slice(0, 3).join(", ")}`,
      );
    } else {
      out("offline", "pass", "no network requests");
    }
    return outcomes;
  }
}

/** Paths where two JSON values differ: in shape (keys, types, lengths) or only in values. */
export function diffPaths(
  a: unknown,
  b: unknown,
  at = "$",
): { structural: string[]; values: string[] } {
  const structural: string[] = [];
  const values: string[] = [];
  const kind = (v: unknown) => (v === null ? "null" : Array.isArray(v) ? "array" : typeof v);
  const walk = (x: unknown, y: unknown, p: string) => {
    if (kind(x) !== kind(y)) return structural.push(p);
    if (Array.isArray(x) && Array.isArray(y)) {
      if (x.length !== y.length) return structural.push(`${p}.length`);
      x.forEach((v, i) => walk(v, y[i], `${p}[${i}]`));
      return;
    }
    if (kind(x) === "object") {
      const xo = x as Record<string, unknown>;
      const yo = y as Record<string, unknown>;
      const keys = new Set([...Object.keys(xo), ...Object.keys(yo)]);
      for (const k of keys) {
        if (!(k in xo) || !(k in yo)) structural.push(`${p}.${k}`);
        else walk(xo[k], yo[k], `${p}.${k}`);
      }
      return;
    }
    if (x !== y) values.push(p);
  };
  walk(a, b, at);
  return { structural, values };
}
