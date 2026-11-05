// The dev and simulator servers serve only the validated listing, to loopback callers that use
// the loopback address, and survive hostile requests (Sprint 05 security review).
import { request } from "node:http";
import { rmSync, symlinkSync, writeFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { printable, readListed } from "../src/files.ts";
import { packFiles } from "../src/pack.ts";
import { startServers } from "../src/serve.ts";
import { validateFolder } from "../src/validate.ts";
import { scaffold, tempDir } from "./helpers.ts";

function get(origin: string, rawPath: string, host?: string) {
  const u = new URL(origin);
  return new Promise<{ status: number; body: string }>((resolve, reject) => {
    const req = request(
      {
        host: u.hostname,
        port: u.port,
        path: rawPath,
        headers: host ? { host } : {},
      },
      (res) => {
        let body = "";
        res.on("data", (c: Buffer) => (body += c.toString()));
        res.on("end", () => resolve({ status: res.statusCode ?? 0, body }));
      },
    );
    req.on("error", reject);
    req.end();
  });
}

/** Swaps `file` for a link to `target`; false where links need privileges (Windows). */
function swapForLink(file: string, target: string): boolean {
  rmSync(file);
  try {
    symlinkSync(target, file);
    return true;
  } catch {
    return false;
  }
}

describe("servers", () => {
  it("serve listed files, refuse other hosts and survive a bad escape", async () => {
    const report = validateFolder(scaffold());
    const s = await startServers(report.files, { mode: "dev" });
    try {
      expect((await get(s.activityOrigin, "/manifest.json")).status).toBe(200);
      expect((await get(s.activityOrigin, "/%E0")).status).toBe(404);
      expect((await get(s.activityOrigin, "/../package.json")).status).toBe(404);
      const port = new URL(s.activityOrigin).port;
      expect((await get(s.activityOrigin, "/manifest.json", `evil.test:${port}`)).status).toBe(404);
      const hport = new URL(s.harnessOrigin).port;
      expect((await get(s.harnessOrigin, "/", `evil.test:${hport}`)).status).toBe(403);
      // Still up after all that.
      expect((await get(s.activityOrigin, "/manifest.json")).status).toBe(200);
    } finally {
      await s.close();
    }
  });

  it("won't serve or pack a file swapped for a link after validation", async (ctx) => {
    const dir = scaffold();
    const report = validateFolder(dir);
    const secret = path.join(tempDir(), "secret.txt");
    writeFileSync(secret, "not for the pack");
    if (!swapForLink(path.join(dir, "style.css"), secret)) {
      ctx.skip();
      return;
    }
    expect(() => packFiles(report.files)).toThrow(/plain file|changed/);
    const s = await startServers(report.files, { mode: "dev" });
    try {
      const r = await get(s.activityOrigin, "/style.css");
      expect(r.status).toBe(404);
      expect(r.body).not.toContain("not for the pack");
    } finally {
      await s.close();
    }
  });

  it("reads listed files and keeps escapes out of terminal text", () => {
    const report = validateFolder(scaffold());
    const f = report.files.find((x) => x.rel === "manifest.json")!;
    expect(readListed(f).length).toBe(f.bytes);
    expect(printable("a\u001b]8;;x\u0007b\u009bc")).toBe("a ]8;;x b c");
  });
});
