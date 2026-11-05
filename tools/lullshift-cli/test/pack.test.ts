// S05-A03: identical input produces an identical `.lullpack` hash.
import { cpSync, readFileSync, utimesSync, writeFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { packFiles, readPack } from "../src/pack.ts";
import { validateFolder } from "../src/validate.ts";
import { scaffold, tempDir } from "./helpers.ts";

const pack = (dir: string) => packFiles(validateFolder(dir).files);

describe("pack", () => {
  it("gives the same bytes and hash for the same folder, whatever the file times", () => {
    const a = scaffold();
    const b = path.join(tempDir(), "copy");
    cpSync(a, b, { recursive: true });
    utimesSync(path.join(b, "main.js"), new Date(2001, 1, 1), new Date(2001, 1, 1));
    const pa = pack(a);
    const pb = pack(b);
    expect(pa.sha256).toBe(pb.sha256);
    expect(pa.bytes.equals(pb.bytes)).toBe(true);
    expect(pack(a).sha256).toBe(pa.sha256);
  });

  it("changes the hash when any byte changes", () => {
    const a = scaffold();
    const before = pack(a).sha256;
    writeFileSync(
      path.join(a, "style.css"),
      `${readFileSync(path.join(a, "style.css"), "utf8")}\n`,
    );
    expect(pack(a).sha256).not.toBe(before);
  });

  it("round-trips every file, in sorted order, as a standard ZIP", () => {
    const a = scaffold();
    const files = validateFolder(a).files;
    const back = readPack(pack(a).bytes);
    expect([...back.keys()]).toEqual(files.map((f) => f.rel));
    for (const f of files) expect(back.get(f.rel)?.equals(readFileSync(f.abs))).toBe(true);
  });

  it("doesn't depend on the order files are given in", () => {
    const files = validateFolder(scaffold()).files;
    expect(packFiles([...files].reverse()).sha256).toBe(packFiles(files).sha256);
  });
});
