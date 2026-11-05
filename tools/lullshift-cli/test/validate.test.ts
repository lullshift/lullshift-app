// S05-A02: invalid manifests, unknown permissions and code inside content packs are rejected;
// plus the folder rules from TECHNICAL_ARCHITECTURE §7.3.
import { spawnSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { compareSemver, validateFolder } from "../src/validate.ts";
import { editJson, scaffold } from "./helpers.ts";

const manifestOf = (dir: string) => path.join(dir, "manifest.json");
const errorsOf = (dir: string) => validateFolder(dir).errors.join("\n");

describe("validate", () => {
  it("passes a fresh app and a fresh content pack (S05-A01)", () => {
    for (const kind of ["app", "content"] as const) {
      const r = validateFolder(scaffold(kind));
      expect(r.errors).toEqual([]);
      expect(r.manifest?.kind).toBe(kind);
    }
  });

  it("rejects invalid manifests", () => {
    const dir = scaffold();
    editJson(manifestOf(dir), (m) => {
      m.offline = false;
      m.id = "Not Reverse DNS";
      m.surprise = true;
    });
    const errors = errorsOf(dir);
    expect(errors).toMatch(/\/offline/);
    expect(errors).toMatch(/\/id/);
    expect(errors).toMatch(/additional properties/);
  });

  it("rejects unknown permissions", () => {
    const dir = scaffold();
    editJson(manifestOf(dir), (m) => {
      m.permissions = ["activity.network"];
    });
    expect(errorsOf(dir)).toMatch(/\/permissions\/0/);
  });

  it("rejects hidden and bidi characters in names", () => {
    const dir = scaffold();
    editJson(manifestOf(dir), (m) => {
      m.name = `Peb${String.fromCodePoint(0x202e)}bles`;
    });
    expect(errorsOf(dir)).toMatch(/\/name/);
  });

  it("rejects code inside content packs", () => {
    const dir = scaffold("content");
    writeFileSync(path.join(dir, "evil.js"), "alert(1)");
    writeFileSync(path.join(dir, "page.html"), "<p>hi</p>");
    const errors = errorsOf(dir);
    expect(errors).toMatch(/evil\.js: content packs can't contain code/);
    expect(errors).toMatch(/page\.html: content packs can't contain code/);
  });

  it("checks pack data against the engine's data schema", () => {
    const dir = scaffold("content");
    writeFileSync(path.join(dir, "deck.json"), JSON.stringify({ title: "x", cards: [], extra: 1 }));
    expect(errorsOf(dir)).toMatch(/deck\.json/);
    editJson(manifestOf(dir), (m) => {
      m.engine = "lullshift.nothing@9";
    });
    expect(errorsOf(dir)).toMatch(/engine lullshift\.nothing@9 wasn't found/);
  });

  it("rejects file types the host won't serve, and unsafe names", () => {
    const dir = scaffold();
    writeFileSync(path.join(dir, "tool.exe"), "MZ");
    writeFileSync(path.join(dir, "-rf.js"), "");
    mkdirSync(path.join(dir, "deep/a/b/c/d/e/f/g"), { recursive: true });
    writeFileSync(path.join(dir, "deep/a/b/c/d/e/f/g/x.js"), "");
    const errors = errorsOf(dir);
    expect(errors).toMatch(/tool\.exe: \.exe files aren't allowed/);
    expect(errors).toMatch(/-rf\.js: name must start/);
    expect(errors).toMatch(/at most 7 levels deep/);
  });

  it("skips hidden files with a warning instead of serving them", () => {
    const dir = scaffold();
    writeFileSync(path.join(dir, ".DS_Store"), "x");
    const r = validateFolder(dir);
    expect(r.errors).toEqual([]);
    expect(r.warnings.join("\n")).toMatch(/\.DS_Store: hidden/);
    expect(r.files.some((f) => f.rel === ".DS_Store")).toBe(false);
  });

  it("refuses inline script, module scripts, inline handlers and remote loads", () => {
    const dir = scaffold();
    writeFileSync(
      path.join(dir, "index.html"),
      [
        "<script>alert(1)</script>",
        '<script type="module" src="main.js"></script>',
        '<button onclick="go()">x</button>',
        '<img src="https://example.com/x.png">',
      ].join("\n"),
    );
    const errors = errorsOf(dir);
    expect(errors).toMatch(/inline <script>/);
    expect(errors).toMatch(/module scripts/);
    expect(errors).toMatch(/inline event handlers/);
    expect(errors).toMatch(/network/);
  });

  it("warns about missing help and preview, and blocked APIs", () => {
    const dir = scaffold();
    editJson(manifestOf(dir), (m) => {
      delete m.help;
      delete m.preview;
    });
    writeFileSync(path.join(dir, "net.js"), "fetch('/x'); new WebSocket('ws://x');");
    const w = validateFolder(dir).warnings.join("\n");
    expect(w).toMatch(/no help/);
    expect(w).toMatch(/no preview/);
    expect(w).toMatch(/net\.js mentions fetch\(\)/);
    expect(w).toMatch(/net\.js mentions WebSocket/);
  });

  it("requires the declared entry point and preview to exist", () => {
    const dir = scaffold();
    rmSync(path.join(dir, "index.html"));
    rmSync(path.join(dir, "preview.svg"));
    const errors = errorsOf(dir);
    expect(errors).toMatch(/index\.html \(the entry point\) is missing/);
    expect(errors).toMatch(/preview preview\.svg is missing/);
  });

  it("orders versions", () => {
    expect(compareSemver("0.2.0", "0.10.0")).toBe(-1);
    expect(compareSemver("1.0.0-beta", "1.0.0")).toBe(-1);
    expect(compareSemver("1.0.0", "1.0.0")).toBe(0);
  });
});

describe("manifest errors", () => {
  it("list the allowed values for an enum", () => {
    const dir = scaffold();
    editJson(manifestOf(dir), (j) => {
      j.category = "puzzle";
    });
    expect(errorsOf(dir)).toMatch(/category .*arcade, break, productivity, interactive/);
  });
});

// S07-A03: breaking a pack's data fails validation with a clear message, from the real command.
describe("validate (content pack data)", () => {
  const cli = path.resolve(import.meta.dirname, "../src/main.ts");

  it("prints plain-language problems with the file and data path", () => {
    const dir = scaffold("content");
    editJson(path.join(dir, "deck.json"), (d) => {
      const cards = d.cards as { front: string }[];
      cards[2]!.front = "x".repeat(201);
      d.colour = "blue";
    });
    const r = spawnSync(process.execPath, [cli, "validate", dir], { encoding: "utf8" });
    expect(r.status).toBe(1);
    expect(r.stdout).toContain("error   deck.json: cards[2].front is longer than 200 characters");
    expect(r.stdout).toContain("error   deck.json: colour isn't a known field");
    expect(r.stdout).not.toMatch(/must NOT|\/cards\/2/);
  });

  it("allows only data, a preview and notes in a pack (ADR-016)", () => {
    const dir = scaffold("content");
    writeFileSync(path.join(dir, "NOTES.md"), "ok");
    expect(validateFolder(dir).errors).toEqual([]);
    writeFileSync(path.join(dir, "photo.png"), "png");
    writeFileSync(path.join(dir, "extra.json"), "{}");
    const errors = errorsOf(dir);
    expect(errors).toMatch(/photo\.png: content packs hold only manifest\.json/);
    expect(errors).toMatch(/extra\.json: content packs hold only manifest\.json/);
  });
});
