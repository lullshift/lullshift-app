// A Chromium-family browser already on the machine, driven over the Chrome DevTools Protocol
// (ADR-017). Nothing is downloaded. The profile is temporary and deleted afterwards.

import { spawn, type ChildProcess } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

/** Standard install locations, most likely first. */
function candidates(): string[] {
  const env = process.env;
  if (process.platform === "win32") {
    const roots = [env["PROGRAMFILES(X86)"], env.PROGRAMFILES, env.LOCALAPPDATA].filter(
      (r): r is string => !!r,
    );
    return roots.flatMap((r) => [
      path.join(r, "Microsoft/Edge/Application/msedge.exe"),
      path.join(r, "Google/Chrome/Application/chrome.exe"),
      path.join(r, "Chromium/Application/chrome.exe"),
    ]);
  }
  if (process.platform === "darwin") {
    return [
      "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
      "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
      "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ];
  }
  const dirs = (env.PATH ?? "").split(path.delimiter).filter(Boolean);
  const names = [
    "google-chrome",
    "google-chrome-stable",
    "chromium",
    "chromium-browser",
    "microsoft-edge",
  ];
  return names.flatMap((n) => dirs.map((d) => path.join(d, n)));
}

/** The browser to use: `--browser`, then `LULLSHIFT_BROWSER`, then the usual places. */
export function findBrowser(explicit?: string): string | null {
  const chosen = explicit ?? process.env.LULLSHIFT_BROWSER;
  if (chosen) return existsSync(chosen) ? chosen : null;
  return candidates().find((c) => existsSync(c)) ?? null;
}

type Listener = (msg: {
  method: string;
  params: Record<string, unknown>;
  sessionId?: string;
}) => void;

export interface Cdp {
  send<T = Record<string, unknown>>(
    method: string,
    params?: object,
    sessionId?: string,
  ): Promise<T>;
  on(listener: Listener): void;
  close(): void;
}

export interface Browser {
  cdp: Cdp;
  /** The page target's session. */
  page: string;
  close(): Promise<void>;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Launches `exe` (headless unless `visible`) and attaches to its first page. */
export async function launchBrowser(
  exe: string,
  opts: { visible?: boolean } = {},
): Promise<Browser> {
  const profile = mkdtempSync(path.join(tmpdir(), "lullshift-sim-"));
  const args = [
    ...(opts.visible ? [] : ["--headless=new"]),
    "--remote-debugging-port=0",
    `--user-data-dir=${profile}`,
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-extensions",
    "--disable-sync",
    "--disable-background-networking",
    "--disable-component-update",
    "--disable-default-apps",
    "--metrics-recording-only",
    "--no-pings",
    "--window-size=1100,800",
    "about:blank",
  ];
  const child: ChildProcess = spawn(exe, args, { stdio: "ignore" });
  const portFile = path.join(profile, "DevToolsActivePort");
  let wsPath = "";
  // Up to 30 s: a cold Edge start under load (the full gate) can take more than 10 s.
  for (let i = 0; i < 300 && !wsPath; i++) {
    await sleep(100);
    if (existsSync(portFile)) {
      let text: string;
      try {
        text = readFileSync(portFile, "utf8");
      } catch {
        // On Windows the browser may still hold the file open (EBUSY): try again shortly.
        continue;
      }
      const [port, p] = text.split("\n");
      if (port && p) wsPath = `ws://127.0.0.1:${port}${p}`;
    }
  }
  const exited = new Promise<void>((resolve) => child.once("exit", () => resolve()));
  const cleanup = async () => {
    child.kill();
    // The browser and its helper processes hold files in the profile until they have exited.
    await Promise.race([exited, sleep(5000)]);
    await sleep(500);
    // Windows can keep a just-closed file locked for a few seconds (helper processes, scanners).
    for (let attempt = 0; ; attempt++) {
      try {
        rmSync(profile, { recursive: true, force: true });
        return;
      } catch (e) {
        if (attempt >= 30) {
          console.error(
            `Couldn't remove the temporary browser profile ${profile} (${(e as Error).message}); delete it by hand.`,
          );
          return;
        }
        await sleep(500);
      }
    }
  };
  if (!wsPath) {
    await cleanup();
    throw new Error(`${path.basename(exe)} didn't start its debugging port`);
  }

  // From here on any failure must still stop the browser: a headless Chromium left running keeps
  // an unauthenticated debugging port open on loopback.
  let socket: WebSocket | undefined;
  try {
    const ws = new WebSocket(wsPath);
    socket = ws;
    await new Promise((resolve, reject) => {
      ws.addEventListener("open", resolve, { once: true });
      ws.addEventListener("error", reject, { once: true });
    });
    let id = 0;
    const pending = new Map<
      number,
      { resolve: (v: unknown) => void; reject: (e: Error) => void }
    >();
    const listeners: Listener[] = [];
    ws.addEventListener("message", (e) => {
      const m = JSON.parse(String(e.data)) as {
        id?: number;
        result?: unknown;
        error?: { message: string };
        method?: string;
        params?: Record<string, unknown>;
        sessionId?: string;
      };
      if (m.id !== undefined) {
        const p = pending.get(m.id);
        pending.delete(m.id);
        if (m.error) p?.reject(new Error(m.error.message));
        else p?.resolve(m.result);
      } else if (m.method) {
        for (const l of listeners)
          l({ method: m.method, params: m.params ?? {}, sessionId: m.sessionId });
      }
    });
    const cdp: Cdp = {
      send: (method, params = {}, sessionId) =>
        new Promise((resolve, reject) => {
          id += 1;
          pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
          ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
        }),
      on: (l) => listeners.push(l),
      close: () => ws.close(),
    };
    const { targetInfos } = await cdp.send<{ targetInfos: { targetId: string; type: string }[] }>(
      "Target.getTargets",
    );
    const pageTarget = targetInfos.find((t) => t.type === "page");
    if (!pageTarget) throw new Error("the browser opened no page");
    const { sessionId } = await cdp.send<{ sessionId: string }>("Target.attachToTarget", {
      targetId: pageTarget.targetId,
      flatten: true,
    });
    return {
      cdp,
      page: sessionId,
      close: async () => {
        // Ask the browser to quit (it then ends its helper processes too) before killing it.
        await Promise.race([cdp.send("Browser.close").catch(() => {}), sleep(2000)]);
        cdp.close();
        await cleanup();
      },
    };
  } catch (e) {
    socket?.close();
    await cleanup();
    throw e;
  }
}
