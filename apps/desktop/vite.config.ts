import { createReadStream, existsSync, statSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

const host = process.env.TAURI_DEV_HOST;

const ACTIVITIES_DIR = fileURLToPath(new URL("../../activities", import.meta.url));
const TYPES: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".webp": "image/webp",
};

/**
 * Dev server only: serves the repository's `activities/` folder at `/demo-activities/` so the
 * browser demo (`demo.html`, see `src/demo/`) can run real bundled games and engines in the same
 * sandboxed iframe the app uses. Never part of a build.
 */
function demoActivities(): Plugin {
  return {
    name: "lullshift-demo-activities",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use("/demo-activities", (req, res, next) => {
        const rel = decodeURIComponent((req.url ?? "/").split(/[?#]/)[0] ?? "/");
        const file = path.resolve(ACTIVITIES_DIR, `.${rel}`);
        if (
          !file.startsWith(ACTIVITIES_DIR + path.sep) ||
          !existsSync(file) ||
          !statSync(file).isFile()
        ) {
          next();
          return;
        }
        res.setHeader(
          "Content-Type",
          TYPES[path.extname(file).toLowerCase()] ?? "application/octet-stream",
        );
        res.setHeader("Cache-Control", "no-store");
        createReadStream(file).pipe(res);
      });
    },
  };
}

// Vite options tailored for Tauri: fixed port, don't hide Rust errors, ignore src-tauri.
// The build has two inputs: index.html (the app) and tray.html (the tray window, which never
// loads the app). demo.html, tray-demo.html and src/demo/ are never bundled.
export default defineConfig({
  plugins: [react(), demoActivities()],
  build: {
    rollupOptions: {
      input: {
        main: fileURLToPath(new URL("./index.html", import.meta.url)),
        tray: fileURLToPath(new URL("./tray.html", import.meta.url)),
      },
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
