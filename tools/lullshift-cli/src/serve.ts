// Local servers for `dev` and `simulate` (ADR-017). Two origins on 127.0.0.1:
// - the harness (a host emulator page) on one port;
// - the activity on another, with the same headers the app sends (ADR-012), so the activity
//   runs in the same sandbox: opaque origin, no network, no inline script.
// Both listen on loopback only, on ports the OS picks. Nothing is fetched from the network.

import { readFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { stripTypeScriptTypes } from "node:module";
import type { AddressInfo } from "node:net";
import path from "node:path";
import { readListed, SERVED_TYPES, type FileEntry } from "./files.ts";
import { HARNESS_DIR, HOST_PROTOCOL_PATH } from "./paths.ts";

/**
 * The activity CSP, identical to `activity_csp()` in `apps/desktop/src-tauri/src/activities.rs`
 * except for `frame-ancestors`. A test compares the two.
 */
export function activityCsp(frameAncestors: string): string {
  return (
    "sandbox allow-scripts; default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; " +
    "font-src 'self'; media-src 'self'; connect-src 'none'; frame-src 'none'; " +
    "child-src 'none'; worker-src 'none'; object-src 'none'; manifest-src 'none'; " +
    `base-uri 'none'; form-action 'none'; frame-ancestors ${frameAncestors}`
  );
}

const PERMISSIONS_POLICY =
  "camera=(), microphone=(), geolocation=(), usb=(), serial=(), hid=(), " +
  "bluetooth=(), payment=(), clipboard-read=(), clipboard-write=(), " +
  "display-capture=(), screen-wake-lock=(), publickey-credentials-get=()";

export interface Servers {
  harnessOrigin: string;
  activityOrigin: string;
  close(): Promise<void>;
}

const listen = (server: Server) =>
  new Promise<number>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve((server.address() as AddressInfo).port));
  });

/**
 * Serves `files` (the validated listing, nothing else) as the activity, and the harness page.
 * `config` is handed to the harness as JSON at `/config.json`.
 */
export async function startServers(files: FileEntry[], config: object): Promise<Servers> {
  const byPath = new Map(files.map((f) => [f.rel, f]));
  let harnessOrigin = "";

  let activityPort = 0;
  let harnessPort = 0;
  const activity = createServer((req, res) => {
    const url = new URL(req.url ?? "/", "http://x");
    let rel: string;
    try {
      rel = decodeURIComponent(url.pathname).replace(/^\//, "");
    } catch {
      rel = "";
    }
    // Only our own loopback address: a page on another site can't reach us by DNS rebinding.
    const ownHost = req.headers.host === `127.0.0.1:${activityPort}`;
    const f = req.method === "GET" && ownHost ? byPath.get(rel) : undefined;
    const type = f ? SERVED_TYPES[f.ext] : undefined;
    const headers = {
      "Content-Security-Policy": activityCsp(harnessOrigin),
      "X-Content-Type-Options": "nosniff",
      "Referrer-Policy": "no-referrer",
      "Cache-Control": "no-store",
      "Permissions-Policy": PERMISSIONS_POLICY,
      "Cross-Origin-Opener-Policy": "same-origin",
    };
    if (!f || !type) {
      res.writeHead(404, { ...headers, "Content-Type": "text/plain; charset=utf-8" });
      res.end("Not found");
      return;
    }
    let body: Buffer;
    try {
      body = readListed(f);
    } catch (e) {
      res.writeHead(404, { ...headers, "Content-Type": "text/plain; charset=utf-8" });
      res.end(`Not served: ${(e as Error).message}`);
      return;
    }
    res.writeHead(200, { ...headers, "Content-Type": type });
    res.end(body);
  });

  const harnessCsp =
    "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; " +
    "connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";
  let activityOrigin = "";
  const harness = createServer((req, res) => {
    const url = new URL(req.url ?? "/", "http://x");
    const send = (status: number, type: string, body: string | Buffer) => {
      res.writeHead(status, {
        "Content-Type": type,
        "Cache-Control": "no-store",
        "Content-Security-Policy": `${harnessCsp}; frame-src ${activityOrigin}`,
        "X-Content-Type-Options": "nosniff",
      });
      res.end(body);
    };
    if (req.method !== "GET") return send(405, "text/plain", "Method not allowed");
    if (req.headers.host !== `127.0.0.1:${harnessPort}`) {
      return send(403, "text/plain", "Wrong host");
    }
    switch (url.pathname) {
      case "/":
        return send(
          200,
          "text/html; charset=utf-8",
          readFileSync(path.join(HARNESS_DIR, "index.html")),
        );
      case "/harness.css":
        return send(
          200,
          "text/css; charset=utf-8",
          readFileSync(path.join(HARNESS_DIR, "harness.css")),
        );
      case "/host.js":
        return send(
          200,
          "text/javascript; charset=utf-8",
          tsToJs(path.join(HARNESS_DIR, "host.ts")),
        );
      case "/protocol.js":
        return send(200, "text/javascript; charset=utf-8", tsToJs(HOST_PROTOCOL_PATH));
      case "/config.json":
        return send(200, "application/json", JSON.stringify({ ...config, activityOrigin }));
      default:
        return send(404, "text/plain", "Not found");
    }
  });

  const [hp, ap] = await Promise.all([listen(harness), listen(activity)]);
  harnessPort = hp;
  activityPort = ap;
  harnessOrigin = `http://127.0.0.1:${hp}`;
  activityOrigin = `http://127.0.0.1:${ap}`;
  return {
    harnessOrigin,
    activityOrigin,
    close: () =>
      Promise.all(
        [harness, activity].map((s) => new Promise<void>((r) => s.close(() => r()))),
      ).then(() => undefined),
  };
}

/** TypeScript to browser JavaScript: strip types, and point the SDK's host protocol import at `/protocol.js`. */
function tsToJs(file: string): string {
  // Node flags stripTypeScriptTypes as experimental; the output is checked by the simulator's own
  // tests, so the warning is noise for creators.
  const warn = process.emitWarning;
  process.emitWarning = () => {};
  let js: string;
  try {
    js = stripTypeScriptTypes(readFileSync(file, "utf8"), { mode: "strip" });
  } finally {
    process.emitWarning = warn;
  }
  return js.replaceAll('from "@lullshift/activity-sdk/host"', 'from "/protocol.js"');
}
