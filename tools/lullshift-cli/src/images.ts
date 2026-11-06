// Pack images (ADR-016 amendment, S12-T11): the CLI's side of the checks the host makes in
// `crates/pack-media`. Same paths, limits and refusals, worded the same way:
// - the path is `images/<name>.png` or `images/<name>.webp` (lowercase letters, digits, dashes);
// - at most 512 KiB, at most 2048 x 2048 pixels, read from the header before anything is
//   decompressed;
// - the leading bytes match the extension (so an SVG, HTML or script renamed .png is refused);
// - only the chunks a still picture needs: no animation, text, EXIF, XMP, time stamps, colour
//   profiles, unknown chunks or bytes after the end;
// - PNG: every chunk CRC is checked and the pixel data is fully inflated (bounded) and must be
//   exactly the size the header implies, with valid row filters. WebP: the container and
//   picture headers are checked here; the full WebP decode happens in the app and at install
//   (Node has no WebP decoder, and the CLI adds no dependency for one).

import { crc32, inflateSync } from "node:zlib";

export const IMAGE_LIMITS = { count: 20, bytes: 512 * 1024, dimension: 2048 } as const;

const IMAGE_PATH = /^images\/[a-z0-9][a-z0-9-]{0,63}\.(png|webp)$/;
const PNG_SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
const PNG_ALLOWED = new Set(["PLTE", "tRNS", "gAMA", "cHRM", "sRGB", "sBIT", "bKGD", "pHYs"]);
/** Bits per pixel for each PNG colour type and the bit depths it allows. */
const PNG_COLOUR: Record<number, { channels: number; depths: number[] }> = {
  0: { channels: 1, depths: [1, 2, 4, 8, 16] },
  2: { channels: 3, depths: [8, 16] },
  3: { channels: 1, depths: [1, 2, 4, 8] },
  4: { channels: 2, depths: [8, 16] },
  6: { channels: 4, depths: [8, 16] },
};
/** Adam7 passes: x start, y start, x step, y step. */
const ADAM7 = [
  [0, 0, 8, 8],
  [4, 0, 8, 8],
  [0, 4, 4, 8],
  [2, 0, 4, 4],
  [0, 2, 2, 4],
  [1, 0, 2, 2],
  [0, 1, 1, 2],
] as const;

export const isImagePath = (p: string) => IMAGE_PATH.test(p);

export interface ImageInfo {
  kind: "png" | "webp";
  width: number;
  height: number;
}

class Refused extends Error {}
const refuse = (why: string): never => {
  throw new Refused(why);
};

/**
 * Checks one pack image. Returns its size, or a plain reason (worded like the app's) to be shown
 * after the path: `images/a.png is animated; only still pictures are allowed`.
 */
export function checkImage(
  path: string,
  bytes: Buffer,
): { ok: true; info: ImageInfo } | { ok: false; why: string } {
  try {
    const m = IMAGE_PATH.exec(path);
    if (!m)
      refuse(
        "must be a .png or .webp file directly in images/, named with lowercase letters, digits and dashes",
      );
    if (bytes.length > IMAGE_LIMITS.bytes) refuse("is over 512 KiB");
    const info = m![1] === "png" ? checkPng(bytes) : checkWebp(bytes);
    return { ok: true, info };
  } catch (e) {
    if (e instanceof Refused) return { ok: false, why: e.message };
    return { ok: false, why: "can't be decoded" };
  }
}

function checkSize(width: number, height: number) {
  if (width === 0 || height === 0) refuse("is damaged (it has no pixels)");
  if (width > IMAGE_LIMITS.dimension || height > IMAGE_LIMITS.dimension) {
    refuse(
      `is ${width} x ${height} pixels; the most is ${IMAGE_LIMITS.dimension} x ${IMAGE_LIMITS.dimension}`,
    );
  }
}

function checkPng(b: Buffer): ImageInfo {
  if (b.length < 8 || !b.subarray(0, 8).equals(PNG_SIGNATURE))
    refuse("isn't the PNG or WebP image its name says");
  let at = 8;
  let header: { width: number; height: number; depth: number; colour: number; interlace: number };
  header = undefined as never;
  const idat: Buffer[] = [];
  let idatDone = false;
  let palette = false;
  for (let first = true; ; first = false) {
    if (b.length - at < 12) refuse("is damaged (it ends in the middle of a chunk)");
    const len = b.readUInt32BE(at);
    const type = b.toString("latin1", at + 4, at + 8);
    if (len > b.length - at - 12) refuse("is damaged (a chunk is longer than the file)");
    const body = b.subarray(at + 8, at + 8 + len);
    if (crc32(b.subarray(at + 4, at + 8 + len)) !== b.readUInt32BE(at + 8 + len))
      refuse("is damaged (a chunk checksum is wrong)");
    at += 12 + len;
    if (first) {
      if (type !== "IHDR" || len !== 13) refuse("is damaged (it doesn't start with a header)");
      header = {
        width: body.readUInt32BE(0),
        height: body.readUInt32BE(4),
        depth: body[8]!,
        colour: body[9]!,
        interlace: body[12]!,
      };
      checkSize(header.width, header.height);
      const c = PNG_COLOUR[header.colour];
      if (!c || !c.depths.includes(header.depth) || body[10] !== 0 || body[11] !== 0)
        refuse("can't be decoded");
      if (header.interlace > 1) refuse("can't be decoded");
      continue;
    }
    if (type === "IDAT") {
      if (idatDone) refuse("is damaged (its image data is split up)");
      idat.push(body);
      continue;
    }
    if (idat.length) idatDone = true;
    if (type === "IEND") {
      if (!idat.length) refuse("is damaged (it has no image data)");
      if (at !== b.length) refuse("is damaged (it has data after the end)");
      break;
    }
    if (type === "IHDR") refuse("is damaged (it has two headers)");
    if (type === "acTL" || type === "fcTL" || type === "fdAT")
      refuse("is animated; only still pictures are allowed");
    if (type === "tEXt" || type === "zTXt" || type === "iTXt") metadata("text");
    if (type === "eXIf") metadata("EXIF data");
    if (type === "tIME") metadata("a time stamp");
    if (type === "iCCP") metadata("a colour profile");
    if (!PNG_ALLOWED.has(type)) metadata("an unknown chunk");
    if (type === "PLTE") palette = true;
  }
  if (header.colour === 3 && !palette) refuse("can't be decoded");
  // Inflate with a hard cap: one byte more than the header allows proves it is too much.
  const expected = rawSize(header);
  let raw: Buffer;
  try {
    raw = inflateSync(Buffer.concat(idat), { maxOutputLength: expected + 1 });
  } catch {
    return refuse("can't be decoded");
  }
  if (raw.length !== expected) refuse("can't be decoded");
  checkFilters(raw, header);
  return { kind: "png", width: header.width, height: header.height };
}

function metadata(what: string): never {
  return refuse(`carries ${what}; save it without metadata or a colour profile`);
}

type Header = { width: number; height: number; depth: number; colour: number; interlace: number };

/** Row sizes (without the filter byte) of each sub-image, in decode order. */
function rows(h: Header): { count: number; bytes: number }[] {
  const bits = PNG_COLOUR[h.colour]!.channels * h.depth;
  const rowBytes = (w: number) => Math.ceil((w * bits) / 8);
  if (h.interlace === 0) return [{ count: h.height, bytes: rowBytes(h.width) }];
  return ADAM7.map(([x0, y0, dx, dy]) => {
    const w = h.width > x0 ? Math.ceil((h.width - x0) / dx) : 0;
    const n = h.height > y0 ? Math.ceil((h.height - y0) / dy) : 0;
    return w && n ? { count: n, bytes: rowBytes(w) } : { count: 0, bytes: 0 };
  });
}

function rawSize(h: Header): number {
  return rows(h).reduce((sum, r) => sum + r.count * (1 + r.bytes), 0);
}

function checkFilters(raw: Buffer, h: Header) {
  let at = 0;
  for (const r of rows(h)) {
    for (let i = 0; i < r.count; i++) {
      if (raw[at]! > 4) refuse("can't be decoded");
      at += 1 + r.bytes;
    }
  }
}

function checkWebp(b: Buffer): ImageInfo {
  if (
    b.length < 20 ||
    b.toString("latin1", 0, 4) !== "RIFF" ||
    b.toString("latin1", 8, 12) !== "WEBP"
  )
    refuse("isn't the PNG or WebP image its name says");
  if (b.readUInt32LE(4) + 8 !== b.length)
    refuse("is damaged (its RIFF size doesn't match the file)");
  let at = 12;
  let size: [number, number] | undefined;
  let extended = false;
  let bitstream = false;
  for (let index = 0; at < b.length; index++) {
    if (b.length - at < 8) refuse("is damaged (it ends in the middle of a chunk)");
    const type = b.toString("latin1", at, at + 4);
    const len = b.readUInt32LE(at + 4);
    const padded = len + (len & 1);
    if (padded > b.length - at - 8) refuse("is damaged (a chunk is longer than the file)");
    const body = b.subarray(at + 8, at + 8 + len);
    at += 8 + padded;
    const canvas = (w: number, h: number) => {
      if (extended) {
        if (!size || size[0] !== w || size[1] !== h)
          refuse("is damaged (its picture and canvas sizes differ)");
      } else size = [w, h];
    };
    switch (type) {
      case "VP8X": {
        if (index !== 0 || len < 10) refuse("is damaged (its extended header is misplaced)");
        const flags = body[0]!;
        if (flags & 0x02) refuse("is animated; only still pictures are allowed");
        if (flags & 0x20) metadata("a colour profile");
        if (flags & 0x0c) metadata("EXIF or XMP data");
        size = [body.readUIntLE(4, 3) + 1, body.readUIntLE(7, 3) + 1];
        extended = true;
        break;
      }
      case "ALPH":
        if (!extended || bitstream) refuse("is damaged (its alpha chunk is misplaced)");
        break;
      case "VP8 ": {
        if (bitstream || len < 10 || body[3] !== 0x9d || body[4] !== 0x01 || body[5] !== 0x2a)
          refuse("is damaged (it has a broken picture chunk)");
        bitstream = true;
        canvas(body.readUInt16LE(6) & 0x3fff, body.readUInt16LE(8) & 0x3fff);
        break;
      }
      case "VP8L": {
        if (bitstream || len < 5 || body[0] !== 0x2f)
          refuse("is damaged (it has a broken picture chunk)");
        const bits = body.readUInt32LE(1);
        bitstream = true;
        canvas((bits & 0x3fff) + 1, ((bits >>> 14) & 0x3fff) + 1);
        break;
      }
      case "ANIM":
      case "ANMF":
        refuse("is animated; only still pictures are allowed");
        break;
      case "EXIF":
      case "XMP ":
        metadata("EXIF or XMP data");
        break;
      case "ICCP":
        metadata("a colour profile");
        break;
      default:
        metadata("an unknown chunk");
    }
  }
  if (!bitstream || !size) refuse("is damaged (it has no picture)");
  checkSize(size![0], size![1]);
  return { kind: "webp", width: size![0], height: size![1] };
}

/** Every string in `data` that starts with `images/`, in document order. */
export function imageReferences(data: unknown): string[] {
  const out: string[] = [];
  const walk = (v: unknown) => {
    if (typeof v === "string") {
      if (v.startsWith("images/")) out.push(v);
    } else if (Array.isArray(v)) v.forEach(walk);
    else if (v && typeof v === "object") Object.values(v).forEach(walk);
  };
  walk(data);
  return out;
}
