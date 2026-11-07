// Captures website screenshots of the browser demo (demo.html) with made-up data.
//
//   pnpm --filter @lullshift/desktop dev            (in another terminal)
//   node apps/desktop/scripts/demo-screenshots.mjs [out dir] [--base http://localhost:1420]
//
// Drives a Chromium-family browser already on the machine (Edge or Chrome, or LULLSHIFT_BROWSER)
// over the DevTools Protocol, headless, with a temporary profile. Writes <shot>-<light|dark>.png
// at 1280x800 CSS pixels and 2x density. Nothing is downloaded and no real data is involved.

import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

const args = process.argv.slice(2);
const baseIdx = args.indexOf("--base");
const BASE = baseIdx >= 0 ? args.splice(baseIdx, 2)[1] : "http://localhost:1420";
const OUT = path.resolve(args[0] ?? "demo-screenshots");
const WIDTH = 1280;
const HEIGHT = 800;

function findBrowser() {
  if (process.env.LULLSHIFT_BROWSER) return process.env.LULLSHIFT_BROWSER;
  const env = process.env;
  const list =
    process.platform === "win32"
      ? [env["PROGRAMFILES(X86)"], env.PROGRAMFILES, env.LOCALAPPDATA]
          .filter(Boolean)
          .flatMap((r) => [
            path.join(r, "Microsoft/Edge/Application/msedge.exe"),
            path.join(r, "Google/Chrome/Application/chrome.exe"),
          ])
      : process.platform === "darwin"
        ? [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
          ]
        : (env.PATH ?? "")
            .split(path.delimiter)
            .flatMap((d) =>
              ["google-chrome", "chromium", "microsoft-edge"].map((n) => path.join(d, n)),
            );
  const found = list.find((p) => existsSync(p));
  if (!found) throw new Error("No Edge or Chrome found. Set LULLSHIFT_BROWSER to its path.");
  return found;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function launch() {
  const profile = mkdtempSync(path.join(tmpdir(), "lullshift-shots-"));
  const child = spawn(
    findBrowser(),
    [
      "--headless=new",
      "--remote-debugging-port=0",
      `--user-data-dir=${profile}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-extensions",
      "--hide-scrollbars",
      "--font-render-hinting=none",
      `--window-size=${WIDTH},${HEIGHT}`,
      "about:blank",
    ],
    { stdio: "ignore" },
  );
  const portFile = path.join(profile, "DevToolsActivePort");
  let ws = "";
  for (let i = 0; i < 300 && !ws; i++) {
    await sleep(100);
    if (existsSync(portFile)) {
      const [port, p] = readFileSync(portFile, "utf8").split("\n");
      if (port && p) ws = `ws://127.0.0.1:${port}${p}`;
    }
  }
  if (!ws) throw new Error("The browser didn't open its DevTools port.");
  const socket = new WebSocket(ws);
  await new Promise((res, rej) => {
    socket.onopen = res;
    socket.onerror = rej;
  });
  let id = 0;
  const waiting = new Map();
  socket.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id && waiting.has(m.id)) {
      const { resolve, reject } = waiting.get(m.id);
      waiting.delete(m.id);
      if (m.error) reject(new Error(m.error.message));
      else resolve(m.result);
    }
  };
  const send = (method, params = {}, sessionId) =>
    new Promise((resolve, reject) => {
      const msg = { id: ++id, method, params };
      if (sessionId) msg.sessionId = sessionId;
      waiting.set(msg.id, { resolve, reject });
      socket.send(JSON.stringify(msg));
    });
  const close = async () => {
    socket.close();
    child.kill();
    await sleep(800);
    try {
      rmSync(profile, { recursive: true, force: true, maxRetries: 5 });
    } catch {
      // The browser can hold the temporary profile a little longer on Windows; it is in the temp folder.
    }
  };
  return { send, close };
}

// Runs in the page: small helpers the shots use.
const HELPERS = `
  window.__wait = (ms) => new Promise((r) => setTimeout(r, ms));
  window.__until = async (fn, ms = 8000) => {
    const end = Date.now() + ms;
    while (Date.now() < end) { const v = fn(); if (v) return v; await __wait(100); }
    throw new Error("timed out waiting");
  };
  window.__clickText = async (selector, text) => {
    const el = await __until(() =>
      [...document.querySelectorAll(selector)]
        .filter((e) => e.textContent.trim().includes(text))
        .sort((a, b) => a.textContent.length - b.textContent.length)[0]);
    el.click();
    await __wait(400);
  };
  window.__nav = (label) => __clickText(".nav-item", label);
`;

/** Fog Garden tiles to reveal, in CSS pixels at 1280x800 (the first reveal is always safe). */
const TILE_CLICKS = [
  [809, 429],
  [677, 281],
  [941, 577],
];

/** Each shot: `steps` run in the page after load, then `keys` and `clicks` (CSS pixels), then `after`. */
const SHOTS = [
  {
    name: "sessions",
    steps: `await __clickText("*", "Token refresh fix"); await __wait(600);`,
  },
  {
    name: "welcome-back",
    steps: `
      await __clickText("button, [role=tab]", "History");
      await __clickText("tr, li, button, [role=button]", "Move sessions to SQLite");`,
  },
  {
    name: "home",
    steps: `await __nav("Home"); await __wait(600);`,
  },
  {
    name: "activities",
    steps: `await __nav("Play");`,
  },
  {
    name: "rest",
    steps: `await __nav("Rest");`,
  },
  {
    name: "game",
    steps: `
      await __nav("Play");
      await __clickText("button", "Start Fog Garden");
      await __until(() => !document.body.innerText.includes("Starting"));
      await __wait(1000);`,
    clicks: TILE_CLICKS,
  },
  {
    name: "game-paused",
    steps: `
      await __nav("Play");
      await __clickText("button", "Start Fog Garden");
      await __until(() => !document.body.innerText.includes("Starting"));
      await __wait(800);`,
    clicks: TILE_CLICKS,
    after: `await lullshiftDemo.attention("claude-code"); await __wait(1200);`,
  },
  {
    name: "companion",
    steps: `
      await __nav("Companion");
      await __wait(800);
      const t = [...document.querySelectorAll("button, a, li")].find((e) => e.textContent.includes("Optimistic locking"));
      t?.click();
      await __wait(800);`,
  },
  {
    name: "integrations",
    steps: `
      await __nav("Settings");
      await __wait(600);
      document.getElementById("integrations")?.scrollIntoView();
      await __wait(300);`,
  },
];

async function main() {
  mkdirSync(OUT, { recursive: true });
  const { send, close } = await launch();
  let failed = 0;
  try {
    for (const scheme of ["light", "dark"]) {
      for (const shot of SHOTS) {
        try {
          const { targetId } = await send("Target.createTarget", { url: "about:blank" });
          const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
          const s = (m, p) => send(m, p, sessionId);
          await s("Page.enable");
          await s("Runtime.enable");
          await s("Emulation.setDeviceMetricsOverride", {
            width: WIDTH,
            height: HEIGHT,
            deviceScaleFactor: 2,
            mobile: false,
          });
          await s("Emulation.setEmulatedMedia", {
            features: [
              { name: "prefers-color-scheme", value: scheme },
              { name: "prefers-reduced-motion", value: "reduce" },
            ],
          });
          await s("Page.navigate", { url: `${BASE}/demo.html` });
          const evaluate = async (code) => {
            const r = await s("Runtime.evaluate", {
              expression: `(async () => { ${HELPERS} ${code} })()`,
              awaitPromise: true,
            });
            if (r.exceptionDetails) {
              throw new Error(
                `${shot.name}: ${r.exceptionDetails.exception?.description ?? r.exceptionDetails.text}`,
              );
            }
          };
          await evaluate(
            `await __until(() => document.querySelector(".nav-item")); await __wait(1200);`,
          );
          await evaluate(shot.steps);
          for (const key of shot.keys ?? []) {
            const code = key.length === 1 ? key : key;
            await s("Input.dispatchKeyEvent", {
              type: "keyDown",
              key,
              code,
              windowsVirtualKeyCode: VK[key] ?? 0,
            });
            await s("Input.dispatchKeyEvent", {
              type: "keyUp",
              key,
              code,
              windowsVirtualKeyCode: VK[key] ?? 0,
            });
            await sleep(150);
          }
          for (const [x, y] of shot.clicks ?? []) {
            await s("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
            await s("Input.dispatchMouseEvent", {
              type: "mousePressed",
              x,
              y,
              button: "left",
              clickCount: 1,
            });
            await s("Input.dispatchMouseEvent", {
              type: "mouseReleased",
              x,
              y,
              button: "left",
              clickCount: 1,
            });
            await sleep(400);
          }
          if (shot.after) await evaluate(shot.after);
          await s("Input.dispatchMouseEvent", { type: "mouseMoved", x: WIDTH - 2, y: HEIGHT - 2 });
          // Move the pointer out of the way and drop focus rings from programmatic clicks.
          await evaluate(`document.activeElement?.blur?.(); await __wait(300);`);
          const { data } = await s("Page.captureScreenshot", { format: "png" });
          const file = path.join(OUT, `${shot.name}-${scheme}.png`);
          writeFileSync(file, Buffer.from(data, "base64"));
          console.log(`wrote ${path.relative(process.cwd(), file)}`);
          await send("Target.closeTarget", { targetId });
        } catch (err) {
          failed++;
          console.error(`failed ${shot.name}-${scheme}: ${err.message ?? err}`);
        }
      }
    }
  } finally {
    await close();
  }
  if (failed) process.exitCode = 1;
}

const VK = {
  ArrowLeft: 37,
  ArrowUp: 38,
  ArrowRight: 39,
  ArrowDown: 40,
  Enter: 13,
  " ": 32,
  Tab: 9,
  Escape: 27,
};

main().catch((err) => {
  console.error(err.message ?? err);
  process.exit(1);
});
