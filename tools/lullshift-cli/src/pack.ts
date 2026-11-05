// `lullshift pack` (ADR-017, TECHNICAL_ARCHITECTURE §7.5): a deterministic, ZIP-compatible
// `.lullpack`. Entries are stored (no compression), in sorted path order, with a fixed
// timestamp (1980-01-01 00:00), fixed attributes and no extra fields, so identical folders
// always produce identical bytes and the same SHA-256.

import { createHash } from "node:crypto";
import { crc32 } from "node:zlib";
import { readListed, type FileEntry } from "./files.ts";

const DOS_TIME = 0; // 00:00:00
const DOS_DATE = (0 << 9) | (1 << 5) | 1; // 1980-01-01
const VERSION = 20; // 2.0: plain stored files
const UTF8_FLAG = 1 << 11;

export interface Packed {
  bytes: Buffer;
  sha256: string;
}

/** Packs `files` (relative paths with forward slashes) in sorted order. */
export function packFiles(files: readonly Pick<FileEntry, "rel" | "abs">[]): Packed {
  const sorted = [...files].sort((a, b) => (a.rel < b.rel ? -1 : a.rel > b.rel ? 1 : 0));
  const local: Buffer[] = [];
  const central: Buffer[] = [];
  let offset = 0;
  for (const f of sorted) {
    const name = Buffer.from(f.rel, "utf8");
    const data = readListed(f);
    const crc = crc32(data) >>> 0;
    const header = Buffer.alloc(30);
    header.writeUInt32LE(0x04034b50, 0);
    header.writeUInt16LE(VERSION, 4);
    header.writeUInt16LE(UTF8_FLAG, 6);
    header.writeUInt16LE(0, 8); // stored
    header.writeUInt16LE(DOS_TIME, 10);
    header.writeUInt16LE(DOS_DATE, 12);
    header.writeUInt32LE(crc, 14);
    header.writeUInt32LE(data.length, 18);
    header.writeUInt32LE(data.length, 22);
    header.writeUInt16LE(name.length, 26);
    header.writeUInt16LE(0, 28);
    local.push(header, name, data);

    const entry = Buffer.alloc(46);
    entry.writeUInt32LE(0x02014b50, 0);
    entry.writeUInt16LE(VERSION, 4); // made by: MS-DOS, 2.0
    entry.writeUInt16LE(VERSION, 6);
    entry.writeUInt16LE(UTF8_FLAG, 8);
    entry.writeUInt16LE(0, 10);
    entry.writeUInt16LE(DOS_TIME, 12);
    entry.writeUInt16LE(DOS_DATE, 14);
    entry.writeUInt32LE(crc, 16);
    entry.writeUInt32LE(data.length, 20);
    entry.writeUInt32LE(data.length, 24);
    entry.writeUInt16LE(name.length, 28);
    // extra, comment, disk, internal attributes and external attributes stay 0
    entry.writeUInt32LE(offset, 42);
    central.push(entry, name);
    offset += header.length + name.length + data.length;
  }
  const centralBytes = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(sorted.length, 8);
  end.writeUInt16LE(sorted.length, 10);
  end.writeUInt32LE(centralBytes.length, 12);
  end.writeUInt32LE(offset, 16);
  const bytes = Buffer.concat([...local, centralBytes, end]);
  return { bytes, sha256: createHash("sha256").update(bytes).digest("hex") };
}

/** Reads the entry names and contents back (for tests and `pack --check`). Stored entries only. */
export function readPack(bytes: Buffer): Map<string, Buffer> {
  const out = new Map<string, Buffer>();
  const endAt = bytes.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  if (endAt < 0) throw new Error("not a ZIP file");
  const count = bytes.readUInt16LE(endAt + 10);
  let at = bytes.readUInt32LE(endAt + 16);
  for (let i = 0; i < count; i++) {
    if (bytes.readUInt32LE(at) !== 0x02014b50) throw new Error("bad central directory");
    const method = bytes.readUInt16LE(at + 10);
    const size = bytes.readUInt32LE(at + 20);
    const nameLen = bytes.readUInt16LE(at + 28);
    const extra = bytes.readUInt16LE(at + 30);
    const comment = bytes.readUInt16LE(at + 32);
    const localAt = bytes.readUInt32LE(at + 42);
    const name = bytes.subarray(at + 46, at + 46 + nameLen).toString("utf8");
    if (method !== 0) throw new Error(`${name}: compressed entries aren't supported`);
    const localName = bytes.readUInt16LE(localAt + 26);
    const localExtra = bytes.readUInt16LE(localAt + 28);
    const start = localAt + 30 + localName + localExtra;
    out.set(name, bytes.subarray(start, start + size));
    at += 46 + nameLen + extra + comment;
  }
  return out;
}
