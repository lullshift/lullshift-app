// S12-T11: pictures in content packs. The CLI refuses what the app refuses: unlisted, disguised,
// animated, oversized or metadata-carrying pictures, and data naming unlisted pictures.
import { cpSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { crc32, deflateSync } from "node:zlib";
import { describe, expect, it } from "vitest";
import { checkImage, imageReferences, isImagePath } from "../src/images.ts";
import { servedFiles } from "../src/simulate.ts";
import { validateFolder } from "../src/validate.ts";
import { editJson, tempDir } from "./helpers.ts";

const SHAPES = path.resolve(
  import.meta.dirname,
  "../../../activities/content/quiz/quiz-shapes-and-sides",
);
const PNG = readFileSync(path.join(SHAPES, "images/hexagon.png"));
const WEBP = readFileSync(path.join(SHAPES, "images/pentagon.webp"));

/** A PNG chunk with a correct CRC. */
function chunk(type: string, data: Buffer = Buffer.alloc(0)): Buffer {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "latin1"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body) >>> 0);
  return Buffer.concat([len, body, crc]);
}

/** The bundled PNG with a chunk inserted right after IHDR. */
const withChunk = (c: Buffer) => Buffer.concat([PNG.subarray(0, 33), c, PNG.subarray(33)]);

/** The bundled PNG with its IHDR width and height replaced (CRC fixed up). */
function resized(width: number, height: number): Buffer {
  const out = Buffer.from(PNG);
  out.writeUInt32BE(width, 16);
  out.writeUInt32BE(height, 20);
  out.writeUInt32BE(crc32(out.subarray(12, 29)) >>> 0, 29);
  return out;
}

const why = (p: string, bytes: Buffer) => {
  const r = checkImage(p, bytes);
  return r.ok ? "ok" : r.why;
};

function shapesCopy(): string {
  const dir = path.join(tempDir(), "pack");
  cpSync(SHAPES, dir, { recursive: true });
  return dir;
}

describe("checkImage", () => {
  it("accepts the bundled pictures and reports their size", () => {
    expect(checkImage("images/hexagon.png", PNG)).toEqual({
      ok: true,
      info: { kind: "png", width: 480, height: 300 },
    });
    expect(checkImage("images/pentagon.webp", WEBP)).toMatchObject({
      ok: true,
      info: { kind: "webp" },
    });
  });

  it("refuses bad paths", () => {
    for (const p of [
      "images/../manifest.png",
      "images/a/b.png",
      "Images/a.png",
      "images/A.png",
      "images/a.svg",
      "images/a.gif",
      "a.png",
      "images/.png",
      "/images/a.png",
      "images\\a.png",
    ]) {
      expect(isImagePath(p), p).toBe(false);
      expect(why(p, PNG), p).toMatch(/must be a \.png or \.webp file/);
    }
  });

  it("refuses disguised files: the leading bytes must match the name", () => {
    const svg = Buffer.from('<svg xmlns="http://www.w3.org/2000/svg"><script/></svg>');
    expect(why("images/a.png", svg)).not.toBe("ok");
    expect(why("images/a.webp", svg)).not.toBe("ok");
    expect(why("images/a.png", Buffer.from("<!doctype html><script>x</script>"))).not.toBe("ok");
    expect(why("images/a.webp", PNG)).not.toBe("ok");
    expect(why("images/a.png", WEBP)).not.toBe("ok");
    expect(why("images/a.png", Buffer.alloc(0))).not.toBe("ok");
  });

  it("refuses animation, text and EXIF metadata, unknown chunks and trailing bytes", () => {
    const actl = Buffer.alloc(8);
    actl.writeUInt32BE(1, 0);
    expect(why("images/a.png", withChunk(chunk("acTL", actl)))).toMatch(/animated/);
    expect(why("images/a.png", withChunk(chunk("tEXt", Buffer.from("k\0v"))))).not.toBe("ok");
    expect(why("images/a.png", withChunk(chunk("eXIf", Buffer.from("MM\0*"))))).not.toBe("ok");
    expect(why("images/a.png", withChunk(chunk("zzZz", Buffer.from("x"))))).not.toBe("ok");
    expect(why("images/a.png", Buffer.concat([PNG, Buffer.from("<script>")]))).not.toBe("ok");
  });

  it("refuses data hiding in IDAT after the pixels (extra stream, raw bytes, zero tail)", () => {
    const ihdr = (w: number, h: number) => {
      const d = Buffer.alloc(13);
      d.writeUInt32BE(w, 0);
      d.writeUInt32BE(h, 4);
      d[8] = 8;
      d[9] = 2;
      return chunk("IHDR", d);
    };
    const png = (idat: Buffer) =>
      Buffer.concat([PNG.subarray(0, 8), ihdr(1, 1), chunk("IDAT", idat), chunk("IEND")]);
    const pixels = Buffer.from([0, 1, 2, 3]);
    expect(why("images/a.png", png(deflateSync(pixels)))).toBe("ok");
    // Raw bytes after the zlib stream end.
    expect(
      why("images/a.png", png(Buffer.concat([deflateSync(pixels), Buffer.from("<html>")]))),
    ).toMatch(/after the end of its image data/);
    // A second zlib stream after the first.
    expect(
      why("images/a.png", png(Buffer.concat([deflateSync(pixels), deflateSync(Buffer.from("x"))]))),
    ).not.toBe("ok");
    // A zero tail that inflates to far more than the pixels (checked under a cap).
    const tail = deflateSync(Buffer.concat([pixels, Buffer.alloc(64 * 1024 * 1024)]));
    expect(tail.length).toBeLessThan(512 * 1024);
    expect(why("images/a.png", png(tail))).not.toBe("ok");
  });

  it("refuses a bad CRC and damaged pixel data", () => {
    const bad = Buffer.from(PNG);
    bad[30] = bad[30]! ^ 0xff; // inside IHDR's CRC
    expect(why("images/a.png", bad)).not.toBe("ok");
    const truncated = PNG.subarray(0, PNG.length - 20);
    expect(why("images/a.png", truncated)).not.toBe("ok");
  });

  it("refuses huge dimensions from the header, before inflating anything", () => {
    expect(why("images/a.png", resized(2049, 10))).toMatch(/2048/);
    expect(why("images/a.png", resized(100_000, 100_000))).toMatch(/2048/);
    expect(why("images/a.png", resized(0, 10))).not.toBe("ok");
    // Header says a different size than the pixel data holds.
    expect(why("images/a.png", resized(479, 300))).not.toBe("ok");
  });

  it("refuses files over 512 KiB", () => {
    expect(why("images/a.png", Buffer.concat([PNG, Buffer.alloc(512 * 1024)]))).toMatch(/512 KiB/);
  });

  it("finds image references anywhere in data", () => {
    expect(
      imageReferences({ a: ["images/x.png", { b: "images/y.webp" }], c: "image/z.png", d: 3 }),
    ).toEqual(["images/x.png", "images/y.webp"]);
  });
});

describe("validate (pack pictures)", () => {
  it("passes the bundled picture pack", () => {
    const r = validateFolder(SHAPES);
    expect(r.errors).toEqual([]);
  });

  it("refuses unlisted pictures, missing ones, and data naming unlisted ones", () => {
    const dir = shapesCopy();
    writeFileSync(path.join(dir, "images/extra.png"), PNG);
    rmSync(path.join(dir, "images/octagon.png"));
    editJson(path.join(dir, "quiz.json"), (d) => {
      const qs = d.questions as { image?: { src: string } }[];
      qs[0]!.image!.src = "images/elsewhere.png";
    });
    const errors = validateFolder(dir).errors.join("\n");
    expect(errors).toMatch(/images\/extra\.png: isn't listed in "images"/);
    expect(errors).toMatch(/images\/octagon\.png is listed in "images" but is missing/);
    expect(errors).toMatch(/quiz\.json names "images\/elsewhere\.png", which isn't listed/);
  });

  it("refuses a disguised picture in a pack", () => {
    const dir = shapesCopy();
    writeFileSync(path.join(dir, "images/hexagon.png"), "<svg><script>alert(1)</script></svg>");
    expect(validateFolder(dir).errors.join("\n")).toMatch(/^images\/hexagon\.png /m);
  });

  it("serves the pack's listed pictures, and never an engine's images/ folder", () => {
    const report = validateFolder(SHAPES);
    const served = servedFiles(report).files.map((f) => f.rel);
    expect(served).toContain("index.html");
    for (const image of report.manifest!.images!) expect(served).toContain(image);
    expect(served).not.toContain("quiz.json");
    // Only the pictures come from the pack; everything else is the engine's.
    const fromPack = servedFiles(report).files.filter((f) => f.abs.startsWith(SHAPES));
    expect(fromPack.map((f) => f.rel).sort()).toEqual([...report.manifest!.images!].sort());
  });
});
